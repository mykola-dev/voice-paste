---
name: Release
description: Prepare or publish a VoicePaste release when the user asks to release, ship, tag, bump a release version, or publish to GitHub. Maintain the changelog, validate packages, commit, tag, push and verify GitHub Actions.
---

# VoicePaste release

Paths below are relative to the repository root, not this skill directory.
Follow `AGENTS.md` and the current build documentation. Run Windows processes
without console windows or focus theft. Never stop an app, interrupt recording,
discard Settings drafts, download models or install SDKs without permission.

## Authorization

- Preparing release infrastructure is not permission to publish a release.
- A direct request to release a specified version authorizes the release commit,
  annotated tag and push. A preparation-only request does not authorize these.
- Ask for the version if missing; normalize `2.1` to `2.1.0` and confirm the
  interpretation if ambiguous. Use stable `MAJOR.MINOR.PATCH` and `vMAJOR.MINOR.PATCH`.
- Never choose the next version automatically, force-push, move/delete a tag,
  overwrite a published release or bypass failed checks.

## Preflight

1. Inspect branch, status, diff, remotes, local/remote tags and latest release.
   Work on `master`; verify the intended remote and that the checkout is current.
   Stop for divergence or an existing target tag/release; do not pull/rebase blindly.
2. Identify all pending changes. Never stage everything blindly or commit unrelated
   user work. If intended release content is unclear, ask before proceeding.
3. Read `CHANGELOG.md`, `Cargo.toml`, the release workflow and relevant build docs.
   Compare the previous release tag with intended release content. If no previous
   tag exists, inspect history and agree on the baseline; do not invent old tags.

## Prepare

1. Review `Unreleased` against the actual changes. Keep short English one-line
   user-facing bullets; combine duplicates, remove implementation noise and verify
   claims. Do not turn known open defects into claims of fixes. No unrelated code
   cleanup, dependency upgrades or settings/default changes as part of release.
2. Require meaningful release notes. Rename `## [Unreleased]` to
   `## [VERSION] - YYYY-MM-DD` using the actual release date, and insert a fresh
   empty `## [Unreleased]` above it. Preserve earlier release notes.
3. Update `[workspace.package].version` in root `Cargo.toml` and corresponding local
   workspace package versions in `Cargo.lock`, without upgrading dependencies.
   Inspect the diff for stale version references; do not rewrite historical notes,
   protocol versions, third-party versions or unrelated test fixtures.
4. Validate the exact release body with
   `cargo xtask release-notes --tag vVERSION`. It must contain only this release.

## Validate

1. Run `cargo xtask fmt`, `cargo xtask clippy` and `cargo xtask test`.
2. Build/check the Release package using `build-portable-windows.bat --check`.
   It updates `build/release/`; ask first if updating it would affect a running app.
   Do not publish raw Cargo binaries. Preserve `data/`; never archive personal data.
3. Inspect the final version, changelog, diff and ZIP/checksum output. Report any
   unrun checks honestly. Failed or blocked required validation stops publication.

## Publish (only when authorized)

1. Stage explicitly reviewed release files, commit as `Release VERSION`, and verify
   that the commit contains all intended changes and no unrelated files.
2. Create annotated tag `vVERSION` on that commit. Push the explicit `master` branch
   and exact tag to the verified remote, preferably together with `git push --atomic`.
   If atomic push is unsupported, report and agree on a safe retry; never force it.
3. The tag triggers `.github/workflows/release-windows.yml`: it validates the tag,
   workspace version and changelog, builds/checks the package, verifies checksums,
   creates a draft with the extracted changelog body, uploads ZIP/checksum and
   publishes. Do not create a competing GitHub Release manually.
4. Use GitHub CLI to check the run associated with this tag/commit and its result.
   Verify the published release's tag, description and ZIP/checksum assets. Do not
   claim success just because push succeeded. If blocked or still running, say so.
5. On failure, report the stage and preserve evidence. Do not retag/rewrite history
   or bypass checks. Published releases are immutable here; use a new version for
   corrections. A draft can be retried through the workflow after diagnosis.

## Report

State the version, release commit/tag, checks performed and GitHub Release URL.
Clearly distinguish prepared, pushed, CI pending, failed and actually published.
