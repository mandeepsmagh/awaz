use std::{env, ffi::OsStr, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=AWAZ_MOONSHINE_LIB_DIR");
    println!("cargo:rerun-if-changed=../../vendor/moonshine/lib");

    let candidate = env::var_os("AWAZ_MOONSHINE_LIB_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/moonshine/lib");
            path.exists().then_some(path)
        });

    let Some(lib_dir) = candidate else {
        println!(
            "cargo:warning=Moonshine native library not configured. Run scripts/fetch-moonshine-runtime.sh or set AWAZ_MOONSHINE_LIB_DIR before linking awaz."
        );
        return;
    };

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "windows" {
        // Moonshine's Windows bundle may contain multiple import/static libraries.
        // Upstream's portable integration instructs consumers to link every .lib.
        let mut libraries = Vec::new();
        if let Ok(entries) = fs::read_dir(&lib_dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension() != Some(OsStr::new("lib")) {
                    continue;
                }
                if let Some(stem) = path.file_stem().and_then(OsStr::to_str) {
                    libraries.push(stem.to_owned());
                }
            }
        }
        libraries.sort();
        libraries.dedup();
        for library in libraries {
            println!("cargo:rustc-link-lib={library}");
        }
        println!("cargo:rustc-link-lib=ole32");
        println!("cargo:rustc-link-lib=mmdevapi");
    } else {
        println!("cargo:rustc-link-lib=moonshine");
    }

    if target_os == "macos" {
        println!("cargo:rustc-link-lib=c++");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=Foundation");
        link_macos_clang_runtime();
    }
}

fn link_macos_clang_runtime() {
    let Ok(output) = Command::new("clang").arg("--print-resource-dir").output() else {
        println!("cargo:warning=Could not locate clang to link its macOS runtime.");
        return;
    };
    if !output.status.success() {
        println!("cargo:warning=Could not locate clang's macOS runtime.");
        return;
    }

    let resource_dir = String::from_utf8_lossy(&output.stdout);
    let runtime_dir = PathBuf::from(resource_dir.trim()).join("lib/darwin");
    if runtime_dir.join("libclang_rt.osx.a").is_file() {
        println!("cargo:rustc-link-search=native={}", runtime_dir.display());
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
    } else {
        println!("cargo:warning=Could not locate libclang_rt.osx.a in clang's resource directory.");
    }
}
