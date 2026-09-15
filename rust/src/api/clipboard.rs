#[cfg(any(not(target_os = "linux"), test))]
use std::fs;
#[cfg(any(not(target_os = "linux"), test))]
use std::io::{BufWriter, Write};
#[cfg(not(target_os = "linux"))]
use std::path::Path;
#[cfg(any(not(target_os = "linux"), test))]
use std::path::PathBuf;
#[cfg(not(target_os = "linux"))]
use std::time::{Duration, SystemTime};

#[cfg(not(target_os = "linux"))]
use arboard::{Clipboard, Error as ArboardError};
#[cfg(any(not(target_os = "linux"), test))]
use tempfile::Builder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileClipboardOperation {
    Copy,
    Cut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileClipboardPayload {
    pub paths: Vec<String>,
    pub operation: FileClipboardOperation,
    pub sequence_number: u32,
}

pub fn set_file_clipboard(
    paths: Vec<String>,
    operation: FileClipboardOperation,
) -> Result<u32, String> {
    #[cfg(target_os = "windows")]
    {
        windows_file_clipboard::set_file_clipboard(paths, operation)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (paths, operation);
        Err("Native file clipboard integration is only available on Windows.".to_string())
    }
}

pub fn read_file_clipboard() -> Result<Option<FileClipboardPayload>, String> {
    #[cfg(target_os = "windows")]
    {
        windows_file_clipboard::read_file_clipboard()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(None)
    }
}

pub fn file_clipboard_sequence_number() -> u32 {
    #[cfg(target_os = "windows")]
    {
        windows_file_clipboard::sequence_number()
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

pub fn clear_file_clipboard_if_sequence(sequence_number: u32) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        windows_file_clipboard::clear_if_sequence(sequence_number)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = sequence_number;
        Ok(false)
    }
}

#[cfg(any(not(target_os = "linux"), test))]
const CLIPBOARD_IMAGE_MAX_PIXELS: usize = 32 * 1024 * 1024;
#[cfg(any(not(target_os = "linux"), test))]
const CLIPBOARD_IMAGE_MAX_PNG_BYTES: u64 = 18 * 1024 * 1024;
#[cfg(not(target_os = "linux"))]
const CLIPBOARD_IMAGE_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
#[cfg(any(not(target_os = "linux"), test))]
const CLIPBOARD_IMAGE_PREFIX: &str = "alera-paste-";

#[cfg(any(target_os = "windows", test))]
fn dropfiles_bytes(paths: &[String]) -> Result<Vec<u8>, String> {
    if paths.is_empty() {
        return Err("File clipboard requires at least one path.".to_string());
    }

    const DROPFILES_HEADER_BYTES: usize = 20;
    let mut encoded_paths = Vec::<u16>::new();
    for path in paths {
        if path.is_empty() || path.contains('\0') {
            return Err("File clipboard contains an invalid path.".to_string());
        }
        encoded_paths.extend(path.encode_utf16());
        encoded_paths.push(0);
    }
    encoded_paths.push(0);

    let mut bytes = vec![0_u8; DROPFILES_HEADER_BYTES + encoded_paths.len() * 2];
    bytes[0..4].copy_from_slice(&(DROPFILES_HEADER_BYTES as u32).to_le_bytes());
    bytes[16..20].copy_from_slice(&1_i32.to_le_bytes());
    for (index, value) in encoded_paths.into_iter().enumerate() {
        let offset = DROPFILES_HEADER_BYTES + index * 2;
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    Ok(bytes)
}

#[cfg(target_os = "windows")]
mod windows_file_clipboard {
    use super::{dropfiles_bytes, FileClipboardOperation, FileClipboardPayload};
    use std::ffi::c_void;
    use std::path::Path;
    use std::ptr;
    use std::thread;
    use std::time::Duration;

    type Handle = *mut c_void;
    type HGlobal = *mut c_void;
    type HDrop = *mut c_void;
    type HWnd = *mut c_void;

    const CF_HDROP: u32 = 15;
    const GMEM_MOVEABLE: u32 = 0x0002;
    const GMEM_ZEROINIT: u32 = 0x0040;
    const DROPEFFECT_COPY: u32 = 1;
    const DROPEFFECT_MOVE: u32 = 2;
    const QUERY_FILE_COUNT: u32 = 0xffff_ffff;
    const OPEN_CLIPBOARD_ATTEMPTS: usize = 8;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(owner: HWnd) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn SetClipboardData(format: u32, memory: Handle) -> Handle;
        fn GetClipboardData(format: u32) -> Handle;
        fn IsClipboardFormatAvailable(format: u32) -> i32;
        fn RegisterClipboardFormatW(format: *const u16) -> u32;
        fn GetClipboardSequenceNumber() -> u32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> HGlobal;
        fn GlobalLock(memory: HGlobal) -> *mut c_void;
        fn GlobalUnlock(memory: HGlobal) -> i32;
        fn GlobalFree(memory: HGlobal) -> HGlobal;
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn DragQueryFileW(drop: HDrop, index: u32, buffer: *mut u16, length: u32) -> u32;
    }

    pub(super) fn set_file_clipboard(
        paths: Vec<String>,
        operation: FileClipboardOperation,
    ) -> Result<u32, String> {
        for path in &paths {
            let path = Path::new(path);
            if !path.is_absolute() || !path.exists() {
                return Err(format!(
                    "File clipboard path does not exist: {}",
                    path.display()
                ));
            }
        }

        let dropfiles = dropfiles_bytes(&paths)?;
        let effect = match operation {
            FileClipboardOperation::Copy => DROPEFFECT_COPY,
            FileClipboardOperation::Cut => DROPEFFECT_MOVE,
        };
        let preferred_drop_effect = preferred_drop_effect_format()?;
        let drop_handle = global_memory(&dropfiles)?;
        let effect_handle = match global_memory(&effect.to_le_bytes()) {
            Ok(handle) => handle,
            Err(error) => {
                free_global(drop_handle);
                return Err(error);
            }
        };

        with_open_clipboard(|| {
            if unsafe { EmptyClipboard() } == 0 {
                free_global(drop_handle);
                free_global(effect_handle);
                return Err("Could not empty the Windows clipboard.".to_string());
            }
            if unsafe { SetClipboardData(CF_HDROP, drop_handle) }.is_null() {
                free_global(drop_handle);
                free_global(effect_handle);
                return Err("Could not write file paths to the Windows clipboard.".to_string());
            }
            if unsafe { SetClipboardData(preferred_drop_effect, effect_handle) }.is_null() {
                free_global(effect_handle);
                unsafe {
                    EmptyClipboard();
                }
                return Err("Could not write the file clipboard operation.".to_string());
            }
            Ok(unsafe { GetClipboardSequenceNumber() })
        })
    }

    pub(super) fn read_file_clipboard() -> Result<Option<FileClipboardPayload>, String> {
        with_open_clipboard(|| {
            if unsafe { IsClipboardFormatAvailable(CF_HDROP) } == 0 {
                return Ok(None);
            }
            let drop_handle = unsafe { GetClipboardData(CF_HDROP) };
            if drop_handle.is_null() {
                return Err("Could not read file paths from the Windows clipboard.".to_string());
            }

            let count =
                unsafe { DragQueryFileW(drop_handle, QUERY_FILE_COUNT, ptr::null_mut(), 0) };
            if count == 0 {
                return Ok(None);
            }
            let mut paths = Vec::with_capacity(count as usize);
            for index in 0..count {
                let length = unsafe { DragQueryFileW(drop_handle, index, ptr::null_mut(), 0) };
                if length == 0 {
                    continue;
                }
                let mut buffer = vec![0_u16; length as usize + 1];
                let copied = unsafe {
                    DragQueryFileW(drop_handle, index, buffer.as_mut_ptr(), buffer.len() as u32)
                };
                if copied == 0 {
                    return Err("Could not decode a file clipboard path.".to_string());
                }
                paths.push(String::from_utf16_lossy(&buffer[..copied as usize]));
            }
            if paths.is_empty() {
                return Ok(None);
            }

            Ok(Some(FileClipboardPayload {
                paths,
                operation: read_preferred_operation()?,
                sequence_number: unsafe { GetClipboardSequenceNumber() },
            }))
        })
    }

    pub(super) fn sequence_number() -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }

    pub(super) fn clear_if_sequence(expected: u32) -> Result<bool, String> {
        if expected == 0 {
            return Ok(false);
        }
        with_open_clipboard(|| {
            if unsafe { GetClipboardSequenceNumber() } != expected {
                return Ok(false);
            }
            if unsafe { EmptyClipboard() } == 0 {
                return Err("Could not clear the Windows clipboard.".to_string());
            }
            Ok(true)
        })
    }

    fn read_preferred_operation() -> Result<FileClipboardOperation, String> {
        let format = preferred_drop_effect_format()?;
        if unsafe { IsClipboardFormatAvailable(format) } == 0 {
            return Ok(FileClipboardOperation::Copy);
        }
        let handle = unsafe { GetClipboardData(format) };
        if handle.is_null() {
            return Ok(FileClipboardOperation::Copy);
        }
        let pointer = unsafe { GlobalLock(handle) };
        if pointer.is_null() {
            return Err("Could not read the file clipboard operation.".to_string());
        }
        let effect = unsafe { ptr::read_unaligned(pointer.cast::<u32>()) };
        unsafe {
            GlobalUnlock(handle);
        }
        Ok(if effect & DROPEFFECT_MOVE != 0 {
            FileClipboardOperation::Cut
        } else {
            FileClipboardOperation::Copy
        })
    }

    fn preferred_drop_effect_format() -> Result<u32, String> {
        let mut wide = "Preferred DropEffect".encode_utf16().collect::<Vec<_>>();
        wide.push(0);
        let format = unsafe { RegisterClipboardFormatW(wide.as_ptr()) };
        if format == 0 {
            Err("Could not register the Windows file clipboard operation format.".to_string())
        } else {
            Ok(format)
        }
    }

    fn with_open_clipboard<T>(operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let mut opened = false;
        for attempt in 0..OPEN_CLIPBOARD_ATTEMPTS {
            if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
                opened = true;
                break;
            }
            if attempt + 1 < OPEN_CLIPBOARD_ATTEMPTS {
                thread::sleep(Duration::from_millis(5));
            }
        }
        if !opened {
            return Err("The Windows clipboard is busy.".to_string());
        }

        let result = operation();
        let closed = unsafe { CloseClipboard() } != 0;
        if !closed && result.is_ok() {
            return Err("Could not close the Windows clipboard.".to_string());
        }
        result
    }

    fn global_memory(bytes: &[u8]) -> Result<HGlobal, String> {
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len()) };
        if handle.is_null() {
            return Err("Could not allocate Windows clipboard memory.".to_string());
        }
        let pointer = unsafe { GlobalLock(handle) };
        if pointer.is_null() {
            free_global(handle);
            return Err("Could not lock Windows clipboard memory.".to_string());
        }
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast::<u8>(), bytes.len());
            GlobalUnlock(handle);
        }
        Ok(handle)
    }

    fn free_global(handle: HGlobal) {
        if !handle.is_null() {
            unsafe {
                GlobalFree(handle);
            }
        }
    }
}

