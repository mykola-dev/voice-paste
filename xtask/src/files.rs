use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub fn hash_reader(mut reader: impl Read) -> Result<String> {
    let mut hash = Sha256::new();
    let mut buffer = [0; 128 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn hash(path: &Path) -> Result<String> {
    hash_reader(File::open(path).with_context(|| format!("Opening {}", path.display()))?)
}

/// Portable Windows-relative names only; manifests must never own personal data.
pub fn safe_path(path: &str) -> Result<()> {
    ensure!(!path.is_empty(), "Empty managed path");
    for part in path.split('/') {
        ensure!(
            !part.is_empty() && part != "." && part != "..",
            "Unsafe path: {path}"
        );
        ensure!(!part.ends_with([' ', '.']), "Unsafe Windows path: {path}");
        ensure!(
            !part
                .chars()
                .any(|c| c.is_control() || "\\:<>\"|?*".contains(c)),
            "Unsafe path: {path}"
        );
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        ensure!(
            !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(1..=9).any(|n| stem == format!("COM{n}") || stem == format!("LPT{n}")),
            "Reserved Windows path: {path}"
        );
        ensure!(
            !part.eq_ignore_ascii_case("data") && !part.eq_ignore_ascii_case("config.json"),
            "Personal data cannot be managed: {path}"
        );
    }
    Ok(())
}

pub fn no_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                ensure!(
                    !meta.file_type().is_symlink(),
                    "Symlink not allowed: {}",
                    ancestor.display()
                );
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        meta.file_attributes() & 0x400 == 0,
                        "Reparse point not allowed: {}",
                        ancestor.display()
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

pub fn copy(source: &Path, destination: &Path) -> Result<()> {
    no_links(source)?;
    no_links(destination)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)
        .with_context(|| format!("Copying {} to {}", source.display(), destination.display()))?;
    Ok(())
}

pub fn walk(directory: &Path) -> Result<Vec<String>> {
    fn visit(root: &Path, dir: &Path, result: &mut Vec<String>) -> Result<()> {
        no_links(dir)?;
        for item in fs::read_dir(dir)? {
            let item = item?;
            let kind = item.file_type()?;
            no_links(&item.path())?;
            if kind.is_dir() {
                visit(root, &item.path(), result)?;
            } else if kind.is_file() {
                let relative = item
                    .path()
                    .strip_prefix(root)?
                    .to_str()
                    .context("Non-Unicode package path")?
                    .replace('\\', "/");
                safe_path(&relative)?;
                result.push(relative);
            } else {
                bail!("Unsupported artifact: {}", item.path().display());
            }
        }
        Ok(())
    }
    let mut result = vec![];
    visit(directory, directory, &mut result)?;
    result.sort();
    Ok(result)
}

pub fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    Ok(())
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("Reading {}", path.display()))?;
    // Compatibility with existing PowerShell-generated manifests.
    Ok(serde_json::from_slice(
        bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes),
    )?)
}

pub fn absolute(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.into()
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_and_private_names() {
        for path in [
            "",
            "../x",
            "a/./x",
            "/x",
            "a\\b",
            "C:/x",
            "a//x",
            "a./x",
            "x ",
            "x:stream",
            "CON.txt",
            "com1",
            "DATA/settings.json",
            "x/config.json",
        ] {
            assert!(safe_path(path).is_err(), "{path}");
        }
        safe_path("runtime/ctranslate2.dll").unwrap();
    }
}
