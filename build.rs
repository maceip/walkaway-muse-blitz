//! Build script: drives `shermes`, then compiles the two native translation
//! units that make up the Hermes side of the browser.
//!
//! Pipeline:
//!   src/js/dom_runtime.ts --(shermes -typed -emit-c)--> $OUT_DIR/dom_runtime.c
//!   $OUT_DIR/dom_runtime.c --(cc, GNU C11)------------> libdom_runtime_unit.a
//!   c_shim/hermes_shim.cpp --(cc, C++20)--------------> libhermes_shim.a
//!   link: -lhermesvm (dylib) + libjsi.a
//!
//! Two details that are easy to get wrong and produce confusing link errors:
//!
//! * The generated C must be compiled with `-DNDEBUG`. `static_h.h` expands
//!   `_SH_MODEL()` to `_sh_model_s22_p8_rel` or `_sh_model_s22_p8_dbg` depending
//!   on `NDEBUG`, and the Release Hermes library only defines the former.
//! * It must be compiled as GNU C (`-std=gnu11`), not ISO C: `sh_tryfast_fp_cvt.h`
//!   uses aarch64 `asm` statement expressions.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));

    let hermes_src = env::var("HERMES_SRC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".cache/hermes-static_h/src"));
    let hermes_build = env::var("HERMES_BUILD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".cache/hermes-static_h/build"));

    assert!(
        hermes_src.join("include/hermes/VM/static_h.h").exists(),
        "Static Hermes sources not found at {}. Set HERMES_SRC_DIR.",
        hermes_src.display()
    );
    assert!(
        hermes_build.join("bin/shermes").exists(),
        "shermes not found at {}. Run scripts/build-hermes.sh. Set HERMES_BUILD_DIR.",
        hermes_build.display()
    );

    let shermes = hermes_build.join("bin/shermes");
    let ts_input = manifest_dir.join("src/js/dom_runtime.ts");
    let generated_c = out_dir.join("dom_runtime.c");

    println!("cargo:rerun-if-changed={}", ts_input.display());
    println!("cargo:rerun-if-changed=c_shim/hermes_shim.cpp");
    println!("cargo:rerun-if-changed=c_shim/blitz_bridge.h");
    println!("cargo:rerun-if-env-changed=HERMES_SRC_DIR");
    println!("cargo:rerun-if-env-changed=HERMES_BUILD_DIR");

    // 1. TypeScript -> C, ahead of time.
    let status = Command::new(&shermes)
        .args([
            "-typed",
            "-exported-unit=dom_runtime",
            "-emit-c",
            "-O",
            ts_input.to_str().expect("utf-8 path"),
            "-o",
            generated_c.to_str().expect("utf-8 path"),
        ])
        .status()
        .expect("failed to run shermes");
    assert!(status.success(), "shermes failed to compile dom_runtime.ts");

    let config_include = hermes_build.join("lib/config");

    // 2. Generated C (GNU C11, NDEBUG to match the Release Hermes library).
    cc::Build::new()
        .file(&generated_c)
        .include(hermes_src.join("include"))
        .include(&config_include)
        .include(manifest_dir.join("c_shim"))
        .flag("-std=gnu11")
        .define("NDEBUG", None)
        .flag_if_supported("-Wno-unused-variable")
        .flag_if_supported("-Wno-unused-function")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-label")
        .compile("dom_runtime_unit");

    // 3. C++ shim.
    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .file(manifest_dir.join("c_shim/hermes_shim.cpp"))
        .include(hermes_src.join("include"))
        .include(hermes_src.join("API"))
        .include(hermes_src.join("API/jsi"))
        .include(hermes_src.join("public"))
        .include(&config_include)
        .include(manifest_dir.join("c_shim"))
        .flag_if_supported("-Wno-unused-parameter")
        .compile("hermes_shim");

    // 4. Link Static Hermes.
    let lib_dir = hermes_build.join("lib");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!(
        "cargo:rustc-link-search=native={}",
        hermes_build.join("jsi").display()
    );
    println!("cargo:rustc-link-lib=dylib=hermesvm");
    println!("cargo:rustc-link-lib=static=jsi");
    // Keep the dylib locatable at runtime inside Walkaway.app/Contents/Frameworks
    // as well as during local cargo runs without DYLD_LIBRARY_PATH.
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
}

fn home() -> PathBuf {
    PathBuf::from(env::var("HOME").expect("HOME"))
}