/// Saves an image-only clipboard payload as a private temporary PNG.
///
/// Returns `Ok(None)` when the clipboard has no image representation. The
/// bridge runs this synchronous function off the Flutter UI isolate.
pub fn save_clipboard_image_as_temp_file() -> Result<Option<String>, String> {
    #[cfg(target_os = "linux")]
    {
        Err("Linux clipboard images are handled by the GTK runner.".to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut clipboard = Clipboard::new().map_err(clipboard_error)?;
        let image = match clipboard.get_image() {
            Ok(image) => image,
            Err(ArboardError::ContentNotAvailable) => return Ok(None),
            Err(error) => return Err(clipboard_error(error)),
        };
        cleanup_expired_clipboard_images();
        let path = write_clipboard_png(image.width, image.height, image.bytes.as_ref())?;
        Ok(Some(path.to_string_lossy().into_owned()))
    }
}

#[cfg(not(target_os = "linux"))]
fn clipboard_error(error: ArboardError) -> String {
    format!("Clipboard image unavailable: {error}")
}

#[cfg(any(not(target_os = "linux"), test))]
fn write_clipboard_png(width: usize, height: usize, bytes: &[u8]) -> Result<PathBuf, String> {
    let pixel_count = width
        .checked_mul(height)
        .ok_or_else(|| "Clipboard image is too large.".to_string())?;
    if width == 0 || height == 0 || pixel_count > CLIPBOARD_IMAGE_MAX_PIXELS {
        return Err("Clipboard image is too large.".to_string());
    }
    let expected_bytes = pixel_count
        .checked_mul(4)
        .ok_or_else(|| "Clipboard image is too large.".to_string())?;
    if bytes.len() != expected_bytes {
        return Err("Clipboard image data is invalid.".to_string());
    }

    let mut file = Builder::new()
        .prefix(CLIPBOARD_IMAGE_PREFIX)
        .suffix(".png")
        .tempfile()
        .map_err(|error| format!("Could not create clipboard image: {error}"))?;
    {
        let mut encoder = png::Encoder::new(
            BufWriter::new(file.as_file_mut()),
            width as u32,
            height as u32,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("Could not encode clipboard image: {error}"))?;
        writer
            .write_image_data(bytes)
            .map_err(|error| format!("Could not encode clipboard image: {error}"))?;
        writer
            .finish()
            .map_err(|error| format!("Could not encode clipboard image: {error}"))?;
    }
    file.as_file_mut()
        .flush()
        .map_err(|error| format!("Could not save clipboard image: {error}"))?;
    let encoded_bytes = file
        .as_file()
        .metadata()
        .map_err(|error| format!("Could not inspect clipboard image: {error}"))?
        .len();
    if encoded_bytes > CLIPBOARD_IMAGE_MAX_PNG_BYTES {
        return Err("Clipboard image is too large.".to_string());
    }
    file.into_temp_path()
        .keep()
        .map_err(|error| format!("Could not retain clipboard image: {error}"))
}

#[cfg(not(target_os = "linux"))]
fn cleanup_expired_clipboard_images() {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_expired_clipboard_image(&path) {
            continue;
        }
        let _ = fs::remove_file(path);
    }
}

