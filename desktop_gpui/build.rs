// The Native workbench composes a large GPUI element tree on the Windows UI
// thread. Windows' default 1 MiB main-thread stack can overflow while GPUI
// constructs the first frame even though the application compiles correctly.
// Reserve stack address space; this does not commit physical RAM until used.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bin=alera-gpui-desktop=/STACK:8388608");
    }
}
