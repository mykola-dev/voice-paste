//! Shared raw multipart delivery; native adapters only transfer one HTTP asset.
use super::model_preparation::{Progress, check_cancel, publish, verify};
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    sync::atomic::AtomicBool,
};
use voicepaste_core::models::Artifact;

const BASE: &str = "https://github.com/mykola-dev/voice-paste/releases/download/models";
const MANIFEST: &str = include_str!("../../../../crates/core/assets/model-mirror.json");

#[derive(Clone, Deserialize)]
pub(crate) struct Mirror {
    name: String,
    bytes: u64,
    sha256: String,
    parts: Vec<Part>,
}
#[derive(Clone, Deserialize)]
struct Part {
    name: String,
    bytes: u64,
    sha256: String,
}

pub(crate) fn mirror(artifact: &Artifact) -> Result<Mirror, String> {
    let entries: Vec<Mirror> = serde_json::from_str(MANIFEST).map_err(|e| e.to_string())?;
    let entry = entries
        .into_iter()
        .find(|m| m.name == artifact.path)
        .ok_or_else(|| format!("No release mirror for {}", artifact.path))?;
    entry.validate(artifact)?;
    Ok(entry)
}

impl Mirror {
    fn validate(&self, artifact: &Artifact) -> Result<(), String> {
        let mut names = HashSet::new();
        let mut total = Some(0u64);
        for part in &self.parts {
            if part.name.is_empty()
                || !part
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
                || part.name == "."
                || part.name == ".."
                || !names.insert(&part.name)
                || part.bytes == 0
                || part.bytes > 1_800_000_000
                || part.sha256.len() != 64
                || !part.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("Invalid release mirror part manifest".into());
            }
            total = total.and_then(|n| n.checked_add(part.bytes));
        }
        if self.parts.is_empty()
            || self.name != artifact.path
            || self.bytes != artifact.bytes
            || !self.sha256.eq_ignore_ascii_case(&artifact.sha256)
            || total != Some(self.bytes)
        {
            return Err("Release mirror does not match pinned artifact identity".into());
        }
        Ok(())
    }
}

