//! Windows-only, Native-edition COM activator for Action Center toast clicks.
//! The OS sees an opaque random token. Live authenticated Host identity stays
//! inside the GPUI process and is revalidated at the actual tab-focus boundary.

use crate::agent_status_notifications::AgentNotice;
use crate::terminal::AgentActivityState;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use windows::core::{implement, ComObject, Error, IUnknown, Interface, Ref, Result, GUID, PCWSTR};
use windows::Win32::Foundation::{LocalFree, CLASS_E_NOAGGREGATION, E_ACCESSDENIED, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};
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

const ACTIVATION_STORE_VERSION: u32 = 1;
const MAX_ACTIVATION_PAYLOAD_BYTES: usize = 4096;
const MAX_PENDING_ACTIVATIONS: usize = 32;
const ACTIVATION_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedAgentNotice {
    version: u32,
    expires_at_unix_ms: u64,
    session_id: String,
    workspace_id: String,
    tab_id: String,
    agent_type: String,
    state: String,
}

/// Cross-process bridge for COM activation. Only the random launch token is
/// placed in Toast XML; the corresponding Host identity is encrypted with
/// Windows DPAPI (CurrentUser) in the Native per-user state directory.
#[derive(Debug, Clone)]
pub struct NativeToastActivationStore {
    state_dir: PathBuf,
}

impl NativeToastActivationStore {
    pub fn new(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            state_dir: state_dir.into().join("agent-toast-activations"),
        }
    }

    fn token_path(&self, token: &str) -> io::Result<PathBuf> {
        if !valid_token(token) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Native toast activation token",
            ));
        }
        Ok(self
            .state_dir
            .join(format!("{}.dpapi", &token[TOKEN_PREFIX.len()..])))
    }

    pub fn store_notice(&self, token: &str, notice: &AgentNotice) -> io::Result<()> {
        self.store_notice_at(token, notice, unix_time_ms(), ACTIVATION_TTL_MS)
    }

    fn store_notice_at(
        &self,
        token: &str,
        notice: &AgentNotice,
        now_ms: u64,
        ttl_ms: u64,
    ) -> io::Result<()> {
        let path = self.token_path(token)?;
        if !valid_notice(notice) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Agent notice is not safe to persist",
            ));
        }
        let expires_at_unix_ms = now_ms.checked_add(ttl_ms).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Activation expiry overflow")
        })?;
        let record = PersistedAgentNotice {
            version: ACTIVATION_STORE_VERSION,
            expires_at_unix_ms,
            session_id: notice.session_id.clone(),
            workspace_id: notice.workspace_id.clone(),
            tab_id: notice.tab_id.clone(),
            agent_type: notice.agent_type.clone(),
            state: notice.state.label().to_owned(),
        };
        let json = serde_json::to_vec(&record)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let encrypted = dpapi_protect(&json)?;
        if encrypted.len() > MAX_ACTIVATION_PAYLOAD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Encrypted Agent activation payload exceeds limit",
            ));
        }

        fs::create_dir_all(&self.state_dir)?;
        self.prune_expired(now_ms)?;
        let pending = fs::read_dir(&self.state_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "dpapi"))
            .count();
        if pending >= MAX_PENDING_ACTIVATIONS {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Too many pending Native Agent notifications",
            ));
        }

        let temp = path.with_extension(format!("tmp-{}-{now_ms}", std::process::id()));
        let write_result = (|| -> io::Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(&encrypted)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, &path)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        write_result
    }

    pub fn take_notice(&self, token: &str) -> io::Result<Option<AgentNotice>> {
        self.take_notice_at(token, unix_time_ms())
    }

    fn take_notice_at(&self, token: &str, now_ms: u64) -> io::Result<Option<AgentNotice>> {
        let path = self.token_path(token)?;
        let nonce = GUID::new()
            .map(|value| format!("{:032x}", value.to_u128()))
            .unwrap_or_else(|_| format!("{}-{now_ms}", std::process::id()));
        let claimed = path.with_extension(format!("claim-{nonce}"));
        match fs::rename(&path, &claimed) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(None),
            Err(error) => return Err(error),
        }
        let result = (|| -> io::Result<Option<AgentNotice>> {
            let encrypted = fs::read(&claimed)?;
            if encrypted.is_empty() || encrypted.len() > MAX_ACTIVATION_PAYLOAD_BYTES {
                return Ok(None);
            }
            let json = dpapi_unprotect(&encrypted)?;
            let record: PersistedAgentNotice = serde_json::from_slice(&json)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            if record.version != ACTIVATION_STORE_VERSION || record.expires_at_unix_ms <= now_ms {
                return Ok(None);
            }
            let Some(state) = AgentActivityState::parse(&record.state) else {
                return Ok(None);
            };
            let notice = AgentNotice {
                session_id: record.session_id,
                workspace_id: record.workspace_id,
                tab_id: record.tab_id,
                agent_type: record.agent_type,
                state,
            };
            if !valid_notice(&notice) {
                return Ok(None);
            }
            Ok(Some(notice))
        })();
        let _ = fs::remove_file(&claimed);
        result
    }

    pub fn remove_notice(&self, token: &str) -> io::Result<()> {
        let path = self.token_path(token)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Windows may start the LocalServer32 executable without delivering the
    /// toast's COM Activate payload. Only a cold-start explicitly marked as a
    /// toast launch may use this conservative fallback: exactly one pending
    /// Native token must exist. Never guess when multiple notices are pending.
    /// The DPAPI record still needs to pass expiry and typed-identity checks.
    pub fn take_unique_pending_notice(&self) -> io::Result<Option<AgentNotice>> {
        let entries = match fs::read_dir(&self.state_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut selected: Option<String> = None;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path.extension().is_some_and(|ext| ext == "dpapi") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
                return Ok(None);
            };
            let token = format!("{TOKEN_PREFIX}{stem}");
            if !valid_token(&token) || selected.replace(token).is_some() {
                return Ok(None);
            }
        }
        match selected {
            Some(token) => self.take_notice(&token),
            None => Ok(None),
        }
    }

    pub fn contains(&self, token: &str) -> io::Result<bool> {
        Ok(self.token_path(token)?.is_file())
    }

    fn prune_expired(&self, now_ms: u64) -> io::Result<()> {
        let entries = match fs::read_dir(&self.state_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "dpapi") {
                let stale = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                    .map(|modified| modified.as_millis() as u64)
                    .is_some_and(|modified| now_ms.saturating_sub(modified) > ACTIVATION_TTL_MS);
                if stale {
                    let _ = fs::remove_file(path);
                }
            }
        }
        Ok(())
    }
}

