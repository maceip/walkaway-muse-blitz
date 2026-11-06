//! Build script: drives `shermes`, then compiles the native translation units
//! for the AOT page adapter and the separate interpreted scratchpad debugger.
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
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_native_tabs(&manifest_dir, &out_dir);
    }

    let hermes_src = env::var("HERMES_SRC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".cache/hermes-static_h/src"));
    let hermes_build = env::var("HERMES_BUILD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".cache/hermes-static_h/build-walkaway-debugger"));

    let cmake_cache = hermes_build.join("CMakeCache.txt");
    println!("cargo:rerun-if-changed={}", cmake_cache.display());
    let cache = std::fs::read_to_string(&cmake_cache).unwrap_or_else(|error| {
        panic!(
            "Cannot read Hermes configuration {}: {error}. Rebuild with HERMES_BUILD_DIR='{}' bash scripts/build-hermes.sh; the debugger requires HERMES_ENABLE_DEBUGGER=ON.",
            cmake_cache.display(), hermes_build.display()
        )
    });
    assert!(
        cache
            .lines()
            .any(|line| line.trim() == "HERMES_ENABLE_DEBUGGER:BOOL=ON"),
        "Hermes at {} was built without HERMES_ENABLE_DEBUGGER=ON. Use the isolated debugger build: HERMES_BUILD_DIR=\"$HOME/.cache/hermes-static_h/build-walkaway-debugger\" bash scripts/build-hermes.sh, then cargo build with the same HERMES_BUILD_DIR (or unset it to use that default). Do not reconfigure a shared running browser's cache.",
        hermes_build.display()
    );

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
    println!("cargo:rerun-if-changed=c_shim/debugger.cpp");
    println!("cargo:rerun-if-changed=c_shim/blitz_bridge.h");
    println!(
        "cargo:rerun-if-changed={}",
        hermes_src.join("API/hermes/DebuggerAPI.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        hermes_src.join("API/hermes/hermes.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        hermes_src.join("API/jsi/jsi/hermes-interfaces.h").display()
    );
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

    // 3. Native shims share ONE debugger-enabled libhermesvm. The debugger's
    // interpreted worker never instantiates or attaches to the AOT DOM unit.
    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .define("HERMES_ENABLE_DEBUGGER", None)
        .file(manifest_dir.join("c_shim/hermes_shim.cpp"))
        .file(manifest_dir.join("c_shim/debugger.cpp"))
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

/// Compile the complete original component and our C ABI in one Swift module.
/// All outputs, including Swift's module cache, belong to this Cargo OUT_DIR;
/// no package resolution, external source checkout, or separate app is used.
fn build_native_tabs(manifest: &Path, out_dir: &Path) {
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    println!("cargo:rerun-if-env-changed=SDKROOT");
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    let native = manifest.join("native-tabs");
    println!("cargo:rerun-if-changed={}", native.display());
    let provenance: serde_json::Value = serde_json::from_slice(
        &std::fs::read(native.join("VENDOR.json")).expect("read native tab provenance"),
    )
    .expect("parse native tab provenance");
    for entry in provenance["files"].as_array().expect("vendored files") {
        let relative = entry["path"].as_str().expect("vendored relative path");
        let path = native.join(relative);
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = std::fs::read(&path).expect("read vendored source");
        let digest = format!("{:x}", Sha256::digest(bytes));
        assert_eq!(
            Some(digest.as_str()),
            entry["vendored_sha256"].as_str(),
            "Vendored source changed without an explicit provenance update: {}",
            path.display()
        );
    }
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        other => panic!("unsupported macOS Swift architecture: {other:?}"),
    };
    let deployment = env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| "13.0".into());
    let major: u32 = deployment
        .split('.')
        .next()
        .and_then(|v| v.parse().ok())
        .expect("valid MACOSX_DEPLOYMENT_TARGET");
    assert!(
        major >= 13,
        "original CompactTabStrip requires macOS 13 or later"
    );
    let target = format!("{arch}-apple-macosx{deployment}");
    let swiftc = command_text(Command::new("xcrun").args(["--find", "swiftc"]));
    let sdk = command_text(Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]));
    let info: serde_json::Value =
        serde_json::from_str(&command_text(Command::new(&swiftc).args([
            "-target",
            &target,
            "-print-target-info",
        ])))
        .expect("parse Swift runtime target information");
    let mut sources: Vec<PathBuf> = std::fs::read_dir(native.join("Sources/CompactTabStrip"))
        .expect("read original Swift source directory")
        .map(|entry| entry.expect("native source entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "swift"))
        .collect();
    sources.sort();
    sources.push(native.join("WalkawayTabBridge.swift"));
    sources.push(native.join("NativeTabSelfTest.swift"));
    let archive = out_dir.join("libwalkaway_native_tabs.a");
    let mut compile = Command::new(&swiftc);
    compile.args([
        "-swift-version",
        "5",
        "-parse-as-library",
        "-O",
        "-whole-module-optimization",
        "-emit-library",
        "-static",
        "-module-name",
        "WalkawayNativeTabs",
        "-target",
        &target,
        "-sdk",
        &sdk,
        "-module-cache-path",
    ]);
    compile.arg(out_dir.join("swift-module-cache"));
    compile.args(&sources).arg("-o").arg(&archive);
    command_text(&mut compile);
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=walkaway_native_tabs");
    // The Mach-O Swift objects carry their own framework/runtime autolink
    // directives. Make SDK stubs and toolchain compatibility archives available
    // to Apple's linker, without embedding machine-local runtime paths.
    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    for path in info["paths"]["runtimeLibraryPaths"]
        .as_array()
        .expect("Swift runtime library paths")
    {
        println!(
            "cargo:rustc-link-search=native={}",
            path.as_str().expect("runtime path")
        );
    }
    for framework in [
        "AppKit",
        "SwiftUI",
        "Foundation",
        "CoreGraphics",
        "QuartzCore",
        "CoreImage",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    // main() also installs @executable_path/../Frameworks for bundled Swift
    // back-deployment libraries (swift-stdlib-tool). Otherwise use macOS's
    // stable Swift runtime; never embed a machine-local Xcode runtime rpath.
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    println!("cargo:rustc-link-arg=-mmacosx-version-min={deployment}");
    // Swift compatibility archives must precede system runtime resolution.
    // Swift driver knows which ones this target needs; do not hardcode a version.
    if let Some(libraries) = info["target"]["compatibilityLibraries"].as_array() {
        for library in libraries {
            if let Some(name) = library["libraryName"].as_str() {
                println!("cargo:rustc-link-lib=static={name}");
            }
        }
    }
}

fn command_text(command: &mut Command) -> String {
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("run {command:?}: {e}"));
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(output.status.success(), "command failed: {command:?}");
    String::from_utf8(output.stdout)
        .expect("UTF-8 compiler output")
        .trim()
        .to_owned()
}
