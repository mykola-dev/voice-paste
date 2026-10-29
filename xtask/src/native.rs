use crate::{files, process::Runner};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Inputs {
    pub ct2_revision: String,
    pub host_files: Vec<String>,
    pub runtime_files: Vec<String>,
    ct2_source: Download,
    archives: Vec<Archive>,
    notices: Vec<Download>,
    vad: Download,
}

#[derive(Deserialize)]
struct Download {
    name: String,
    url: String,
    hash: String,
    #[serde(default)]
    version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Archive {
    #[serde(flatten)]
    download: Download,
    license_path: String,
    files: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    schema_version: u32,
    ct2_revision: String,
    files: Vec<Record>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    path: String,
    bytes: u64,
    sha256: String,
    version: String,
    origin: String,
    license_path: String,
}

pub fn inputs() -> Result<Inputs> {
    Ok(serde_json::from_str(include_str!("../native-inputs.json"))?)
}

fn pinned_file_with(
    input: &Download,
    cache: &Path,
    offline: bool,
    transport: impl FnOnce(&Path) -> Result<()>,
) -> Result<PathBuf> {
    files::safe_path(&input.name)?;
    ensure!(
        !input.name.contains('/')
            && input.url.starts_with("https://")
            && input.url[8..].contains('/'),
        "Invalid pinned URL/name"
    );
    ensure!(
        input.hash.len() == 64 && input.hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid SHA-256"
    );
    fs::create_dir_all(cache)?;
    let destination = cache.join(&input.name);
    files::no_links(&destination)?;
    if !destination.exists() {
        ensure!(!offline, "Offline input missing: {}", input.name);
        let partial = tempfile::NamedTempFile::new_in(cache)?;
        transport(partial.path())?;
        ensure!(
            files::hash(partial.path())?.eq_ignore_ascii_case(&input.hash),
            "Download hash mismatch: {}",
            input.name
        );
        partial.persist_noclobber(&destination)?;
    }
    ensure!(
        files::hash(&destination)?.eq_ignore_ascii_case(&input.hash),
        "Cached hash mismatch: {}",
        input.name
    );
    Ok(destination)
}

fn pinned_file(input: &Download, runner: &Runner, offline: bool) -> Result<PathBuf> {
    pinned_file_with(
        input,
        &runner.root.join(crate::layout::UPSTREAM_NATIVE),
        offline,
        |partial| {
            println!("Downloading {}", input.name);
            runner.run(
                runner
                    .command("curl")
                    .args([
                        "--fail",
                        "--location",
                        "--silent",
                        "--show-error",
                        "--proto",
                        "=https",
                        "--proto-redir",
                        "=https",
                        "--connect-timeout",
                        "30",
                        "--max-time",
                        "600",
                        "--retry",
                        "2",
                        "--output",
                    ])
                    .arg(partial)
                    .arg(&input.url),
            )
        },
    )
}

fn extract(archive: &Path, mappings: &BTreeMap<String, String>, directory: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(File::open(archive)?)?;
    let mut seen = BTreeSet::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        if let Some(relative) = mappings.get(entry.name()) {
            files::safe_path(relative)?;
            ensure!(
                seen.insert(relative.to_lowercase()) && entry.size() > 0 && !entry.is_dir(),
                "Duplicate/empty native payload: {relative}"
            );
            let path = directory.join(relative);
            fs::create_dir_all(path.parent().context("Missing extraction parent")?)?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            std::io::copy(&mut entry, &mut file)?;
        }
    }
    ensure!(
        seen.len() == mappings.len(),
        "Missing native archive payload: {}",
        archive.display()
    );
    Ok(())
}

fn record(
    stage: &Path,
    path: &str,
    version: &str,
    origin: &str,
    license: &str,
    records: &mut Vec<Record>,
) -> Result<()> {
    records.push(Record {
        path: path.into(),
        bytes: fs::metadata(stage.join(path))?.len(),
        sha256: files::hash(&stage.join(path))?,
        version: version.into(),
        origin: origin.into(),
        license_path: license.into(),
    });
    Ok(())
}

