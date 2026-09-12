use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=src/AppleSpeechBridge.swift");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("Cargo profile directory exists");
    let helper = profile_dir.join("awaz-apple-speech");
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/AppleSpeechBridge.swift");

    let status = Command::new("xcrun")
        .args(["swiftc", "-parse-as-library", "-O", "-warnings-as-errors"])
        .arg(&source)
        .args(["-o"])
        .arg(&helper)
        .status()
        .expect("run xcrun swiftc");
    assert!(status.success(), "failed to compile Apple Speech helper");

    println!(
        "cargo:rustc-env=AWAZ_APPLE_SPEECH_BUILD_HELPER={}",
        helper.display()
    );
}
