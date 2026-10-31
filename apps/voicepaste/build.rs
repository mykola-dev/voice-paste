fn main() {
    println!("cargo:rerun-if-changed=native/macos-host.m");
    println!("cargo:rerun-if-changed=native/macos-input.m");
    println!("cargo:rerun-if-changed=native/macos-overlay.m");
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
    let library = output.join("libvoicepaste_macos_host.a");
    let mut objects = Vec::new();
    for name in ["macos-host", "macos-input", "macos-overlay"] {
        let object = output.join(format!("{name}.o"));
        let status = Command::new("xcrun")
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
            ])
            .arg(format!("native/{name}.m"))
            .arg("-o")
            .arg(&object)
            .status()
            .expect("Cannot start Xcode native host build tool");
        assert!(
            status.success(),
            "macOS host adapter compilation failed: {status}"
        );
        objects.push(object);
    }
    assert!(
        Command::new("xcrun")
            .args(["ar", "rcs"])
            .arg(&library)
            .args(&objects)
            .status()
            .expect("Cannot archive native adapter")
            .success()
    );
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=static=voicepaste_macos_host");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=AVFoundation");
    println!("cargo:rustc-link-lib=framework=ApplicationServices");
    println!("cargo:rustc-link-lib=framework=QuartzCore");
}
