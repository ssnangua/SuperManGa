fn main() {
    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    // rustc incremental often ignores CARGO_PKG_VERSION alone; rustc-env
    // changes the compiler command line so About / update checks rebuild.
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rustc-env=APP_VERSION={version}");
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
