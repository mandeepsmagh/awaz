use std::env;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "linux" {
        // Release archives keep libmoonshine.so beside the binary in ./lib.
        // Source builds stage it under ./vendor, which is two levels above
        // target/{debug,release}/awaz. Keeping both paths makes direct source
        // builds runnable without requiring users to manage LD_LIBRARY_PATH.
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/lib:$ORIGIN/../../vendor/moonshine/lib"
        );
    }
}