fn valid_notice(notice: &AgentNotice) -> bool {
    let valid = |value: &str, max: usize| {
        !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
    };
    valid(&notice.session_id, 128)
        && valid(&notice.workspace_id, 256)
        && valid(&notice.tab_id, 256)
        && valid(&notice.agent_type, 128)
        && notice.state != AgentActivityState::Working
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn dpapi_protect(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes
            .len()
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "DPAPI input too large"))?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error))?;
        let encrypted = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(encrypted)
    }
}

fn dpapi_unprotect(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes
            .len()
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "DPAPI input too large"))?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error))?;
        let plaintext = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(plaintext)
    }
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
    fn h03_activation_store_is_dpapi_encrypted_bounded_expiring_and_one_time() {
        let temp = tempfile::tempdir().expect("isolated Native state");
        let store = NativeToastActivationStore::new(temp.path());
        let token = new_token().expect("opaque token");
        let notice = AgentNotice {
            session_id: "private-session-h03".into(),
            workspace_id: "private-workspace-h03".into(),
            tab_id: "private-tab-h03".into(),
            agent_type: "codex".into(),
            state: AgentActivityState::Waiting,
        };
        store.store_notice(&token, &notice).expect("DPAPI persist");
        let encrypted_file = fs::read_dir(temp.path().join("agent-toast-activations"))
            .expect("activation directory")
            .next()
            .expect("one pending token")
            .expect("token file")
            .path();
        let encrypted = fs::read(&encrypted_file).expect("DPAPI bytes");
        let raw = String::from_utf8_lossy(&encrypted);
        assert!(!raw.contains("private-session-h03"));
        assert!(!raw.contains("private-workspace-h03"));
        assert_eq!(
            store.take_notice(&token).expect("DPAPI decrypt"),
            Some(notice.clone())
        );
        assert_eq!(store.take_notice(&token).expect("one-time consume"), None);
        assert!(!encrypted_file.exists());

        let expired_token = new_token().expect("expired token");
        store
            .store_notice_at(&expired_token, &notice, 100, 10)
            .expect("persist expiring token");
        assert_eq!(
            store
                .take_notice_at(&expired_token, 110)
                .expect("expiry check"),
            None
        );
    }

    #[test]
    fn h03_unique_cold_start_toast_recovers_single_notice_and_refuses_ambiguity() {
        let temp = tempfile::tempdir().expect("isolated Native activation state");
        let store = NativeToastActivationStore::new(temp.path());
        assert_eq!(store.take_unique_pending_notice().unwrap(), None);
        let notice = AgentNotice {
            session_id: "host-h03-fallback".into(),
            workspace_id: "workspace-h03-fallback".into(),
            tab_id: "tab-h03-fallback".into(),
            agent_type: "codex".into(),
            state: AgentActivityState::Waiting,
        };
        let first = new_token().expect("random token");
        store.store_notice(&first, &notice).expect("DPAPI persist");
        assert_eq!(
            store.take_unique_pending_notice().unwrap(),
            Some(notice.clone())
        );
        assert!(!store.contains(&first).unwrap());
        assert_eq!(store.take_unique_pending_notice().unwrap(), None);

        let second = new_token().expect("another token");
        let third = new_token().expect("another token");
        store.store_notice(&second, &notice).unwrap();
        store.store_notice(&third, &notice).unwrap();
        assert_eq!(store.take_unique_pending_notice().unwrap(), None);
        assert!(store.contains(&second).unwrap());
        assert!(store.contains(&third).unwrap());
    }

    /// Child-process endpoint for the cross-process COM roundtrip test.
    #[test]
    fn h03_com_activation_child_client() {
        if std::env::var("ALERA_H03_COM_CHILD").as_deref() != Ok("1") {
            return;
        }
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .expect("child COM MTA");
        }
        let callback: INotificationActivationCallback = unsafe {
            windows::Win32::System::Com::CoCreateInstance(
                &NATIVE_ACTIVATOR_CLSID,
                None::<&IUnknown>,
                CLSCTX_LOCAL_SERVER,
            )
            .expect("cross-process Native COM activation")
        };
        let token = std::env::var("ALERA_H03_COM_TOKEN").expect("test token");
        let app = crate::edition::NATIVE_WINDOWS_APP_ID
            .encode_utf16()
            .chain([0])
            .collect::<Vec<_>>();
        let launch = token.encode_utf16().chain([0]).collect::<Vec<_>>();
        unsafe {
            callback
                .Activate(PCWSTR(app.as_ptr()), PCWSTR(launch.as_ptr()), &[])
                .expect("cross-process INotificationActivationCallback::Activate");
            CoUninitialize();
        }
    }

    #[test]
    fn h03_com_class_factory_exposes_native_notification_callback() {
        let factory: IClassFactory =
            ComObject::new(NativeNotificationClassFactory).into_interface();
        assert!(!factory.as_raw().is_null());
        let callback: INotificationActivationCallback =
            ComObject::new(NativeNotificationCallback).into_interface();
        assert!(!callback.as_raw().is_null());
    }

    #[test]
    fn h03_com_registration_accepts_a_separate_process_client() {
        if std::env::var("ALERA_H03_COM_CHILD").as_deref() == Ok("1") {
            return;
        }
        let registration = NativeToastComRegistration::register()
            .expect("parent Native COM LocalServer registration");
        let temp = tempfile::tempdir().expect("isolated per-user activation state");
        let store = NativeToastActivationStore::new(temp.path());
        let token = new_token().expect("opaque token");
        let notice = AgentNotice {
            session_id: "h03-cross-process-session".into(),
            workspace_id: "h03-cross-process-workspace".into(),
            tab_id: "h03-cross-process-tab".into(),
            agent_type: "codex".into(),
            state: AgentActivityState::Waiting,
        };
        store
            .store_notice(&token, &notice)
            .expect("DPAPI token mapping");
        let child = std::process::Command::new(std::env::current_exe().expect("test exe"))
            .args([
                "--exact",
                "native_toast_activation::tests::h03_com_activation_child_client",
                "--nocapture",
            ])
            .env("ALERA_H03_COM_CHILD", "1")
            .env("ALERA_H03_COM_TOKEN", &token)
            .output()
            .expect("spawn isolated COM client process");
        assert!(
            child.status.success(),
            "child COM client failed: stdout={} stderr={}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        let activated_token = take_token().expect("cross-process COM callback token");
        assert_eq!(activated_token, token);
        assert_eq!(
            store
                .take_notice(&activated_token)
                .expect("DPAPI activation lookup"),
            Some(notice)
        );
        drop(registration);
    }
}
