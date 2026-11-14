//! Windows-only, Native-edition COM activator for Action Center toast clicks.
//! The OS sees an opaque random token. Live authenticated Host identity stays
//! inside the GPUI process and is revalidated at the actual tab-focus boundary.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};
use windows::core::{implement, ComObject, Error, IUnknown, Interface, Ref, Result, GUID, PCWSTR};
use windows::Win32::Foundation::{CLASS_E_NOAGGREGATION, E_ACCESSDENIED};
use windows::Win32::System::Com::{
    CoInitializeEx, CoRegisterClassObject, CoRevokeClassObject, CoUninitialize, IClassFactory,
    IClassFactory_Impl, CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED, REGCLS_MULTIPLEUSE,
};
use windows::Win32::UI::Notifications::{
    INotificationActivationCallback, INotificationActivationCallback_Impl,
    NOTIFICATION_USER_INPUT_DATA,
};
use windows_core::BOOL;

/// Stable per-edition identity, never Classic's COM registration or App ID.
/// Must match PKEY_AppUserModel_ToastActivatorCLSID in the opt-in installer.
pub const NATIVE_ACTIVATOR_CLSID: GUID =
    GUID::from_u128(0x9c44e5c2_8303_4570_a80a_a78b41f532bdu128);

const TOKEN_PREFIX: &str = "alera-native:";
const MAX_QUEUE: usize = 32;

fn activation_queue() -> &'static Mutex<VecDeque<String>> {
    static ACTIVATIONS: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    ACTIVATIONS.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Never accept raw session IDs or arbitrary launch arguments from the OS.
pub fn valid_token(token: &str) -> bool {
    token.len() == TOKEN_PREFIX.len() + 32
        && token.starts_with(TOKEN_PREFIX)
        && token[TOKEN_PREFIX.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
}

pub fn new_token() -> Result<String> {
    let random = GUID::new()?;
    Ok(format!("{TOKEN_PREFIX}{:032x}", random.to_u128()))
}

fn queue_native_activation(app_id: &str, token: &str) -> bool {
    if app_id != crate::edition::NATIVE_WINDOWS_APP_ID || !valid_token(token) {
        return false;
    }
    let Ok(mut queue) = activation_queue().lock() else {
        return false;
    };
    if queue.len() == MAX_QUEUE {
        queue.pop_front();
    }
    queue.push_back(token.to_owned());
    true
}

pub fn take_token() -> Option<String> {
    activation_queue().lock().ok()?.pop_front()
}

#[implement(INotificationActivationCallback)]
struct NativeNotificationCallback;

impl INotificationActivationCallback_Impl for NativeNotificationCallback_Impl {
    fn Activate(
        &self,
        appusermodelid: &PCWSTR,
        invokedargs: &PCWSTR,
        _data: *const NOTIFICATION_USER_INPUT_DATA,
        _count: u32,
    ) -> Result<()> {
        let app_id = unsafe { appusermodelid.to_string()? };
        let args = unsafe { invokedargs.to_string().unwrap_or_default() };
        if queue_native_activation(&app_id, &args) {
            Ok(())
        } else {
            Err(Error::from(E_ACCESSDENIED))
        }
    }
}

#[implement(IClassFactory)]
struct NativeNotificationClassFactory;

impl IClassFactory_Impl for NativeNotificationClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<'_, IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        if !punkouter.is_null() {
            return Err(Error::from(CLASS_E_NOAGGREGATION));
        }
        if ppvobject.is_null() || riid.is_null() {
            return Err(Error::from(E_ACCESSDENIED));
        }
        let callback: INotificationActivationCallback =
            ComObject::new(NativeNotificationCallback).into_interface();
        // QueryInterface addrefs the requested interface; the local variable
        // then drops safely. COM owns the returned reference.
        unsafe { callback.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, _lock: BOOL) -> Result<()> {
        Ok(())
    }
}

/// Register from a dedicated MTA thread. The GPUI UI thread must stay free
/// to render, and an STA test runner waiting in sleep() cannot pump COM calls.
/// All COM registrations are process-lifetime only; this never writes the
/// registry, changes a shortcut or grants native notification ownership.
pub struct NativeToastComRegistration {
    stop_tx: std::sync::mpsc::Sender<()>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl NativeToastComRegistration {
    pub fn register() -> Result<Self> {
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<()>>(1);
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let worker = std::thread::Builder::new()
            .name("alera-native-toast-com".into())
            .spawn(move || {
                if let Err(error) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() } {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
                let factory: IClassFactory =
                    ComObject::new(NativeNotificationClassFactory).into_interface();
                let registered = unsafe {
                    CoRegisterClassObject(
                        &NATIVE_ACTIVATOR_CLSID,
                        &factory,
                        CLSCTX_LOCAL_SERVER,
                        REGCLS_MULTIPLEUSE,
                    )
                };
                match registered {
                    Ok(cookie) => {
                        let _ = ready_tx.send(Ok(()));
                        // MTA COM callbacks are served by COM worker threads,
                        // not by a UI message pump on this parked worker.
                        let _ = stop_rx.recv();
                        unsafe {
                            let _ = CoRevokeClassObject(cookie);
                        }
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                    }
                }
                drop(factory);
                unsafe { CoUninitialize() };
            })
            .map_err(|_| Error::from_hresult(windows::core::HRESULT(0x80004005u32 as i32)))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                stop_tx,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => {
                let _ = worker.join();
                Err(Error::from_hresult(windows::core::HRESULT(
                    0x80004005u32 as i32,
                )))
            }
        }
    }
}

impl Drop for NativeToastComRegistration {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h03_com_activation_accepts_only_bounded_opaque_native_tokens() {
        let token = new_token().expect("Windows random GUID");
        assert!(valid_token(&token));
        assert!(!valid_token("alera-native:host-001"));
        assert!(!valid_token(&format!("{token}\nextra")));
        let callback: INotificationActivationCallback =
            ComObject::new(NativeNotificationCallback).into_interface();
        let app = crate::edition::NATIVE_WINDOWS_APP_ID
            .encode_utf16()
            .chain([0])
            .collect::<Vec<_>>();
        let classic = crate::edition::CLASSIC_WINDOWS_APP_ID
            .encode_utf16()
            .chain([0])
            .collect::<Vec<_>>();
        let launch = token.encode_utf16().chain([0]).collect::<Vec<_>>();
        assert!(
            unsafe { callback.Activate(PCWSTR(classic.as_ptr()), PCWSTR(launch.as_ptr()), &[]) }
                .is_err(),
            "Classic AUMID cannot activate Native"
        );
        assert!(
            unsafe { callback.Activate(PCWSTR(app.as_ptr()), PCWSTR(launch.as_ptr()), &[]) }
                .is_ok()
        );
        assert_eq!(take_token(), Some(token));
        assert_eq!(take_token(), None);
    }

    #[test]
    fn h03_com_class_factory_exposes_native_notification_callback() {
        let factory: IClassFactory =
            ComObject::new(NativeNotificationClassFactory).into_interface();
        // Same type that CoRegisterClassObject will publish for the Native CLSID.
        assert!(!factory.as_raw().is_null());
        let callback: INotificationActivationCallback =
            ComObject::new(NativeNotificationCallback).into_interface();
        assert!(!callback.as_raw().is_null());
    }
}
