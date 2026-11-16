// The Native workbench composes a large GPUI element tree on the Windows UI
// thread. Windows' default 1 MiB main-thread stack can overflow while GPUI
// constructs the first frame even though the application compiles correctly.
// Reserve stack address space; this does not commit physical RAM until used.
fn main() {
    // Native i18n uses hand-maintained menus as well as generated UI labels.
    println!("cargo:rerun-if-changed=locales");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Classic keeps the canonical desktop icon in windows/runner/resources.
        // Embedding the exact same .ico avoids the generic Rust/GPUI icon in
        // Explorer, taskbar, window switcher and a renamed AleraNative.exe.
        // Keep Native's distinct AppUserModelID; icon parity does not merge
        // the two editions' shortcut or instance identities.
        println!("cargo:rerun-if-changed=native_app_icon.rc");
        println!("cargo:rerun-if-changed=../windows/runner/resources/app_icon.ico");
        embed_resource::compile("native_app_icon.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("Failed to embed Alera Classic icon in Native Windows executable");

        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg-bin=alera-gpui-desktop=/STACK:8388608");
        }
    }
}