pub fn prepare(runner: &Runner, source: &Path, vs: &Path, offline: bool) -> Result<()> {
    let lock = inputs()?;
    files::no_links(source)?;
    if source.exists() {
        validate(source, &lock)?;
        return Ok(());
    }
    let parent = source.parent().context("Native source needs a parent")?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".native-")
        .tempdir_in(parent)?;
    let mut records = vec![];
    for archive in &lock.archives {
        let path = pinned_file(&archive.download, runner, offline)?;
        extract(&path, &archive.files, stage.path())?;
        for relative in archive.files.values() {
            let license = if relative == "libiomp5md.dll" {
                "notices/Intel-Simplified-Software-LICENSE.txt"
            } else {
                &archive.license_path
            };
            record(
                stage.path(),
                relative,
                &archive.download.version,
                &archive.download.url,
                license,
                &mut records,
            )?;
        }
    }
    let source_zip = pinned_file(&lock.ct2_source, runner, offline)?;
    let prefix = format!("CTranslate2-{}/", lock.ct2_revision);
    let mut zip = zip::ZipArchive::new(File::open(&source_zip)?)?;
    let mut headers = BTreeMap::new();
    headers.insert(
        format!("{prefix}LICENSE"),
        "notices/CTranslate2-LICENSE.txt".into(),
    );
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        if let Some(relative) = entry.name().strip_prefix(&prefix)
            && relative.starts_with("include/")
            && (relative.ends_with(".h") || relative.ends_with(".hpp"))
        {
            ensure!(
                headers
                    .insert(entry.name().into(), relative.into())
                    .is_none(),
                "Duplicate header"
            );
        }
    }
    extract(&source_zip, &headers, stage.path())?;
    for relative in headers.values() {
        record(
            stage.path(),
            relative,
            "4.7.1",
            &lock.ct2_source.url,
            "notices/CTranslate2-LICENSE.txt",
            &mut records,
        )?;
    }
    for name in ["avx_mathfun.h", "avx512_mathfun.h"] {
        let mut text = String::new();
        zip.by_name(&format!("{prefix}third_party/{name}"))?
            .read_to_string(&mut text)?;
        let comment = text
            .split_once("#include")
            .context("Missing SIMD notice")?
            .0
            .trim();
        ensure!(
            comment.to_lowercase().contains("permission") || comment.contains("Licence : BSD-2"),
            "Invalid SIMD notice"
        );
        let relative = format!("notices/{name}-LICENSE.txt");
        fs::write(stage.path().join(&relative), format!("{comment}\n"))?;
        record(
            stage.path(),
            &relative,
            "4.7.1",
            &lock.ct2_source.url,
            &relative,
            &mut records,
        )?;
    }
    for notice in &lock.notices {
        let path = pinned_file(notice, runner, offline)?;
        let relative = format!("notices/{}", notice.name);
        files::copy(&path, &stage.path().join(&relative))?;
        record(
            stage.path(),
            &relative,
            "pinned upstream notice",
            &notice.url,
            &relative,
            &mut records,
        )?;
    }
    let version =
        fs::read_to_string(vs.join("VC/Auxiliary/Build/Microsoft.VCRedistVersion.default.txt"))?;
    let crt = vs.join(format!(
        "VC/Redist/MSVC/{}/x64/Microsoft.VC143.CRT",
        version.trim()
    ));
    for name in [
        "msvcp140.dll",
        "msvcp140_1.dll",
        "vcruntime140.dll",
        "vcruntime140_1.dll",
    ] {
        let path = crt.join(name);
        runner.run(
            runner
                .command("signtool.exe")
                .args(["verify", "/pa", "/q"])
                .arg(&path),
        )?;
        files::copy(&path, &stage.path().join(name))?;
        record(
            stage.path(),
            name,
            version.trim(),
            "https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution",
            "notices/Microsoft-CRT-LICENSE.docx",
            &mut records,
        )?;
    }
    let vad = pinned_file(&lock.vad, runner, offline)?;
    files::copy(&vad, &stage.path().join(&lock.vad.name))?;
    record(
        stage.path(),
        &lock.vad.name,
        &lock.vad.version,
        &lock.vad.url,
        "notices/Silero-LICENSE.txt",
        &mut records,
    )?;
    records.sort_by(|a, b| a.path.cmp(&b.path));
    files::write_json(
        &stage.path().join("notices/inventory.json"),
        &Inventory {
            schema_version: 1,
            ct2_revision: lock.ct2_revision.clone(),
            files: records,
        },
    )?;
    validate(stage.path(), &lock)?;
    ensure!(
        !source.exists(),
        "Native destination appeared during preparation; refusing overwrite"
    );
    fs::rename(stage.path(), source)?;
    println!("Native inputs ready: {}", source.display());
    Ok(())
}

