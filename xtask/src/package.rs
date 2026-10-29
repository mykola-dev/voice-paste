use crate::{files, native, process::Runner};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    path::Path,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    version: String,
    target: String,
    profile: String,
    files: Vec<Entry>,
}

fn entries(directory: &Path) -> Result<Vec<Entry>> {
    files::walk(directory)?
        .into_iter()
        .filter(|p| p != "manifest.json")
        .map(|path| {
            let full = directory.join(&path);
            let bytes = fs::metadata(&full)?.len();
            ensure!(bytes > 0, "Empty artifact: {path}");
            Ok(Entry {
                path,
                bytes,
                sha256: files::hash(&full)?,
            })
        })
        .collect()
}

fn validate_manifest(manifest: &Manifest) -> Result<BTreeMap<String, &Entry>> {
    ensure!(
        matches!(manifest.profile.as_str(), "release" | "debug")
            && manifest.target == "x86_64-pc-windows-msvc",
        "Not a supported Windows package"
    );
    let parts: Vec<_> = manifest.version.split('.').collect();
    ensure!(
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "Invalid package version"
    );
    let mut entries = BTreeMap::new();
    for entry in &manifest.files {
        files::safe_path(&entry.path)?;
        ensure!(
            !entry.path.eq_ignore_ascii_case("manifest.json"),
            "Manifest cannot own itself"
        );
        ensure!(
            entry.bytes > 0
                && entry.sha256.len() == 64
                && entry.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid manifest record"
        );
        ensure!(
            entries.insert(entry.path.to_lowercase(), entry).is_none(),
            "Duplicate manifest path"
        );
    }
    Ok(entries)
}

fn check_file(directory: &Path, entry: &Entry) -> Result<()> {
    let path = directory.join(&entry.path);
    files::no_links(&path)?;
    ensure!(
        fs::metadata(&path)?.len() == entry.bytes
            && files::hash(&path)?.eq_ignore_ascii_case(&entry.sha256),
        "Artifact mismatch: {}",
        path.display()
    );
    Ok(())
}

fn name(manifest: &Manifest) -> String {
    format!(
        "VoicePaste-{}-windows-x86_64{}",
        manifest.version,
        if manifest.profile == "debug" {
            "-debug"
        } else {
            ""
        }
    )
}

/// Fail before any writes for locked binaries, unsafe paths or unmanaged collisions.
/// Only manifest-owned files are updated; data/ is never traversed or packaged.
pub fn publish(source: &Path, destination: &Path) -> Result<()> {
    files::no_links(destination)?;
    let manifest: Manifest = files::read_json(&source.join("manifest.json"))?;
    let wanted = validate_manifest(&manifest)?;
    for entry in &manifest.files {
        check_file(source, entry)?;
    }
    let old: Option<Manifest> = if destination.join("manifest.json").exists() {
        files::no_links(&destination.join("manifest.json"))?;
        Some(files::read_json(&destination.join("manifest.json"))?)
    } else {
        None
    };
    let previous = old
        .as_ref()
        .map(validate_manifest)
        .transpose()?
        .unwrap_or_default();
    if old.is_none() && destination.exists() {
        for item in fs::read_dir(destination)? {
            ensure!(
                item?.file_name().eq_ignore_ascii_case("data"),
                "Unmanaged destination: {}",
                destination.display()
            );
        }
    }
    for (key, entry) in &previous {
        if !wanted.contains_key(key) && destination.join(&entry.path).exists() {
            check_file(destination, entry)
                .context("Stale managed artifact was modified; refusing to delete it")?;
        }
    }
    let mut locks = vec![];
    let paths: BTreeSet<_> = manifest
        .files
        .iter()
        .map(|e| &e.path)
        .chain(old.iter().flat_map(|m| m.files.iter().map(|e| &e.path)))
        .collect();
    for relative in paths {
        let path = destination.join(relative);
        files::no_links(&path)?;
        if path.exists() {
            ensure!(
                previous.contains_key(&relative.to_lowercase()),
                "Refusing to overwrite unmanaged file: {}",
                path.display()
            );
            let mut options = OpenOptions::new();
            options.read(true).write(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.share_mode(0);
            }
            locks.push(options.open(&path).with_context(|| format!("Cannot update {}. Finish recording, save Settings and quit this portable instance. No files changed", path.display()))?);
        }
    }
    // Windows denies replacing a mapped executable; preflight never terminates processes.
    drop(locks);
    fs::create_dir_all(destination)?;
    for entry in &manifest.files {
        files::copy(&source.join(&entry.path), &destination.join(&entry.path))?;
        check_file(destination, entry)?;
    }
    for (key, entry) in &previous {
        if !wanted.contains_key(key) && destination.join(&entry.path).exists() {
            fs::remove_file(destination.join(&entry.path))?;
        }
    }
    files::copy(
        &source.join("manifest.json"),
        &destination.join("manifest.json"),
    )?;
    Ok(())
}

