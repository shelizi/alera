//! Windows Explorer-compatible CF_HDROP file clipboard. GPUI's pinned Windows
//! clipboard writer intentionally ignores ClipboardEntry::ExternalPaths, so
//! writing that entry would NOT support pasting files into Windows Explorer.
#![cfg(windows)]

use crate::explorer_ops::ExplorerClipboardMode;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_HDROP;
use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;

fn io_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

/// DROPFILES is 20 bytes: pFiles(4), POINT(8), fNC(4), fWide(4).
/// Its filename array is a UTF-16 MULTI_SZ ending in two zero code units.
fn drop_files_bytes(paths: &[PathBuf]) -> io::Result<Vec<u8>> {
    if paths.is_empty() || paths.len() > 120 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid file selection",
        ));
    }
    let mut paths_utf16 = Vec::<u16>::new();
    for path in paths {
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Native file clipboard requires absolute paths",
            ));
        }
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Native file clipboard refuses symlinks",
            ));
        }
        let wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
        if wide.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "NUL in filename",
            ));
        }
        paths_utf16.extend(wide);
        paths_utf16.push(0);
    }
    paths_utf16.push(0);
    let mut bytes = vec![0u8; 20 + paths_utf16.len() * 2];
    bytes[..4].copy_from_slice(&20u32.to_le_bytes()); // pFiles
    bytes[16..20].copy_from_slice(&1u32.to_le_bytes()); // fWide = TRUE
    for (i, value) in paths_utf16.iter().enumerate() {
        bytes[20 + i * 2..22 + i * 2].copy_from_slice(&value.to_le_bytes());
    }
    Ok(bytes)
}

struct GlobalClipboardMemory(HGLOBAL);

impl GlobalClipboardMemory {
    fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        // The OS owns this movable block after successful SetClipboardData.
        let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }.map_err(io_error)?;
        let locked = unsafe { GlobalLock(memory) };
        if locked.is_null() {
            unsafe {
                let _ = GlobalFree(Some(memory));
            }
            return Err(io::Error::last_os_error());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), locked.cast::<u8>(), bytes.len());
            let _ = GlobalUnlock(memory);
        }
        Ok(Self(memory))
    }

    fn transfer(self, format: u32) -> io::Result<()> {
        unsafe { SetClipboardData(format, Some(HANDLE(self.0 .0))) }.map_err(io_error)?;
        std::mem::forget(self); // clipboard now owns the HGLOBAL
        Ok(())
    }
}

impl Drop for GlobalClipboardMemory {
    fn drop(&mut self) {
        unsafe {
            let _ = GlobalFree(Some(self.0));
        }
    }
}

struct OpenClipboardGuard;

impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

/// Windows Explorer can paste these files into any folder. Copy creates a
/// second file; Cut requests MOVE via the standard Preferred DropEffect.
pub fn write_native_files(paths: &[PathBuf], mode: ExplorerClipboardMode) -> io::Result<()> {
    let payload = GlobalClipboardMemory::from_bytes(&drop_files_bytes(paths)?)?;
    let drop_effect = GlobalClipboardMemory::from_bytes(
        &(if mode == ExplorerClipboardMode::Cut {
            2u32
        } else {
            1u32
        })
        .to_le_bytes(),
    )?;
    let format = unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) };
    if format == 0 {
        return Err(io::Error::last_os_error());
    }
    // EmptyClipboard with a null owner prevents SetClipboardData from
    // transferring ownership. A UI-triggered Copy/Cut has an active window.
    let owner = unsafe { GetActiveWindow() };
    if owner.0.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "No active Alera clipboard owner",
        ));
    }
    unsafe { OpenClipboard(Some(owner)) }.map_err(io_error)?;
    let _guard = OpenClipboardGuard;
    unsafe { EmptyClipboard() }.map_err(io_error)?;
    payload.transfer(CF_HDROP.0 as u32)?;
    if let Err(error) = drop_effect.transfer(format) {
        unsafe {
            let _ = EmptyClipboard();
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdrop_layout_contains_utf16_multi_sz_and_wide_flag() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("中文 file.txt");
        std::fs::write(&file, "data").unwrap();
        let payload = drop_files_bytes(&[file.clone()]).unwrap();
        assert_eq!(&payload[0..4], &20u32.to_le_bytes());
        assert_eq!(&payload[16..20], &1u32.to_le_bytes());
        let wide = file.as_os_str().encode_wide().collect::<Vec<_>>();
        let encoded = payload[20..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        assert_eq!(&encoded[..wide.len()], wide.as_slice());
        assert_eq!(&encoded[wide.len()..], &[0, 0]);
    }
}
