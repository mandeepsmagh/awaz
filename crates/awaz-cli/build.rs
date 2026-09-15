use std::{env, path::PathBuf};

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let nemo = root.join("vendor/nemo/link");

    if target_os == "linux" {
        // Release archives keep provider libraries under ./lib. The absolute
        // development path also lets Cargo run test binaries from target/deps.
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/lib:$ORIGIN/../../vendor/moonshine/lib:{}",
            nemo.display()
        );
    } else if target_os == "macos" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", nemo.display());
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/lib");
    }
}
