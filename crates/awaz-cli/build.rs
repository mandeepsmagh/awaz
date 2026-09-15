use std::{env, path::PathBuf};

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let profile = env::var("PROFILE").unwrap_or_default();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let nemo = root.join("vendor/nemo/link");

    if target_os == "linux" {
        let mut runpath = String::from(
            "$ORIGIN/lib:$ORIGIN/../../vendor/moonshine/lib:$ORIGIN/../../vendor/nemo/link",
        );
        if profile != "release" {
            // Cargo test binaries run under target/debug/deps, so relative
            // development paths do not reach the workspace root.
            runpath.push(':');
            runpath.push_str(&nemo.to_string_lossy());
        }
        println!("cargo:rustc-link-arg=-Wl,-rpath,{runpath}");
    } else if target_os == "macos" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/lib");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../../vendor/nemo/link");
        if profile != "release" {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", nemo.display());
        }
    }
}
