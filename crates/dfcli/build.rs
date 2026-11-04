//! Embed the version information in the Windows executable (shown by
//! Explorer, and checked when the release is code-signed).
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../packaging/icons/dfstudio.ico");
        res.set("ProductName", "DeltaF Studio");
        res.set("FileDescription", "DeltaF Studio command-line runner");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the Windows version information: {e}");
        }
    }
}
