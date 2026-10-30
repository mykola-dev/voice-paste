use anyhow::{Result, bail, ensure};

pub(crate) fn notes(manifest: &str, changelog: &str, tag: &str) -> Result<String> {
    let version = tag.strip_prefix('v').unwrap_or_default();
    let parts: Vec<_> = version.split('.').collect();
    ensure!(
        parts.len() == 3
            && parts.iter().all(|part| {
                !part.is_empty()
                    && part.bytes().all(|byte| byte.is_ascii_digit())
                    && (part.len() == 1 || !part.starts_with('0'))
            }),
        "Expected a stable release tag vMAJOR.MINOR.PATCH"
    );
    let mut workspace = false;
    let mut manifest_version = None;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            workspace = line == "[workspace.package]";
        } else if workspace
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "version"
        {
            manifest_version = Some(value.trim().trim_matches('"'));
        }
    }
    ensure!(
        manifest_version == Some(version),
        "Tag/workspace version mismatch"
    );
    let heading = format!("## [{version}]");
    let mut found = false;
    let mut collecting = false;
    let mut body = Vec::new();
    for line in changelog.lines() {
        if line.starts_with("## ") {
            collecting = line == heading
                || line
                    .strip_prefix(&heading)
                    .is_some_and(|suffix| suffix.starts_with(" - "));
            if collecting {
                ensure!(!found, "Duplicate changelog section for {version}");
                found = true;
            }
        } else if collecting {
            body.push(line);
        }
    }
    if !found {
        bail!("Missing changelog section for {version}");
    }
    ensure!(
        body.iter().any(|line| line
            .strip_prefix("- ")
            .is_some_and(|text| !text.trim().is_empty())),
        "Release changelog must contain a nonempty bullet"
    );
    Ok(format!("{}\n", body.join("\n").trim()))
}

#[cfg(test)]
mod tests {
    use super::notes;

    const MANIFEST: &str = "[workspace.package]\nversion = \"2.0.0\"\n";

    #[test]
    fn extracts_only_selected_release_with_lf_or_crlf() {
        for newline in ["\n", "\r\n"] {
            let changelog = "# Changelog\n\n## [Unreleased]\n- Future\n\n## [2.0.0] - 2026-10-29\n\n### Added\n- Rust rewrite\n\n## [1.0.0]\n- Old\n".replace('\n', newline);
            assert_eq!(
                notes(MANIFEST, &changelog, "v2.0.0").unwrap(),
                "### Added\n- Rust rewrite\n"
            );
        }
    }

    #[test]
    fn accepts_undated_historical_release() {
        assert_eq!(
            notes(MANIFEST, "## [2.0.0]\n- Rewrite", "v2.0.0").unwrap(),
            "- Rewrite\n"
        );
    }

    #[test]
    fn rejects_missing_empty_duplicate_or_prefix_match() {
        for changelog in [
            "## [Unreleased]\n- Future",
            "## [2.0.0]\n",
            "## [2.0.0]\n- ",
            "## [2.0.0]\n- One\n## [2.0.0]\n- Two",
            "## [2.0.0]extra\n- Wrong",
        ] {
            assert!(notes(MANIFEST, changelog, "v2.0.0").is_err());
        }
    }

    #[test]
    fn rejects_invalid_or_mismatched_tag() {
        for tag in [
            "2.0.0",
            "v2.0",
            "v02.0.0",
            "v2.0.0-beta.1",
            "v2.1.0",
            "v../../x",
        ] {
            assert!(notes(MANIFEST, "## [2.0.0]\n- Rewrite", tag).is_err());
        }
    }
}
