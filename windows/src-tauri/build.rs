fn main() {
    // Unit-test executables also link native dialogs through screenshot Save As.
    // They do not receive Tauri's application manifest, so request Common Controls
    // v6 explicitly; the legacy DLL has no TaskDialogIndirect entry point.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
    // The exe manifest names the sparse package that lends it an identity (notifications need one).
    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("windows-app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri build script");
}
