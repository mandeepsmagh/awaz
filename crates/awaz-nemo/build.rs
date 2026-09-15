use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=AWAZ_NEMO_LIB_DIR");
    println!("cargo:rerun-if-changed=../../vendor/nemo/link");

    let candidate = env::var_os("AWAZ_NEMO_LIB_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/nemo/link");
            path.exists().then_some(path)
        });

    let Some(lib_dir) = candidate else {
        println!(
            "cargo:warning=NeMo Speech native library not configured. Run scripts/fetch-nemo-runtime.sh or set AWAZ_NEMO_LIB_DIR before linking awaz."
        );
        return;
    };

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=nemo_speech_asr_c");

    match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
            println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/lib");
        }
        Ok("linux") => {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
            println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/lib");
        }
        _ => {}
    }
}
