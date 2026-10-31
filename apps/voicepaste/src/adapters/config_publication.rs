//! Shared config staging lifetime; native adapters select permissions and commit.
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) fn stage(
    destination: &Path,
    bytes: &[u8],
    open: impl FnOnce(&Path) -> Result<File, String>,
    commit: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temporary = destination
        .parent()
        .ok_or("Missing settings parent directory")?
        .join(format!(
            ".config-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
    // Ownership starts only after create_new succeeds. Never delete a colliding
    // path belonging to another operation when staging creation fails.
    let mut file = open(&temporary)?;
    struct Staging(PathBuf);
    impl Drop for Staging {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _staging = Staging(temporary.clone());
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    commit(&temporary)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_commit_removes_only_owned_staging_and_preserves_existing_config() {
        let root =
            std::env::temp_dir().join(format!("voicepaste-config-stage-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _root = Owned(root.clone());
        let config = root.join("config.json");
        fs::write(&config, b"previous config").unwrap();
        let result = stage(
            &config,
            b"new config",
            |path| {
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(|e| e.to_string())
            },
            |path| {
                assert_eq!(fs::read(path).unwrap(), b"new config");
                Err("commit failed".into())
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(&config).unwrap(), b"previous config");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        let mut collision = None;
        assert!(
            stage(
                &config,
                b"new config",
                |path| {
                    fs::write(path, b"unrelated staging").unwrap();
                    collision = Some(path.to_owned());
                    Err("staging already exists".into())
                },
                |_| panic!("must not commit")
            )
            .is_err()
        );
        assert_eq!(fs::read(collision.unwrap()).unwrap(), b"unrelated staging");
    }
}
