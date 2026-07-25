fn main() {
    tauri_build::build();

    // The library test target explicitly links Tauri's generated manifest resource.
    // Keep this as a search path only so application binaries do not link it twice.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out_dir = std::env::var("OUT_DIR").expect("Cargo must set OUT_DIR");
        println!("cargo:rustc-link-search=native={out_dir}");
    }
}
