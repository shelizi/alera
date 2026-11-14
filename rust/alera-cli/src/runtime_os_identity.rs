//! Non-secret identity of the actual Runtime Host process owner.
//!
//! This is intentionally derived from the OS security token, not HOME,
//! USERPROFILE, USERNAME or a configurable runtime directory. The resulting
//! digest is a local isolation hint, not proof of any Google sign-in.
//! Never return a SID, credential, Windows username or bearer token.

use std::io;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HostOsIdentity {
    pub(crate) kind: String,
    pub(crate) subject_hash: String,
}

impl HostOsIdentity {
    fn from_os_subject(kind: &str, subject: &str) -> io::Result<Self> {
        if !matches!(kind, "windows-token-sid" | "unix-effective-uid")
            || subject.is_empty()
            || subject.len() > 256
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid OS identity",
            ));
        }
        let input = format!("alera-host-os-identity-v1\0{kind}\0{subject}");
        Ok(Self {
            kind: kind.to_owned(),
            subject_hash: hex::encode(Sha256::digest(input.as_bytes())),
        })
    }

    pub(crate) fn validated(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "windows-token-sid" | "unix-effective-uid"
        ) && self.subject_hash.len() == 64
            && self
                .subject_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
    }

    /// Same Windows SID is definitively not two independently keyring-scoped
    /// accounts. Different identities still require a separate authenticated
    /// Host connection and verified Google login before routing any Agent.
    #[cfg(test)]
    pub(crate) fn definitely_same_os_user(&self, other: &Self) -> bool {
        self.validated()
            && other.validated()
            && self.kind == other.kind
            && self.subject_hash == other.subject_hash
    }
}

pub(crate) fn current_host_os_identity() -> io::Result<HostOsIdentity> {
    #[cfg(windows)]
    {
        return HostOsIdentity::from_os_subject("windows-token-sid", &windows_user_sid()?);
    }
    #[cfg(unix)]
    {
        return HostOsIdentity::from_os_subject(
            "unix-effective-uid",
            &unsafe { libc::geteuid() }.to_string(),
        );
    }
    #[cfg(not(any(windows, unix)))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "OS identity is unavailable on this platform",
        ))
    }
}

#[cfg(windows)]
fn windows_user_sid() -> io::Result<String> {
    use std::mem::size_of;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{LocalFree, HANDLE, HLOCAL};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut handle = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) }
        .map_err(io::Error::other)?;
    let token = unsafe { OwnedHandle::from_raw_handle(handle.0) };
    let mut required = 0u32;
    let _ = unsafe {
        GetTokenInformation(
            HANDLE(token.as_raw_handle()),
            TokenUser,
            None,
            0,
            &mut required,
        )
    };
    if required < size_of::<TOKEN_USER>() as u32 || required > 64 * 1024 {
        return Err(io::Error::other("Invalid Windows token identity length"));
    }
    let mut words = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
    unsafe {
        GetTokenInformation(
            HANDLE(token.as_raw_handle()),
            TokenUser,
            Some(words.as_mut_ptr().cast()),
            (words.len() * size_of::<usize>()) as u32,
            &mut required,
        )
    }
    .map_err(io::Error::other)?;
    let user = unsafe { &*words.as_ptr().cast::<TOKEN_USER>() };
    let mut sid = PWSTR::null();
    unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid) }.map_err(io::Error::other)?;
    let text = unsafe { sid.to_string() }.map_err(io::Error::other);
    unsafe { LocalFree(Some(HLOCAL(sid.0.cast()))) };
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_user_cannot_be_mistaken_for_isolated_host() {
        let first = HostOsIdentity::from_os_subject("windows-token-sid", "S-1-5-21-100").unwrap();
        let same = HostOsIdentity::from_os_subject("windows-token-sid", "S-1-5-21-100").unwrap();
        let second = HostOsIdentity::from_os_subject("windows-token-sid", "S-1-5-21-200").unwrap();
        assert!(first.definitely_same_os_user(&same));
        assert!(!first.definitely_same_os_user(&second));
        assert_ne!(first.subject_hash, "S-1-5-21-100");
        assert!(first.validated());
    }

    #[test]
    fn malformed_or_unsupported_identity_is_not_accepted() {
        assert!(HostOsIdentity::from_os_subject("userprofile-env", "x").is_err());
        assert!(HostOsIdentity::from_os_subject("windows-token-sid", "").is_err());
        let forged = HostOsIdentity {
            kind: "windows-token-sid".into(),
            subject_hash: "a".into(),
        };
        assert!(!forged.validated());
        assert!(!forged.definitely_same_os_user(&forged));
    }

    #[test]
    fn current_process_identity_can_be_read_without_credentials() {
        let identity = current_host_os_identity().unwrap();
        assert!(identity.validated());
    }
}
