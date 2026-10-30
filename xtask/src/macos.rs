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
    let executable = contents.join("MacOS/VoicePaste");
    files::copy(
        &runner
            .root
            .join(layout::TARGET)
            .join(profile)
            .join("voicepaste"),
        &executable,
    )?;
    fs::write(contents.join("Info.plist"), PLIST)?;
    let report = format!(
        "VoicePaste macOS Settings preview\nCommit: {sha}\nArchitecture: arm64\nMinimum macOS: {MINIMUM_OS}\nSigning: ad-hoc, NOT Developer ID signed, NOT notarized\nUI: GPUI Kit 0.8.1 / GPUI-pre 0.3.10\nConnected: shared Settings, native menu bar, local config and opt-in current-user login item.\nNo STT backend/worker/models; recording, hotkeys, overlay and paste are not connected yet.\nNo model downloads or microphone/accessibility permission requests. Config: ~/Library/Application Support/VoicePaste/config.json.\n"
    );
    fs::write(contents.join("Resources/preview.txt"), &report)?;
    let architectures = runner.output(runner.command("lipo").arg("-archs").arg(&executable))?;
    ensure!(
        architectures.trim() == "arm64",
        "Expected arm64 only: {architectures}"
    );
    let linked = runner.output(runner.command("otool").arg("-L").arg(&executable))?;
    for line in linked.lines().skip(1) {
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
    fn launch_bundle_has_stable_identity_without_premature_permission_requests() {
        assert!(PLIST.contains("dev.mykola.voicepaste"));
        assert!(PLIST.contains("<string>VoicePaste</string>"));
        assert!(PLIST.contains(&format!("<string>{MINIMUM_OS}</string>")));
        assert!(!PLIST.contains("UsageDescription"));
        assert!(!PLIST.contains("LSUIElement")); // This launch gate has a normal window, no menu bar yet.
    }
}