pub fn validate(source: &Path, lock: &Inputs) -> Result<()> {
    let inventory: Inventory = files::read_json(&source.join("notices/inventory.json"))?;
    ensure!(
        inventory.schema_version == 1 && inventory.ct2_revision == lock.ct2_revision,
        "Native inventory revision mismatch"
    );
    let mut seen = BTreeSet::new();
    for entry in &inventory.files {
        files::safe_path(&entry.path)?;
        let path = source.join(&entry.path);
        files::no_links(&path)?;
        ensure!(
            seen.insert(entry.path.to_lowercase()),
            "Duplicate native inventory path"
        );
        ensure!(
            entry.origin.starts_with("https://") && !entry.version.is_empty(),
            "Missing native provenance"
        );
        ensure!(
            entry.bytes > 0
                && fs::metadata(&path)?.len() == entry.bytes
                && files::hash(&path)?.eq_ignore_ascii_case(&entry.sha256),
            "Native inventory mismatch: {}",
            entry.path
        );
    }
    for entry in &inventory.files {
        files::safe_path(&entry.license_path)?;
        ensure!(
            entry.license_path.starts_with("notices/")
                && seen.contains(&entry.license_path.to_lowercase()),
            "Missing native notice: {}",
            entry.path
        );
    }
    for name in lock
        .runtime_files
        .iter()
        .chain(&lock.host_files)
        .chain([&"include/ctranslate2/translator.h".to_string()])
    {
        ensure!(
            seen.contains(&name.to_lowercase()),
            "Native inventory missing: {name}"
        );
    }
    let actual: BTreeSet<_> = files::walk(source)?
        .into_iter()
        .filter(|p| p != "notices/inventory.json")
        .map(|p| p.to_lowercase())
        .collect();
    ensure!(actual == seen, "Untracked native input files");
    Ok(())
}

