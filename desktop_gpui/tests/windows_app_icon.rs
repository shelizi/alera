//! Verify the actual PE icon group, not just the presence of an .ico on disk.
//! GPUI's Windows backend loads ID 1 using LoadImageW. Classic's own ID 101
//! is deliberately not the lookup contract for this executable.
#![cfg(all(windows, feature = "native-ui"))]

use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::PathBuf, ptr};

type Handle = *mut c_void;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryExW(path: *const u16, file: Handle, flags: u32) -> Handle;
    fn FreeLibrary(module: Handle) -> i32;
    fn FindResourceW(module: Handle, name: *const u16, kind: *const u16) -> Handle;
    fn LoadResource(module: Handle, resource: Handle) -> Handle;
    fn LockResource(resource: Handle) -> *const u8;
    fn SizeofResource(module: Handle, resource: Handle) -> u32;
}

struct ResourceModule(Handle);

impl ResourceModule {
    fn open(path: PathBuf) -> Self {
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // Map the PE as resources only; do not execute a second application.
        let module = unsafe { LoadLibraryExW(path.as_ptr(), ptr::null_mut(), 0x22) };
        assert!(!module.is_null(), "map native executable as resource data");
        Self(module)
    }

    fn resource(&self, kind: u16, id: u16) -> Vec<u8> {
        unsafe {
            let resource = FindResourceW(self.0, id as usize as _, kind as usize as _);
            assert!(!resource.is_null(), "missing resource type {kind}, ID {id}");
            let length = SizeofResource(self.0, resource) as usize;
            let data = LoadResource(self.0, resource);
            assert!(!data.is_null() && length > 0);
            let bytes = LockResource(data);
            assert!(!bytes.is_null());
            std::slice::from_raw_parts(bytes, length).to_vec()
        }
    }
}

impl Drop for ResourceModule {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.0);
        }
    }
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn u32_at(bytes: &[u8], offset: usize) -> usize {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize
}

#[test]
fn gpui_window_icon_resource_matches_classic_ico_at_every_size() {
    // Cargo supplies the built native executable for integration tests.
    // Standalone resource-link smoke tests can link the build-script output
    // into this test executable itself, without rebuilding the GPUI renderer.
    let path = option_env!("CARGO_BIN_EXE_alera-gpui-desktop")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_exe().unwrap());
    let module = ResourceModule::open(path);
    const RT_GROUP_ICON: u16 = 14;
    const RT_ICON: u16 = 3;
    let group = module.resource(RT_GROUP_ICON, 1);
    let classic = include_bytes!("../../windows/runner/resources/app_icon.ico");
    assert_eq!(&group[..6], &classic[..6]);
    let count = u16_at(classic, 4) as usize;
    assert!(count > 0);
    assert_eq!(group.len(), 6 + count * 14);
    for index in 0..count {
        let group_entry = 6 + index * 14;
        let ico_entry = 6 + index * 16;
        // RC normalizes unspecified planes (0) to 1 for PNG icons.
        // Other directory metadata and the image payload must match exactly.
        assert_eq!(
            &group[group_entry..group_entry + 4],
            &classic[ico_entry..ico_entry + 4],
            "icon dimensions/palette {index}"
        );
        let classic_planes = u16_at(classic, ico_entry + 4);
        assert_eq!(u16_at(&group, group_entry + 4), classic_planes.max(1));
        assert_eq!(
            &group[group_entry + 6..group_entry + 12],
            &classic[ico_entry + 6..ico_entry + 12],
            "icon depth/size {index}"
        );
        let id = u16_at(&group, group_entry + 12);
        let len = u32_at(classic, ico_entry + 8);
        let offset = u32_at(classic, ico_entry + 12);
        assert_eq!(
            module.resource(RT_ICON, id),
            classic[offset..offset + len],
            "icon image {index}"
        );
    }
}
