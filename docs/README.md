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
- [Apple Silicon macOS beta and device evidence](macos-preview.md)
- [macOS model delivery and historical investigation](macos-model-delivery.md)
- [Engineering rules](engineering-principles.md)
- [Settings UI requirements](settings-ui-engineering.md)
- [Public default settings](../crates/core/assets/default-settings.json)
- [Revision plan and unfinished work](revision-plan.md)
- [Windows reliability/package recheck (2026-10-29)](verification-2026-10-29.md)
- [Shared-session and provisioning verification (2026-10-31)](verification-2026-10-31.md)

The numbered documents and `RUNNING.md` in this directory describe the removed
C#/WPF/Python application. They are retained only as historical behavior/design
references; their commands, dependencies, defaults and status are not current.
Source paths in those documents refer to the Git baseline, not the present tree.
Use the current behavior contract and public defaults when the references differ.