pub fn import_library(runner: &Runner, source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let exports = runner.output(
        runner
            .command("dumpbin.exe")
            .args(["/nologo", "/exports"])
            .arg(source.join("ctranslate2.dll")),
    )?;
    let symbols: Vec<_> = exports
        .lines()
        .filter_map(|line| {
            let words: Vec<_> = line.split_whitespace().collect();
            (words.len() >= 4
                && words[0].parse::<u32>().is_ok()
                && u32::from_str_radix(words[1], 16).is_ok()
                && u32::from_str_radix(words[2], 16).is_ok())
            .then(|| words[3])
        })
        .collect();
    ensure!(symbols.len() > 100, "Unexpected CT2 exports");
    let definition = destination.join("ctranslate2.def");
    let mut file = File::create(&definition)?;
    writeln!(
        file,
        "LIBRARY ctranslate2.dll\nEXPORTS\n{}",
        symbols.join("\n")
    )?;
    drop(file);
    runner.run(
        runner
            .command("lib.exe")
            .args(["/nologo", "/machine:x64"])
            .arg(format!("/def:{}", definition.display()))
            .arg(format!(
                "/out:{}",
                destination.join("ctranslate2.lib").display()
            )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_cache_and_failure_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let input = Download {
            name: "fixture.zip".into(),
            url: "https://example.invalid/file".into(),
            hash: files::hash_reader(&b"fixture"[..]).unwrap(),
            version: "test".into(),
        };
        assert!(pinned_file_with(&input, temp.path(), true, |_| panic!()).is_err());
        for fail in [true, false] {
            assert!(
                pinned_file_with(&input, temp.path(), false, |path| {
                    fs::write(path, "wrong")?;
                    ensure!(!fail, "Synthetic transport failure");
                    Ok(())
                })
                .is_err()
            );
            assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
        }
        let path = pinned_file_with(&input, temp.path(), false, |path| {
            fs::write(path, "fixture")?;
            Ok(())
        })
        .unwrap();
        pinned_file_with(&input, temp.path(), true, |_| panic!()).unwrap();
        fs::write(path, "tampered").unwrap();
        assert!(pinned_file_with(&input, temp.path(), false, |_| panic!()).is_err());
    }

    #[test]
    fn embedded_lock_is_valid_and_complete() {
        let inputs = inputs().unwrap();
        assert_eq!(inputs.runtime_files.len(), 10);
        assert_eq!(inputs.archives.len(), 4);
        for name in inputs.host_files.iter().chain(&inputs.runtime_files) {
            files::safe_path(name).unwrap();
        }
    }

    #[test]
    fn inventory_rejects_integrity_provenance_and_path_failures() {
        let temp = tempfile::tempdir().unwrap();
        let lock = inputs().unwrap();
        let mut records = vec![];
        for relative in lock
            .runtime_files
            .iter()
            .map(String::as_str)
            .chain(["include/ctranslate2/translator.h", "notices/LICENSE.txt"])
        {
            let path = temp.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "fixture").unwrap();
            record(
                temp.path(),
                relative,
                "fixture",
                "https://example.invalid/source",
                "notices/LICENSE.txt",
                &mut records,
            )
            .unwrap();
        }
        let baseline = serde_json::to_string(&Inventory {
            schema_version: 1,
            ct2_revision: lock.ct2_revision.clone(),
            files: records,
        })
        .unwrap();
        for scenario in [
            "valid",
            "revision",
            "hash",
            "size",
            "duplicate",
            "traversal",
            "license",
            "missing-crt",
            "origin",
            "untracked",
        ] {
            let mut inventory: Inventory = serde_json::from_str(&baseline).unwrap();
            match scenario {
                "revision" => inventory.ct2_revision = "wrong".into(),
                "hash" => inventory.files[0].sha256 = "0".repeat(64),
                "size" => inventory.files[0].bytes += 1,
                "duplicate" => inventory.files.push(
                    serde_json::from_str(&serde_json::to_string(&inventory.files[0]).unwrap())
                        .unwrap(),
                ),
                "traversal" => inventory.files[0].path = "../outside".into(),
                "license" => inventory.files[0].license_path = "notices/missing".into(),
                "missing-crt" => inventory.files.retain(|e| e.path != "msvcp140_1.dll"),
                "origin" => inventory.files[0].origin = "http://example.invalid/source".into(),
                "untracked" => fs::write(temp.path().join("unknown.dll"), "extra").unwrap(),
                _ => (),
            }
            files::write_json(&temp.path().join("notices/inventory.json"), &inventory).unwrap();
            assert_eq!(
                validate(temp.path(), &lock).is_ok(),
                scenario == "valid",
                "{scenario}"
            );
        }
    }

    #[test]
    fn archive_mapping_rejects_missing_duplicate_and_unsafe_payloads() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("input.zip");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for _ in 0..2 {
            zip.start_file("native.dll", zip::write::FileOptions::default())
                .unwrap();
            zip.write_all(b"fixture").unwrap();
        }
        zip.finish().unwrap();
        for (entry, destination) in [
            ("missing", "out.dll"),
            ("native.dll", "out.dll"),
            ("native.dll", "../escape.dll"),
        ] {
            let output = tempfile::tempdir().unwrap();
            assert!(
                extract(
                    &path,
                    &BTreeMap::from([(entry.into(), destination.into())]),
                    output.path()
                )
                .is_err()
            );
        }
    }
}
