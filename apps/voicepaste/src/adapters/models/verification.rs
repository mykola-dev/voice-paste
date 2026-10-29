//! Local verification receipts, not a security boundary against deliberate tampering.
//! Changed size/creation/write time or pinned manifest invalidates the receipt.
use crate::adapters::files;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use voicepaste_core::models::Model;

#[derive(Serialize, Deserialize, PartialEq)]
struct FileStamp {
    path: String,
    sha256: String,
    bytes: u64,
    created: u64,
    modified: u64,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub(super) struct Receipt {
    version: u32,
    revision: String,
    files: Vec<FileStamp>,
}

impl Receipt {
    pub(super) fn inspect(directory: &Path, model: &Model) -> Option<Self> {
        let files = model
            .files
            .iter()
            .map(|artifact| {
                let metadata = fs::metadata(directory.join(&artifact.path)).ok()?;
                if !metadata.is_file() || metadata.len() != artifact.bytes {
                    return None;
                }
                let (created, modified) = super::receipt_times(&metadata);
                Some(FileStamp {
                    path: artifact.path.clone(),
                    sha256: artifact.sha256.clone(),
                    bytes: metadata.len(),
                    created,
                    modified,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            version: 1,
            revision: model.revision.clone(),
            files,
        })
    }

    pub(super) fn current(directory: &Path, model: &Model) -> bool {
        let Some(current) = Self::inspect(directory, model) else {
            return false;
        };
        fs::read(directory.join(".verified.json"))
            .ok()
            .and_then(|data| serde_json::from_slice::<Self>(&data).ok())
            .is_some_and(|saved| saved == current)
    }

    pub(super) fn save(&self, directory: &Path) -> Result<(), String> {
        use std::io::Write;
        // Caller holds the exclusive model lock. A crash leaves only a cache
        // miss; the receipt is published only after all pinned hashes passed.
        let temporary = directory.join(".verified.json.partial");
        let mut file = files::exclusive_options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        let data = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        file.write_all(&data)
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        files::replace(&temporary, &directory.join(".verified.json"))
    }
}
