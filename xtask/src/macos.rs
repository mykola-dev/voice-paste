//! Complete macOS preview bundle. No worker/models are claimed or downloaded.
use crate::{files, layout, process::Runner};
use anyhow::{Context, Result, ensure};
use std::fs;

const MINIMUM_OS: &str = "26.0";
const PLIST: &str = include_str!("../macos/Info.plist");

pub fn preview(runner: &mut Runner, release: bool, offline: bool) -> Result<()> {
    ensure!(
        cfg!(all(target_os = "macos", target_arch = "aarch64")),
        "macos-preview requires an Apple Silicon macOS build host"
    );
    ensure!(
        std::env::var_os("CARGO_BUILD_TARGET").is_none(),
        "Unset CARGO_BUILD_TARGET; this task builds for its native Apple Silicon host"
    );
    let profile = if release { "release" } else { "debug" };
    let destination = runner
        .root
        .join("build")
        .join(profile)
        .join("VoicePaste.app");
    files::no_links(&destination)?;
    ensure!(
        !destination.exists(),
        "Refusing to replace an existing preview bundle (it may be running): {}. Move it aside explicitly first.",
        destination.display()
    );
    let sha = runner.output(runner.command("git").args(["rev-parse", "HEAD"]))?;
    let sha = sha.trim();
    ensure!(
        sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid build SHA"
    );
    runner.env.insert("VOICEPASTE_BUILD_SHA".into(), sha.into());
    runner
        .env
        .insert("MACOSX_DEPLOYMENT_TARGET".into(), MINIMUM_OS.into());
    runner.env.insert(
        "CARGO_TARGET_DIR".into(),
        runner.root.join(layout::TARGET).into_os_string(),
    );
    // whisper-rs-sys 0.15.0 consumes GGML_* env vars but does not register
    // rerun-if-env-changed for them. Invalidate only its compilation cache when
    // our native policy changes; never reuse a newer-runner CPU-native binary.
    let policy = files::hash(&runner.root.join(".cargo/config.toml"))?;
    let stamp = runner.root.join("build/cache/macos-native-policy.sha256");
    if fs::read_to_string(&stamp).ok().as_deref() != Some(policy.as_str()) {
        runner.cargo(
            &["clean", "--package", "whisper-rs-sys", "--locked"],
            false,
            offline,
        )?;
        fs::create_dir_all(
            stamp
                .parent()
                .context("Native policy stamp has no parent")?,
        )?;
        fs::write(&stamp, policy)?;
    }
    runner.cargo(
        &[
            "build",
            "--locked",
            "--package",
            "voicepaste",
            "--bin",
            "voicepaste",
        ],
        release,
        offline,
    )?;
    runner.cargo(
        &[
            "build",
            "--locked",
            "--package",
            "voicepaste-stt-worker",
            "--bin",
            "voicepaste-stt-worker",
        ],
        release,
        offline,
    )?;

    let stage = runner
        .root
        .join("build/cache")
        .join(format!("macos-preview-{}", std::process::id()));
    files::no_links(&stage)?;
    fs::create_dir(&stage)?;
    let bundle = stage.join("VoicePaste.app");
    let contents = bundle.join("Contents");
    fs::create_dir_all(contents.join("MacOS"))?;
    fs::create_dir_all(contents.join("Resources"))?;
    let iconset = stage.join("VoicePaste.iconset");
    fs::create_dir(&iconset)?;
    let source = runner
        .root
        .join("apps/voicepaste/assets/tray/masters/lips-open.png");
    for logical in [16, 32, 128, 256, 512] {
        for scale in [1, 2] {
            let pixels = (logical * scale).to_string();
            let suffix = if scale == 2 { "@2x" } else { "" };
            runner.run(
                runner
                    .command("sips")
                    .args(["-z", &pixels, &pixels])
                    .arg(&source)
                    .arg("--out")
                    .arg(iconset.join(format!("icon_{logical}x{logical}{suffix}.png"))),
            )?;
        }
    }
    runner.run(
        runner
            .command("iconutil")
            .args(["-c", "icns"])
            .arg(&iconset)
            .arg("-o")
            .arg(contents.join("Resources/VoicePaste.icns")),
    )?;
    fs::write(
        contents.join("Resources/worker-third-party-notices.md"),
        include_str!("../../apps/stt-worker/THIRD_PARTY_NOTICES.md"),
    )?;
    let executable = contents.join("MacOS/VoicePaste");
    files::copy(
        &runner
            .root
            .join(layout::TARGET)
            .join(profile)
            .join("voicepaste"),
        &executable,
    )?;
    let worker = contents.join("MacOS/voicepaste-stt-worker");
    files::copy(
        &runner
            .root
            .join(layout::TARGET)
            .join(profile)
            .join("voicepaste-stt-worker"),
        &worker,
    )?;
    runner.run(
        runner
            .command("codesign")
            .args(["--force", "--sign", "-", "--timestamp=none"])
            .arg(&worker),
    )?;
    fs::write(contents.join("Info.plist"), PLIST)?;
    let report = format!(
        "VoicePaste macOS development build\nCommit: {sha}\nArchitecture: arm64\nMinimum macOS: {MINIMUM_OS}\nSigning: ad-hoc, NOT Developer ID signed, NOT notarized\nUI: GPUI Kit 0.8.1 / GPUI-pre 0.3.10\nShared dictation runtime, CPAL capture, Settings and statistics. Native menu, global single-key recording/Escape, nonactivating overlay and Cmd+V adapters.\nBundled whisper-rs 0.16.0 / whisper-rs-sys 0.15.0 worker, Metal-only or CPU; full-size pinned GGML large-v2, large-v3 and turbo plus Silero VAD. Models are NOT bundled or downloaded by builds.\nDownload selected model requires menu confirmation. Microphone access is requested on explicit Start; Accessibility prompt only through Enable recording keys.\nConfig: ~/Library/Application Support/VoicePaste/config.json. End-to-end hardware/permissions/paste/visual acceptance remains unperformed.\n"
    );
    fs::write(contents.join("Resources/preview.txt"), &report)?;
    let mut linked = String::new();
    for binary in [&executable, &worker] {
        let architectures = runner.output(runner.command("lipo").arg("-archs").arg(binary))?;
        ensure!(
            architectures.trim() == "arm64",
            "Expected arm64 only: {architectures}"
        );
        let libraries = runner.output(runner.command("otool").arg("-L").arg(binary))?;
        for line in libraries.lines().skip(1) {
            let library = line
                .trim()
                .split(" (compatibility")
                .next()
                .unwrap_or_default();
            ensure!(
                library.starts_with("/System/Library/") || library.starts_with("/usr/lib/"),
                "Unbundled dependency: {library}"
            );
        }
        linked.push_str(&libraries);
    }
    runner.run(
        runner
            .command("plutil")
            .arg("-lint")
            .arg(contents.join("Info.plist")),
    )?;
    runner.run(
        runner
            .command("codesign")
            .args(["--force", "--sign", "-", "--timestamp=none"])
            .arg(&bundle),
    )?;
    runner.run(
        runner
            .command("codesign")
            .args(["--verify", "--deep", "--strict", "--verbose=2"])
            .arg(&bundle),
    )?;
    fs::create_dir_all(
        destination
            .parent()
            .context("Bundle has no parent directory")?,
    )?;
    fs::rename(&bundle, &destination)?;
    let package = runner.root.join("build/package");
    fs::create_dir_all(&package)?;
    let suffix = if release { "" } else { "-debug" };
    let name = format!("VoicePaste-macos-arm64-launch-preview-{sha}{suffix}.zip");
    let archive = package.join(&name);
    ensure!(
        !archive.exists(),
        "Preview archive already exists: {}",
        archive.display()
    );
    runner.run(
        runner
            .command("ditto")
            .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
            .arg(&destination)
            .arg(&archive),
    )?;
    fs::write(
        package.join(format!("{name}.sha256")),
        format!("{}  {name}\n", files::hash(&archive)?),
    )?;
    fs::write(
        package.join("macos-preview.txt"),
        format!("{report}\nLinked system libraries:\n{linked}"),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_bundle_has_stable_identity_and_explicit_microphone_purpose() {
        assert!(PLIST.contains("dev.mykola.voicepaste"));
        assert!(PLIST.contains("<string>VoicePaste</string>"));
        assert!(PLIST.contains(&format!("<string>{MINIMUM_OS}</string>")));
        assert!(PLIST.contains("NSMicrophoneUsageDescription"));
        assert!(PLIST.contains("VoicePaste.icns"));
        let cargo = include_str!("../../.cargo/config.toml");
        assert!(cargo.contains("GGML_NATIVE = { value = \"OFF\", force = true }"));
        assert!(cargo.contains("armv8.4-a+fp16+dotprod"));
        assert!(!PLIST.contains("NSCameraUsageDescription"));
        assert!(!PLIST.contains("NSAppleEventsUsageDescription"));
    }
}
