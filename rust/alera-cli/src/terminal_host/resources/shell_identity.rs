//! Windows PTY resource identity from the already-owned child handle.
//! The Unix-seconds field stays compatible with existing sysinfo consumers.
use super::process_tree::ShellProcess;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use windows::Win32::Foundation::{FILETIME, HANDLE, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{
    GetProcessId, GetProcessTimes, OpenProcess, WaitForSingleObject,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};

fn live_creation(pid: u32, handle: HANDLE) -> Option<u64> {
    if pid == 0 || handle.is_invalid() || unsafe { GetProcessId(handle) } != pid {
        return None;
    }
    let (mut creation, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) }.ok()?;
    // Nonsignaled state, unlike a numeric STILL_ACTIVE exit code, cannot
    // confuse a completed process with a living process.
    if unsafe { WaitForSingleObject(handle, 0) } != WAIT_TIMEOUT {
        return None;
    }
    Some((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

/// Read the original process object. The caller owns and retains the handle.
pub fn seal_shell_from_handle(pid: u32, handle: RawHandle) -> Option<ShellProcess> {
    ShellProcess::from_windows_creation_time(pid, live_creation(pid, HANDLE(handle))?)
}

/// Revalidate a previously captured exact marker. No termination access.
pub(super) fn matches_live_creation(shell: ShellProcess) -> bool {
    let Some(expected) = shell.creation_filetime() else {
        return true;
    };
    let Ok(raw) = (unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            shell.pid,
        )
    }) else {
        return false;
    };
    // SAFETY: successful OpenProcess transfers one owned handle.
    let handle = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    live_creation(shell.pid, HANDLE(handle.as_raw_handle())) == Some(expected)
}

/// Restore an exact marker supplied by a private Worker registry. A
/// missing/inaccessible/mismatched process remains unmeasured; never mint
/// a new precise identity by observing a recycled PID.
pub fn recover_precise_shell(pid: u32, creation: u64) -> Option<ShellProcess> {
    let sealed = ShellProcess::from_windows_creation_time(pid, creation)?;
    matches_live_creation(sealed).then_some(sealed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::process::{Child, Command, Stdio};

    #[test]
    fn precise_marker_preserves_unix_seconds_but_distinguishes_same_second() {
        let creation = (11_644_473_600 + 1000) * 10_000_000 + 1;
        let first = ShellProcess::from_windows_creation_time(10, creation).unwrap();
        let second = ShellProcess::from_windows_creation_time(10, creation + 1).unwrap();
        assert_eq!(first.start_time, 1000);
        assert_eq!(first.start_time, second.start_time);
        assert_ne!(first.creation_filetime(), second.creation_filetime());
        assert_eq!(ShellProcess::coarse(10, 1000).creation_filetime(), None);
        assert!(ShellProcess::from_windows_creation_time(0, creation).is_none());
        assert!(ShellProcess::from_windows_creation_time(10, 0).is_none());
        assert!(
            ShellProcess::from_windows_creation_time(10, 11_644_473_600 * 10_000_000).is_none()
        );
    }

    #[test]
    #[ignore = "private helper for the handle identity regression"]
    fn hold_identity_child() {
        if std::env::var_os("ALERA_TEST_HOLD_IDENTITY_CHILD").as_deref()
            == Some(std::ffi::OsStr::new("1"))
        {
            let _ = std::io::stdin().read(&mut [0u8]);
        }
    }

    #[test]
    fn original_child_handle_and_recovery_reject_wrong_creation() {
        struct ChildGuard(Child);
        impl Drop for ChildGuard {
            fn drop(&mut self) {
                drop(self.0.stdin.take());
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let name = format!(
            "{}::hold_identity_child",
            module_path!().split_once("::").unwrap().1
        );
        let mut child = ChildGuard(
            Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", &name])
                .env("ALERA_TEST_HOLD_IDENTITY_CHILD", "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("private child"),
        );
        let pid = child.0.id();
        let sealed =
            seal_shell_from_handle(pid, child.0.as_raw_handle()).expect("original child handle");
        let creation = sealed.creation_filetime().unwrap();
        assert!(sealed.start_time > 0);
        assert_eq!(recover_precise_shell(pid, creation), Some(sealed));
        assert!(seal_shell_from_handle(pid.wrapping_add(1), child.0.as_raw_handle()).is_none());
        let wrong = if creation % 10_000_000 == 9_999_999 {
            creation - 1
        } else {
            creation + 1
        };
        assert_eq!(
            ShellProcess::from_windows_creation_time(pid, wrong)
                .unwrap()
                .start_time,
            sealed.start_time
        );
        assert!(recover_precise_shell(pid, wrong).is_none());
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        assert!(seal_shell_from_handle(pid, child.0.as_raw_handle()).is_none());
        assert!(recover_precise_shell(pid, creation).is_none());
    }
}