fn archive(directory: &Path, destination: &Path, manifest: &Manifest) -> Result<()> {
    let mut zip = zip::ZipWriter::new(File::create(destination)?);
    let prefix = name(manifest);
    let options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for relative in files::walk(directory)? {
        zip.start_file(format!("{prefix}/{relative}"), options)?;
        std::io::copy(&mut File::open(directory.join(relative))?, &mut zip)?;
    }
    zip.finish()?;
    Ok(())
}

pub fn verify_archive(path: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    let mut manifest_names = vec![];
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        if entry.name().split('/').count() == 2 && entry.name().ends_with("/manifest.json") {
            manifest_names.push(entry.name().to_string());
        }
    }
    ensure!(
        manifest_names.len() == 1,
        "Expected exactly one package manifest"
    );
    let manifest: Manifest = serde_json::from_reader(zip.by_name(&manifest_names[0])?)?;
    let expected = validate_manifest(&manifest)?;
    let prefix = format!("{}/", name(&manifest));
    ensure!(
        manifest_names[0] == format!("{prefix}manifest.json"),
        "Package root/version mismatch"
    );
    let inputs = native::inputs()?;
    let required = [
        "VoicePaste.exe".to_string(),
        "runtime/voicepaste-stt-worker.exe".into(),
    ]
    .into_iter()
    .chain(inputs.runtime_files.iter().map(|n| format!("runtime/{n}")))
    .chain(inputs.host_files.iter().cloned());
    for path in required {
        ensure!(
            expected.contains_key(&path.to_lowercase()),
            "Missing packaged dependency: {path}"
        );
    }
    ensure!(
        zip.len() == expected.len() + 1,
        "Unexpected archive entries"
    );
    let mut seen = BTreeSet::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let relative = entry
            .name()
            .strip_prefix(&prefix)
            .context("Entry outside package root")?
            .to_string();
        ensure!(seen.insert(relative.to_lowercase()), "Duplicate ZIP entry");
        if relative == "manifest.json" {
            continue;
        }
        let record = expected
            .get(&relative.to_lowercase())
            .context("Unexpected ZIP entry")?;
        ensure!(
            relative == record.path
                && entry.size() == record.bytes
                && files::hash_reader(&mut entry)?.eq_ignore_ascii_case(&record.sha256),
            "ZIP size/hash/path mismatch: {relative}"
        );
    }
    println!("ZIP verified: {} files, no user data", expected.len());
    Ok(())
}

fn system_import(name: &str) -> bool {
    // Explicit OS/driver boundary, never "it exists in this builder's System32".
    name.starts_with("api-ms-")
        || name.starts_with("ext-ms-")
        || matches!(
            name,
            "kernel32.dll"
                | "kernelbase.dll"
                | "ntdll.dll"
                | "user32.dll"
                | "gdi32.dll"
                | "gdi32full.dll"
                | "advapi32.dll"
                | "shell32.dll"
                | "shlwapi.dll"
                | "ole32.dll"
                | "oleaut32.dll"
                | "combase.dll"
                | "comdlg32.dll"
                | "comctl32.dll"
                | "coremessaging.dll"
                | "gdiplus.dll"
                | "icuuc.dll"
                | "psapi.dll"
                | "uiautomationcore.dll"
                | "uuid.dll"
                | "ws2_32.dll"
                | "bcrypt.dll"
                | "bcryptprimitives.dll"
                | "crypt32.dll"
                | "ncrypt.dll"
                | "secur32.dll"
                | "sspicli.dll"
                | "rpcrt4.dll"
                | "userenv.dll"
                | "version.dll"
                | "winmm.dll"
                | "winhttp.dll"
                | "wininet.dll"
                | "dnsapi.dll"
                | "iphlpapi.dll"
                | "setupapi.dll"
                | "cfgmgr32.dll"
                | "imm32.dll"
                | "msctf.dll"
                | "uxtheme.dll"
                | "dwmapi.dll"
                | "d3d11.dll"
                | "d3d12.dll"
                | "dxgi.dll"
                | "d2d1.dll"
                | "dwrite.dll"
                | "dcomp.dll"
                | "d3dcompiler_47.dll"
                | "opengl32.dll"
                | "glu32.dll"
                | "propsys.dll"
                | "powrprof.dll"
                | "wintrust.dll"
                | "imagehlp.dll"
                | "dbghelp.dll"
                | "msvcrt.dll"
                | "ucrtbase.dll"
                | "winspool.drv"
                | "windowscodecs.dll"
                | "avrt.dll"
                | "mmdevapi.dll"
                | "mf.dll"
                | "mfplat.dll"
                | "mfreadwrite.dll"
                | "mfuuid.dll"
                | "nvcuda.dll"
                | "nvml.dll"
        )
}

