# Documentation

Current Rust application:

- [Project and quick start](../README.md)
- [Windows build, packaging and release automation](build-windows.md)
- [Native CI inputs and license inventory](native-inputs.md)
- [Host implementation](../apps/voicepaste/README.md)
- [Worker/native prerequisites](../apps/stt-worker/README.md)
- [Behavior contract](behavior.md)
- [Architecture](architecture.md)
- [Cross-platform boundaries and qualification](cross-platform-roadmap.md)
- [Engineering rules](engineering-principles.md)
- [Settings UI requirements](settings-ui-engineering.md)
- [Public default settings](../crates/core/assets/default-settings.json)
- [Unfinished work](../TODO.md)
- [Windows reliability/package recheck (2026-10-29)](verification-2026-10-29.md)

The numbered documents and `RUNNING.md` in this directory describe the removed
C#/WPF/Python application. They are retained only as historical behavior/design
references; their commands, dependencies, defaults and status are not current.
Source paths in those documents refer to the Git baseline, not the present tree.
Use the current behavior contract and frozen snapshot when the references differ.
