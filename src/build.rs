//! Build script (`[package] build = "src/build.rs"` — kept inside `src/` per the repo's
//! no-new-root-paths rule): embeds the brand icon + version metadata into the Windows
//! executable.
#![allow(clippy::print_stdout)]

fn main() {
    println!("cargo:rerun-if-changed=reference/lumagen-brand-pack/windows/lumagen.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("reference/lumagen-brand-pack/windows/lumagen.ico");
        res.set("ProductName", "Lumagen");
        res.set("FileDescription", "Lumagen — PBR material studio");
        if let Err(e) = res.compile() {
            // Missing rc.exe (no Windows SDK) shouldn't fail the build — the app runs
            // fine, the .exe just keeps the default icon.
            println!("cargo:warning=embedding the exe icon failed: {e}");
        }
    }
}