pub fn verify_dependencies(runner: &Runner, directory: &Path) -> Result<()> {
    for relative in files::walk(directory)? {
        if !relative.to_lowercase().ends_with(".dll") && !relative.to_lowercase().ends_with(".exe")
        {
            continue;
        }
        let path = directory.join(&relative);
        let output = runner.output(
            runner
                .command("dumpbin.exe")
                .args(["/nologo", "/dependents"])
                .arg(&path),
        )?;
        let mut found_import_section = false;
        for line in output.lines() {
            if line.contains("dependencies:") {
                found_import_section = true;
            }
            let dependency = line.trim().to_lowercase();
            if !dependency.ends_with(".dll") || dependency.split_whitespace().count() != 1 {
                continue;
            }
            if path
                .parent()
                .context("Missing binary parent")?
                .join(&dependency)
                .is_file()
                || system_import(&dependency)
            {
                continue;
            }
            bail!("Missing app-local DLL: {relative} -> {dependency}");
        }
        ensure!(
            found_import_section,
            "Could not inspect DLL imports: {relative}"
        );
    }
    Ok(())
}

fn lucide_license() -> Result<std::path::PathBuf> {
    let home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|p| Path::new(&p).join(".cargo"))
        })
        .context("Cannot find Cargo home")?;
    let mut matches = vec![];
    for registry in fs::read_dir(home.join("registry/src"))? {
        let path = registry?
            .path()
            .join("gpui-kit-assets-0.8.1/LICENSE-LUCIDE");
        if path.is_file() {
            matches.push(path);
        }
    }
    ensure!(
        matches.len() == 1,
        "Pinned Lucide license missing or ambiguous"
    );
    Ok(matches.remove(0))
}

