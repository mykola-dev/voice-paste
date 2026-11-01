use super::*;
use std::cell::Cell;

#[test]
fn source_switch_does_not_show_stale_http_byte_counts() {
    let status = Progress::cumulative("HTTPS".into(), 100);
    status.bytes(50);
    status.clear_tracked_file();
    status.message("Git LFS".into());
    assert_eq!(status.snapshot(), "Git LFS");
    status.bytes(100);
    assert!(status.snapshot().contains("MiB"));
}

#[test]
fn verified_primary_does_not_use_fallback() {
    transfer_with_fallback(
        &AtomicBool::new(false),
        &mut (),
        |_| Ok(()),
        |_| Ok(true),
        |_| panic!("unexpected fallback"),
    )
    .unwrap();
}

#[test]
fn failed_or_invalid_primary_switches_once() {
    for primary in [Ok(()), Err("offline".into())] {
        let calls = Cell::new(0);
        transfer_with_fallback(
            &AtomicBool::new(false),
            &mut (),
            |_| primary,
            |_| Ok(false),
            |_| {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls.get(), 1);
    }
}

#[test]
fn cancellation_does_not_switch_sources() {
    let cancel = AtomicBool::new(false);
    assert!(
        transfer_with_fallback(
            &cancel,
            &mut (),
            |_| {
                cancel.store(true, Ordering::Release);
                Err("cancelled".into())
            },
            |_| panic!("unexpected verification"),
            |_| panic!("unexpected fallback")
        )
        .is_err()
    );
    assert!(
        transfer_with_fallback(
            &cancel,
            &mut (),
            |_| panic!("unexpected primary"),
            |_| Ok(true),
            |_| Ok(())
        )
        .is_err()
    );
}

#[test]
fn failed_fallback_preserves_both_source_errors() {
    let error = transfer_with_fallback(
        &AtomicBool::new(false),
        &mut (),
        |_| Err("blocked".into()),
        |_| Ok(false),
        |_| Err("missing Git LFS".into()),
    )
    .unwrap_err();
    assert!(error.contains("HTTPS: blocked"));
    assert!(error.contains("Git LFS fallback: missing Git LFS"));
}
