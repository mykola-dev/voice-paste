use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_ATTEMPTS: usize = 128;
static NEXT_NAME: AtomicU64 = AtomicU64::new(0);

// Only names successfully created by this process may acquire an AudioFile owner.
pub(super) fn create(
    root: &Path,
    prefix: &str,
    mut options: OpenOptions,
) -> io::Result<(PathBuf, File)> {
    create_with_counter(root, prefix, &mut options, &NEXT_NAME)
}

fn create_with_counter(
    root: &Path,
    prefix: &str,
    options: &mut OpenOptions,
    counter: &AtomicU64,
) -> io::Result<(PathBuf, File)> {
    options.write(true).create_new(true);
    for _ in 0..MAX_ATTEMPTS {
        let number = counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| io::Error::other("Audio filename counter exhausted"))?;
        let path = root.join(format!("{prefix}-{number}.wav"));
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "Audio filename attempts exhausted",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashSet, fs, io::Write, sync::Arc, thread};

    struct TestDir(PathBuf);
    impl TestDir {
        fn new(label: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("voicepaste-unique-{label}-{}", std::process::id()));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn collisions_preserve_bytes_and_non_ascii_root() {
        let dir = TestDir::new("тест");
        let prefix = "recording-42-1";
        let old = dir.0.join(format!("{prefix}-0.wav"));
        fs::write(&old, b"retained recording").unwrap();
        let counter = AtomicU64::new(0);
        let (path, mut file) =
            create_with_counter(&dir.0, prefix, &mut OpenOptions::new(), &counter).unwrap();
        file.write_all(b"new").unwrap();
        drop(file);
        assert_eq!(fs::read(old).unwrap(), b"retained recording");
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert!(
            path.canonicalize()
                .unwrap()
                .starts_with(dir.0.canonicalize().unwrap())
        );
    }

    #[test]
    fn exhausted_collisions_and_other_errors_fail_closed() {
        let dir = TestDir::new("exhausted");
        for n in 0..MAX_ATTEMPTS {
            fs::write(dir.0.join(format!("file-request-7-0-{n}.wav")), b"old").unwrap();
        }
        let counter = AtomicU64::new(0);
        let result = create_with_counter(
            &dir.0,
            "file-request-7-0",
            &mut OpenOptions::new(),
            &counter,
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        for n in 0..MAX_ATTEMPTS {
            assert_eq!(
                fs::read(dir.0.join(format!("file-request-7-0-{n}.wav"))).unwrap(),
                b"old"
            );
        }
        let counter = AtomicU64::new(0);
        assert!(
            create_with_counter(
                &dir.0.join("missing"),
                "file-request-7-0",
                &mut OpenOptions::new(),
                &counter
            )
            .is_err()
        );
        assert_eq!(counter.load(Ordering::Relaxed), 1);
        let counter = AtomicU64::new(u64::MAX);
        assert!(
            create_with_counter(
                &dir.0,
                "file-request-7-0",
                &mut OpenOptions::new(),
                &counter
            )
            .is_err()
        );
    }

    #[test]
    fn parallel_creations_get_distinct_paths() {
        let dir = TestDir::new("parallel");
        let counter = Arc::new(AtomicU64::new(0));
        let handles: Vec<_> = (0..24)
            .map(|n| {
                let root = dir.0.clone();
                let counter = counter.clone();
                thread::spawn(move || {
                    let (path, mut file) = create_with_counter(
                        &root,
                        "recording-1-1",
                        &mut OpenOptions::new(),
                        &counter,
                    )
                    .unwrap();
                    file.write_all(&[n]).unwrap();
                    (path, n)
                })
            })
            .collect();
        let mut paths = HashSet::new();
        for handle in handles {
            let (path, byte) = handle.join().unwrap();
            assert!(paths.insert(path.clone()));
            assert_eq!(fs::read(path).unwrap(), [byte]);
        }
    }
}