#[cfg(not(target_os = "linux"))]
fn is_expired_clipboard_image(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    if !name.starts_with(CLIPBOARD_IMAGE_PREFIX) || !name.ends_with(".png") {
        return false;
    }
    let Ok(modified) = path.metadata().and_then(|metadata| metadata.modified()) else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|age| age > CLIPBOARD_IMAGE_MAX_AGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_private_png_temp_file() {
        let path = write_clipboard_png(1, 1, &[255, 0, 0, 255]).expect("png");
        let bytes = fs::read(&path).expect("read png");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o077, 0);
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_invalid_or_oversized_images() {
        assert!(write_clipboard_png(1, 1, &[0, 0, 0]).is_err());
        assert!(write_clipboard_png(CLIPBOARD_IMAGE_MAX_PIXELS + 1, 1, &[]).is_err());
    }

    #[test]
    fn dropfiles_payload_uses_wide_double_null_terminated_paths() {
        let bytes = dropfiles_bytes(&[r"C:\one.txt".to_string(), r"D:\two".to_string()])
            .expect("dropfiles");
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 20);
        assert_eq!(i32::from_le_bytes(bytes[16..20].try_into().unwrap()), 1);

        let values = bytes[20..]
            .chunks_exact(2)
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
            .collect::<Vec<_>>();
        let expected = [r"C:\one.txt", r"D:\two"]
            .into_iter()
            .flat_map(|path| path.encode_utf16().chain(std::iter::once(0)))
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        assert_eq!(values, expected);
    }

    #[test]
    fn dropfiles_payload_rejects_empty_or_nul_paths() {
        assert!(dropfiles_bytes(&[]).is_err());
        assert!(dropfiles_bytes(&[String::new()]).is_err());
        assert!(dropfiles_bytes(&["bad\0path".to_string()]).is_err());
    }
}
