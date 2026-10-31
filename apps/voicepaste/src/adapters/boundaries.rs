//! Source-level guard, not a claim of non-Windows host compilation/qualification.
#[test]
fn shared_coordination_and_policies_do_not_import_native_apis() {
    let sources = [
        ("main", include_str!("../main.rs")),
        ("runtime", include_str!("../runtime.rs")),
        ("storage", include_str!("storage.rs")),
        ("config publication", include_str!("config_publication.rs")),
        ("diagnostics", include_str!("diagnostics.rs")),
        ("models", include_str!("models.rs")),
        ("model preparation", include_str!("model_preparation.rs")),
        ("receipts", include_str!("models/verification.rs")),
        ("worker", include_str!("worker.rs")),
        ("transport", include_str!("worker/transport.rs")),
        ("session", include_str!("platform/session.rs")),
        ("shared UI session", include_str!("ui_session.rs")),
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
#[test]
fn native_session_hosts_do_not_own_application_scenarios() {
    for source in [
        include_str!("platform/windows/session.rs"),
        include_str!("platform/macos/session.rs"),
    ] {
        for forbidden in ["Command::", "SettingsSession", "OverlayWindow", "loop {"] {
            assert!(
                !source.contains(forbidden),
                "native session host contains {forbidden}"
            );
        }
    }
    assert!(!include_str!("settings_window.rs").contains("PLATFORM =="));
}
