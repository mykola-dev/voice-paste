fn main() {
    println!("cargo:rerun-if-changed=native/macos-host.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_macos_host();
    }
    println!("cargo:rerun-if-changed=resources.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // GPUI's Windows backend loads icon resource 1 for its window class.
        // The same multi-resolution original asset is used by Explorer.
        embed_resource::compile("resources.rc", embed_resource::NONE)
            .manifest_required()
            .expect("VoicePaste Windows icon resource must compile");
    }
}

fn build_macos_host() {
    use std::{path::PathBuf, process::Command};
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo supplies OUT_DIR"));
    let object = output.join("macos-host.o");
    let library = output.join("libvoicepaste_macos_host.a");
    for command in [
        Command::new("xcrun")
            .args([
                "clang",
                "-arch",
                "arm64",
                "-mmacosx-version-min=26.0",
                "-fobjc-arc",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-c",
                "native/macos-host.m",
                "-o",
            ])
            .arg(&object),
        Command::new("xcrun")
            .args(["ar", "rcs"])
            .arg(&library)
            .arg(&object),
    ] {
        let status = command
            .status()
            .expect("Cannot start Xcode native host build tool");
        assert!(
            status.success(),
            "macOS host adapter compilation failed: {status}"
        );
    }
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=static=voicepaste_macos_host");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=AVFoundation");
}