pub fn package(
    runner: &Runner,
    native_source: &Path,
    release: bool,
    deploy: bool,
    create_archive: bool,
) -> Result<()> {
    let inputs = native::inputs()?;
    native::validate(native_source, &inputs)?;
    let build = runner.root.join("build");
    let cache = runner.root.join(crate::layout::CACHE);
    fs::create_dir_all(&cache)?;
    let stage = tempfile::Builder::new()
        .prefix(".package-")
        .tempdir_in(&cache)?;
    let payload = stage.path().join("payload");
    fs::create_dir_all(&payload)?;
    let profile = crate::layout::profile(release);
    let target = runner.root.join(crate::layout::TARGET).join(profile);
    files::copy(
        &target.join("voicepaste.exe"),
        &payload.join("VoicePaste.exe"),
    )?;
    files::copy(
        &target.join("voicepaste-stt-worker.exe"),
        &payload.join("runtime/voicepaste-stt-worker.exe"),
    )?;
    for file in &inputs.runtime_files {
        files::copy(
            &native_source.join(file),
            &payload.join("runtime").join(file),
        )?;
    }
    for file in &inputs.host_files {
        files::copy(&native_source.join(file), &payload.join(file))?;
    }
    for (source, destination) in [
        ("README.md", "README.md"),
        (
            "apps/voicepaste/assets/icon.png",
            "apps/voicepaste/assets/icon.png",
        ),
        ("apps/voicepaste/assets/README.md", "notices/ui/README.md"),
        (
            "apps/voicepaste/assets/tray/README.md",
            "notices/ui/Tray-Artwork-Provenance.md",
        ),
        (
            "apps/voicepaste/assets/one/LICENSE",
            "notices/ui/One-Dark-MIT.txt",
        ),
        (
            "apps/voicepaste/assets/fonts/OFL.txt",
            "notices/ui/IBM-Plex-Sans-OFL.txt",
        ),
    ] {
        files::copy(&runner.root.join(source), &payload.join(destination))?;
    }
    files::copy(&lucide_license()?, &payload.join("notices/ui/Lucide.txt"))?;
    for relative in files::walk(&native_source.join("notices"))? {
        files::copy(
            &native_source.join("notices").join(&relative),
            &payload.join("notices/native").join(relative),
        )?;
    }
    verify_dependencies(runner, &payload)?;
    let manifest = Manifest {
        version: env!("CARGO_PKG_VERSION").into(),
        target: "x86_64-pc-windows-msvc".into(),
        profile: profile.into(),
        files: entries(&payload)?,
    };
    files::write_json(&payload.join("manifest.json"), &manifest)?;
    let archive_name = format!("{}.zip", name(&manifest));
    let staged_zip = stage.path().join(&archive_name);
    if create_archive {
        archive(&payload, &staged_zip, &manifest)?;
        verify_archive(&staged_zip)?;
    }
    if deploy {
        let destination = build.join(profile);
        publish(&payload, &destination)?;
        println!("Portable folder: {}", destination.display());
    } else {
        println!("--no-deploy: runnable folder build/{profile} was NOT updated.");
    }
    if create_archive {
        let packages = runner.root.join(crate::layout::PACKAGE);
        fs::create_dir_all(&packages)?;
        let checksum = format!("{}  {archive_name}\n", files::hash(&staged_zip)?);
        files::copy(&staged_zip, &packages.join(&archive_name))?;
        fs::write(packages.join(format!("{archive_name}.sha256")), checksum)?;
        println!("Archive: {}", packages.join(archive_name).display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(dir: &Path, profile: &str) -> Manifest {
        fs::create_dir_all(dir).unwrap();
        let inputs = native::inputs().unwrap();
        for relative in [
            "VoicePaste.exe".to_string(),
            "runtime/voicepaste-stt-worker.exe".into(),
        ]
        .into_iter()
        .chain(inputs.runtime_files.iter().map(|n| format!("runtime/{n}")))
        .chain(inputs.host_files)
        {
            let path = dir.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "fixture").unwrap();
        }
        let manifest = Manifest {
            version: "2.0.0".into(),
            target: "x86_64-pc-windows-msvc".into(),
            profile: profile.into(),
            files: entries(dir).unwrap(),
        };
        files::write_json(&dir.join("manifest.json"), &manifest).unwrap();
        manifest
    }

    #[test]
    fn repeated_publication_preserves_all_personal_data() {
        for profile in ["debug", "release"] {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("source");
            let destination = temp.path().join("destination");
            fixture(&source, profile);
            publish(&source, &destination).unwrap();
            for path in [
                "data/config.json",
                "data/models/weights.bin",
                "data/sessions/current.wav",
            ] {
                let path = destination.join(path);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, "personal prompt — Україна").unwrap();
            }
            fs::write(source.join("VoicePaste.exe"), "rebuilt").unwrap();
            let manifest = Manifest {
                files: entries(&source).unwrap(),
                ..fixture_manifest(profile)
            };
            files::write_json(&source.join("manifest.json"), &manifest).unwrap();
            publish(&source, &destination).unwrap();
            publish(&source, &destination).unwrap();
            for path in [
                "data/config.json",
                "data/models/weights.bin",
                "data/sessions/current.wav",
            ] {
                assert_eq!(
                    fs::read_to_string(destination.join(path)).unwrap(),
                    "personal prompt — Україна"
                );
            }
        }
    }

    fn fixture_manifest(profile: &str) -> Manifest {
        Manifest {
            version: "2.0.0".into(),
            target: "x86_64-pc-windows-msvc".into(),
            profile: profile.into(),
            files: vec![],
        }
    }

    #[test]
    fn zip_integrity_and_host_crt_are_required() {
        for profile in ["debug", "release"] {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("source");
            let mut manifest = fixture(&source, profile);
            let zip = temp.path().join("package.zip");
            archive(&source, &zip, &manifest).unwrap();
            verify_archive(&zip).unwrap();
            fs::write(source.join("VoicePaste.exe"), "tampered").unwrap();
            archive(&source, &zip, &manifest).unwrap();
            assert!(verify_archive(&zip).is_err());
            fs::write(source.join("VoicePaste.exe"), "fixture").unwrap();
            fs::remove_file(source.join("vcruntime140.dll")).unwrap();
            manifest.files = entries(&source).unwrap();
            files::write_json(&source.join("manifest.json"), &manifest).unwrap();
            archive(&source, &zip, &manifest).unwrap();
            assert!(verify_archive(&zip).is_err());
        }
    }

    #[test]
    fn corrupt_staging_and_unsafe_old_manifest_do_not_write() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        let manifest = fixture(&source, "release");
        publish(&source, &destination).unwrap();
        fs::write(source.join("VoicePaste.exe"), "corrupt").unwrap();
        assert!(publish(&source, &destination).is_err());
        assert_eq!(
            fs::read(destination.join("VoicePaste.exe")).unwrap(),
            b"fixture"
        );
        fs::write(source.join("VoicePaste.exe"), "fixture").unwrap();
        let mut old = manifest;
        old.files[0].path = "data/config.json".into();
        files::write_json(&destination.join("manifest.json"), &old).unwrap();
        assert!(publish(&source, &destination).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn locked_destination_is_untouched() {
        use std::os::windows::fs::OpenOptionsExt;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        fixture(&source, "release");
        publish(&source, &destination).unwrap();
        let _lock = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(destination.join("VoicePaste.exe"))
            .unwrap();
        assert!(publish(&source, &destination).is_err());
        assert_eq!(
            fs::read(destination.join("runtime/voicepaste-stt-worker.exe")).unwrap(),
            b"fixture"
        );
    }

    #[test]
    fn system_allowlist_does_not_hide_missing_crt_or_cuda() {
        assert!(system_import("kernel32.dll"));
        assert!(system_import("nvcuda.dll"));
        for name in [
            "vcruntime140.dll",
            "msvcp140_1.dll",
            "cublas64_12.dll",
            "unknown.dll",
        ] {
            assert!(!system_import(name));
        }
    }

    #[test]
    fn archive_rejects_manifest_and_untracked_entry_failures() {
        for scenario in [
            "size",
            "hash",
            "duplicate",
            "private",
            "traversal",
            "target",
            "version",
            "profile",
            "missing-worker",
            "extra",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let source = temp.path().join("source");
            let mut manifest = fixture(&source, "release");
            match scenario {
                "size" => manifest.files[0].bytes += 1,
                "hash" => manifest.files[0].sha256 = "0".repeat(64),
                "duplicate" => manifest.files.push(manifest.files[0].clone()),
                "private" => manifest.files[0].path = "data/config.json".into(),
                "traversal" => manifest.files[0].path = "../outside".into(),
                "target" => manifest.target = "wrong".into(),
                "version" => manifest.version = "../escape".into(),
                "profile" => manifest.profile = "wrong".into(),
                "missing-worker" => {
                    fs::remove_file(source.join("runtime/voicepaste-stt-worker.exe")).unwrap();
                    manifest.files = entries(&source).unwrap();
                }
                "extra" => fs::write(source.join("untracked.txt"), "extra").unwrap(),
                _ => unreachable!(),
            }
            files::write_json(&source.join("manifest.json"), &manifest).unwrap();
            let zip = temp.path().join("package.zip");
            archive(&source, &zip, &manifest).unwrap();
            assert!(verify_archive(&zip).is_err(), "{scenario}");
        }
    }

    #[test]
    fn publication_removes_only_unmodified_stale_managed_files() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        let mut manifest = fixture(&source, "release");
        fs::write(source.join("old.txt"), "old").unwrap();
        manifest.files = entries(&source).unwrap();
        files::write_json(&source.join("manifest.json"), &manifest).unwrap();
        publish(&source, &destination).unwrap();
        fs::write(destination.join("personal.txt"), "keep").unwrap();
        fs::remove_file(source.join("old.txt")).unwrap();
        manifest.files = entries(&source).unwrap();
        files::write_json(&source.join("manifest.json"), &manifest).unwrap();
        fs::write(destination.join("old.txt"), "modified").unwrap();
        assert!(publish(&source, &destination).is_err());
        fs::write(destination.join("old.txt"), "old").unwrap();
        publish(&source, &destination).unwrap();
        assert!(!destination.join("old.txt").exists());
        assert_eq!(fs::read(destination.join("personal.txt")).unwrap(), b"keep");
    }
}
