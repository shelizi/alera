//! Windows ACL hardening for the secret-bearing PTY worker reconnect registry.
//! Never publish a reconnect token under an inherited, potentially public ACL.
//! Fail closed if the current user SID cannot be resolved or the ACL cannot
//! be applied. SYSTEM and local administrators retain normal OS recovery access.

use std::ffi::OsStr;
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;

use windows::core::{BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{LocalFree, HANDLE, HLOCAL};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
    SetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows::Win32::Security::{
    GetSecurityDescriptorDacl, GetTokenInformation, TokenUser, DACL_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

struct LocalDescriptor(PSECURITY_DESCRIPTOR);

impl Drop for LocalDescriptor {
    fn drop(&mut self) {
        if !self.0 .0.is_null() {
            unsafe { LocalFree(Some(HLOCAL(self.0 .0))) };
        }
    }
}

fn current_user_sid() -> io::Result<String> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(io::Error::other)?;
    let token = unsafe { OwnedHandle::from_raw_handle(token.0) };
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
        return Err(io::Error::other("invalid Windows token user SID length"));
    }
    // TOKEN_USER contains pointer-sized fields. Allocate aligned words so it
    // can safely be read as TOKEN_USER after the OS fills the buffer.
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

fn protected_sddl(user_sid: &str, directory: bool) -> String {
    // Protected DACL, no inherited group/everyone/other-user access.
    // Files inherit the parent's protected DACL at creation, and we also
    // explicitly protect the file before it is atomically published.
    let inherit = if directory { "OICI" } else { "" };
    format!("D:P(A;{inherit};FA;;;{user_sid})(A;{inherit};FA;;;SY)(A;{inherit};FA;;;BA)")
}

pub(super) fn protect_path(path: &Path, directory: bool) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink()
        || !if directory {
            meta.is_dir()
        } else {
            meta.is_file()
        }
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "PTY worker secret registry path is not a regular file or directory",
        ));
    }
    // For Windows junctions and other reparse points, is_symlink() alone is
    // insufficient: reject every reparse point before modifying security.
    use std::os::windows::fs::MetadataExt;
    if meta.file_attributes() & 0x400 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "PTY worker secret registry path is a Windows reparse point",
        ));
    }

    let owner = current_user_sid()?;
    let sddl = protected_sddl(&owner, directory);
    let mut wide = OsStr::new(&sddl).encode_wide().collect::<Vec<_>>();
    wide.push(0);
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(wide.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
    }
    .map_err(io::Error::other)?;
    let descriptor = LocalDescriptor(descriptor);
    let mut present = BOOL::default();
    let mut defaulted = BOOL::default();
    let mut acl = std::ptr::null_mut();
    unsafe { GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted) }
        .map_err(io::Error::other)?;
    if !present.as_bool() || acl.is_null() {
        return Err(io::Error::other("PTY worker registry ACL is absent"));
    }
    let mut path_wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    path_wide.push(0);
    let result = unsafe {
        SetNamedSecurityInfoW(
            PWSTR(path_wide.as_mut_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(acl),
            None,
        )
    };
    if result.0 != 0 {
        return Err(io::Error::from_raw_os_error(result.0 as i32));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_dacl_has_no_generic_users_and_inherits_on_directories_only() {
        let sid = "S-1-5-21-1-2-3-1000";
        assert_eq!(
            protected_sddl(sid, false),
            "D:P(A;;FA;;;S-1-5-21-1-2-3-1000)(A;;FA;;;SY)(A;;FA;;;BA)"
        );
        assert!(protected_sddl(sid, true).contains("A;OICI;FA;;;"));
    }

    #[test]
    fn protects_real_windows_directory_and_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let directory = tmp.path().join("worker-tokens");
        std::fs::create_dir(&directory).unwrap();
        protect_path(&directory, true).expect("must protect Worker registry directory");
        let file = directory.join("token.json");
        std::fs::write(&file, b"test-token").unwrap();
        protect_path(&file, false).expect("must protect reconnect secret");
        assert_eq!(std::fs::read(file).unwrap(), b"test-token");
    }
}
