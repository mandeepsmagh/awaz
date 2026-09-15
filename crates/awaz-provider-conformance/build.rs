use std::{env, path::PathBuf};

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "linux" && target_os != "macos" {
        return;
    }

    let lib_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/nemo/link");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
}