/// Caller holds the cache writer lock. Short part files resume; complete corrupt
/// parts restart only with consent. Assembly is bounded and never the destination.
#[allow(
    clippy::too_many_arguments,
    reason = "Explicit scenario inputs and native effects keep platform policy outside the shared lifecycle"
)]
pub(crate) fn prepare(
    paths: (&Path, &Path),
    artifact: &Artifact,
    entry: impl FnOnce() -> Result<Mirror, String>,
    cancel: &AtomicBool,
    allow_network: bool,
    status: &Progress,
    mut open: impl FnMut(&Path) -> Result<File, String>,
    mut transfer: impl FnMut(&str, &Path, u64, u64) -> Result<(), String>,
) -> Result<(), String> {
    let (partial, destination) = paths;
    if publish(partial, destination, artifact, cancel)? {
        return Ok(());
    }
    let entry = entry()?;
    entry.validate(artifact)?;
    let directory = partial.parent().ok_or("Missing model staging directory")?;
    for (index, part) in entry.parts.iter().enumerate() {
        check_cancel(cancel)?;
        let path = directory.join(format!("{}.partial", part.name));
        let pinned = Artifact {
            path: part.name.clone(),
            bytes: part.bytes,
            sha256: part.sha256.clone(),
        };
        status.message(format!(
            "Verifying {} part {}/{}",
            artifact.path,
            index + 1,
            entry.parts.len()
        ));
        if verify(&path, &pinned, cancel)? {
            continue;
        }
        if !allow_network {
            return Err(
                "The selected model needs to be downloaded. Start recording to approve or retry."
                    .into(),
            );
        }
        check_cancel(cancel)?;
        // verify() already rejected non-file staging, including symbolic links.
        let file = open(&path)?;
        let mut offset = file.metadata().map_err(|e| e.to_string())?.len();
        if offset >= part.bytes {
            file.set_len(0).map_err(|e| e.to_string())?;
            offset = 0;
        }
        drop(file);
        status.message(format!(
            "Downloading {} part {}/{}; resumable",
            artifact.path,
            index + 1,
            entry.parts.len()
        ));
        status.track_file(path.clone(), part.bytes);
        struct Active<'a>(&'a Progress);
        impl Drop for Active<'_> {
            fn drop(&mut self) {
                self.0.set_downloading(false);
            }
        }
        status.set_downloading(true);
        let active = Active(status);
        transfer(&format!("{BASE}/{}", part.name), &path, offset, part.bytes)?;
        drop(active);
        if !verify(&path, &pinned, cancel)? {
            return Err(format!(
                "Release part {} failed exact size/SHA-256 verification; not published",
                part.name
            ));
        }
    }
    check_cancel(cancel)?;
    // A prior complete partial may have been corrupt. Only verified parts can
    // replace it, including offline recovery from previously downloaded parts.
    if fs::symlink_metadata(partial).is_ok_and(|m| !m.is_file()) {
        return Err("Refusing non-file model staging path".into());
    }
    status.message(format!("Assembling {}", artifact.path));
    let mut output = open(partial)?;
    output.set_len(0).map_err(|e| e.to_string())?;
    let mut buffer = [0u8; 256 * 1024];
    for part in &entry.parts {
        let mut input = File::open(directory.join(format!("{}.partial", part.name)))
            .map_err(|e| e.to_string())?;
        let mut remaining = part.bytes;
        while remaining != 0 {
            check_cancel(cancel)?;
            let limit = remaining.min(buffer.len() as u64) as usize;
            let count = input
                .read(&mut buffer[..limit])
                .map_err(|e| e.to_string())?;
            if count == 0 {
                return Err("Release part truncated during assembly".into());
            }
            output
                .write_all(&buffer[..count])
                .map_err(|e| e.to_string())?;
            remaining -= count as u64;
        }
    }
    drop(output);
    status.message(format!("Verifying {}", artifact.path));
    if !publish(partial, destination, artifact, cancel)? {
        return Err(
            "Assembled release artifact failed exact size/SHA-256 verification; not published"
                .into(),
        );
    }
    // Best-effort cache cleanup only after verified publication; failures merely
    // retain reusable verified parts, never invalidate the published artifact.
    for part in &entry.parts {
        let _ = fs::remove_file(directory.join(format!("{}.partial", part.name)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        io::{Seek, SeekFrom},
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    const DATA: &[u8] = b"tiny raw model weights";
    struct Fixture {
        root: PathBuf,
        artifact: Artifact,
        mirror: Mirror,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "voicepaste-parts-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let artifact = Artifact {
                path: "model.bin".into(),
                bytes: DATA.len() as u64,
                sha256: digest(DATA),
            };
            let mirror = Mirror {
                name: artifact.path.clone(),
                bytes: artifact.bytes,
                sha256: artifact.sha256.clone(),
                parts: vec![
                    Part {
                        name: "model.bin.part001".into(),
                        bytes: 5,
                        sha256: digest(&DATA[..5]),
                    },
                    Part {
                        name: "model.bin.part002".into(),
                        bytes: (DATA.len() - 5) as u64,
                        sha256: digest(&DATA[5..]),
                    },
                ],
            };
            Self {
                root,
                artifact,
                mirror,
            }
        }
        fn run(
            &self,
            network: bool,
            cancel: &AtomicBool,
            transfer: impl FnMut(&str, &Path, u64, u64) -> Result<(), String>,
        ) -> Result<(), String> {
            prepare(
                (
                    &self.root.join("model.partial"),
                    &self.root.join("model.bin"),
                ),
                &self.artifact,
                || Ok(self.mirror.clone()),
                cancel,
                network,
                &Progress::staging_file(String::new()),
                |path| {
                    fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(false)
                        .open(path)
                        .map_err(|e| e.to_string())
                },
                transfer,
            )
        }
        fn part(&self, index: usize) -> PathBuf {
            self.root
                .join(format!("{}.partial", self.mirror.parts[index].name))
        }
        fn seed(&self) {
            fs::write(self.part(0), &DATA[..5]).unwrap();
            fs::write(self.part(1), &DATA[5..]).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn digest(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }
    fn deliver(url: &str, path: &Path, offset: u64, size: u64) -> Result<(), String> {
        assert!(url.starts_with(&format!("{BASE}/")));
        let bytes = if url.ends_with("part001") {
            &DATA[..5]
        } else {
            &DATA[5..]
        };
        assert_eq!(size, bytes.len() as u64);
        let mut file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        file.write_all(&bytes[offset as usize..]).unwrap();
        Ok(())
    }
    #[test]
    fn short_part_resumes_and_verified_parts_are_skipped_then_raw_concatenated() {
        let fixture = Fixture::new();
        fs::write(fixture.part(0), &DATA[..5]).unwrap();
        fs::write(fixture.part(1), &DATA[5..8]).unwrap();
        let mut calls = 0;
        fixture
            .run(true, &AtomicBool::new(false), |url, path, offset, size| {
                calls += 1;
                assert!(url.ends_with("part002"));
                assert_eq!(offset, 3);
                deliver(url, path, offset, size)
            })
            .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(fs::read(fixture.root.join("model.bin")).unwrap(), DATA);
        assert!(!fixture.part(0).exists());
        assert!(!fixture.part(1).exists());
    }
    #[test]
    fn corrupt_complete_and_oversized_parts_reset_only_with_consent() {
        for bytes in [b"xxxxx".as_slice(), b"oversized"] {
            let fixture = Fixture::new();
            fs::write(fixture.part(0), bytes).unwrap();
            assert!(
                fixture
                    .run(false, &AtomicBool::new(false), |_, _, _, _| panic!(
                        "offline transport"
                    ))
                    .is_err()
            );
            assert_eq!(fs::read(fixture.part(0)).unwrap(), bytes);
            fixture
                .run(true, &AtomicBool::new(false), |url, path, offset, size| {
                    assert_eq!(offset, 0);
                    deliver(url, path, offset, size)
                })
                .unwrap();
        }
    }
    #[test]
    fn complete_parts_recover_offline_and_complete_artifact_partial_needs_no_parts() {
        for complete_artifact in [false, true] {
            let fixture = Fixture::new();
            if complete_artifact {
                fs::write(fixture.root.join("model.partial"), DATA).unwrap();
            } else {
                fixture.seed();
            }
            fixture
                .run(false, &AtomicBool::new(false), |_, _, _, _| {
                    panic!("offline transport")
                })
                .unwrap();
            assert_eq!(fs::read(fixture.root.join("model.bin")).unwrap(), DATA);
        }
    }
    #[test]
    fn bad_part_or_full_hash_never_replaces_destination() {
        for bad_part in [false, true] {
            let mut fixture = Fixture::new();
            fs::write(fixture.root.join("model.bin"), b"previous").unwrap();
            if !bad_part {
                fixture.seed();
                fixture.artifact.sha256 = digest(b"wrong full identity");
                fixture.mirror.sha256 = fixture.artifact.sha256.clone();
            }
            assert!(
                fixture
                    .run(true, &AtomicBool::new(false), |_, path, _, size| {
                        fs::write(path, vec![b'x'; size as usize]).unwrap();
                        Ok(())
                    })
                    .is_err()
            );
            assert_eq!(
                fs::read(fixture.root.join("model.bin")).unwrap(),
                b"previous"
            );
        }
    }
    #[test]
    fn interruption_and_cancellation_retain_parts_and_never_publish_or_start_next_part() {
        for cancelled in [false, true] {
            let fixture = Fixture::new();
            let cancel = AtomicBool::new(false);
            let mut calls = 0;
            assert!(
                fixture
                    .run(true, &cancel, |_, path, _, _| {
                        calls += 1;
                        fs::write(path, b"ti").unwrap();
                        cancel.store(cancelled, Ordering::Release);
                        if cancelled {
                            Ok(())
                        } else {
                            Err("interrupted".into())
                        }
                    })
                    .is_err()
            );
            assert_eq!(calls, 1);
            assert_eq!(fs::read(fixture.part(0)).unwrap(), b"ti");
            assert!(!fixture.root.join("model.bin").exists());
        }
    }
    #[test]
    fn cancelled_assembly_keeps_previous_destination() {
        let fixture = Fixture::new();
        fixture.seed();
        fs::write(fixture.root.join("model.bin"), b"previous").unwrap();
        let cancel = AtomicBool::new(false);
        assert!(
            prepare(
                (
                    &fixture.root.join("model.partial"),
                    &fixture.root.join("model.bin")
                ),
                &fixture.artifact,
                || Ok(fixture.mirror.clone()),
                &cancel,
                false,
                &Progress::staging_file(String::new()),
                |path| {
                    let file = fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(false)
                        .open(path)
                        .map_err(|e| e.to_string())?;
                    // Deterministically cancel after part verification, exactly
                    // when assembly acquires its output, without timing sleeps.
                    cancel.store(true, Ordering::Release);
                    Ok(file)
                },
                |_, _, _, _| panic!("offline transport"),
            )
            .is_err()
        );
        assert_eq!(
            fs::read(fixture.root.join("model.bin")).unwrap(),
            b"previous"
        );
    }
    #[test]
    fn manifest_rejects_paths_duplicate_parts_size_and_identity_drift() {
        for mutation in 0..5 {
            let mut fixture = Fixture::new();
            match mutation {
                0 => fixture.mirror.parts[0].name = "../escape".into(),
                1 => fixture.mirror.parts[1].name = fixture.mirror.parts[0].name.clone(),
                2 => fixture.mirror.parts[0].bytes = 1_800_000_001,
                3 => fixture.mirror.sha256 = digest(b"wrong"),
                _ => fixture.mirror.parts[0].bytes = 1,
            }
            assert!(
                fixture
                    .run(true, &AtomicBool::new(false), |_, _, _, _| panic!(
                        "invalid manifest transport"
                    ))
                    .is_err()
            );
        }
    }
    #[test]
    fn embedded_mirrors_match_every_pinned_macos_artifact() {
        use voicepaste_core::{models, settings::SettingsPlatform};
        for name in ["large-v2", "large-v3", "large-v3-turbo"] {
            let model = models::find_for(name, SettingsPlatform::Macos).unwrap();
            for artifact in model.files {
                mirror(&artifact).unwrap();
            }
        }
        for artifact in models::macos_vad().unwrap().files {
            mirror(&artifact).unwrap();
        }
    }
}
