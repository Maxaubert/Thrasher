// Puts the app icon (and the version) into thrasher.exe, for Explorer, the Start menu and
// the taskbar before the window sets its own. Needs rc.exe from the Windows SDK.
fn main() {
    println!("cargo:rerun-if-changed=ui/icon/thrasher.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("ui/icon/thrasher.ico");
        res.compile().expect("compile the Windows resources");
    }
}
