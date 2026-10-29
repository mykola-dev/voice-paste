//! Source-level guard, not a claim of non-Windows host compilation/qualification.
#[test]
fn shared_coordination_and_policies_do_not_import_native_apis() {
    let sources = [
        ("main", include_str!("../main.rs")),
        ("runtime", include_str!("../runtime.rs")),
        ("storage", include_str!("storage.rs")),
        ("diagnostics", include_str!("diagnostics.rs")),
        ("models", include_str!("models.rs")),
        ("receipts", include_str!("models/verification.rs")),
        ("worker", include_str!("worker.rs")),
        ("transport", include_str!("worker/transport.rs")),
        ("session", include_str!("platform/session.rs")),
        ("settings", include_str!("settings_window.rs")),
        ("overlay", include_str!("overlay.rs")),
        ("tray artwork", include_str!("tray_icons.rs")),
        ("memory projection", include_str!("memory.rs")),
    ];
    for (name, source) in sources {
        for native in [
            "windows_sys::",
            "std::os::windows",
            "os::windows::",
            "RawWindowHandle",
            "unsafe {",
        ] {
            assert!(
                !source.contains(native),
                "native API escaped into {name}: {native}"
            );
        }
    }
}
