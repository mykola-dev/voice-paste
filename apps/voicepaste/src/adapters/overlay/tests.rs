use super::*;
use gpui_kit::TestAppContext;
use voicepaste_core::SessionId;

#[gpui_kit::test]
fn shared_view_draws_and_schedules_frames_only_while_active(cx: &mut TestAppContext) {
    let (sender, levels) = crossbeam_channel::bounded(CAPACITY);
    let overlay = cx.update(|cx| {
        gpui_kit::init(cx);
        super::super::settings_theme::install(cx).unwrap();
        OverlayWindow::open(
            "#E9563D",
            OverlayAppearance::default(),
            levels,
            Instant::now(),
            cx,
        )
        .unwrap()
    });
    cx.update_window(overlay.handle, |_, window, cx| {
        window.draw(cx).clear(cx);
        assert_eq!(window.simulate_next_frame(cx), 0);
        overlay.update(State::Recording(SessionId(1)), false, cx);
        sender
            .try_send(Measurement {
                session: SessionId(1),
                timestamp: std::time::Duration::ZERO,
                level: 0.2,
            })
            .unwrap();
        window.draw(cx).clear(cx);
        assert!(
            overlay
                .view
                .read(cx)
                .presentation
                .wave
                .profile(224)
                .last()
                .unwrap()
                .level
                > 0.
        );
        assert!(window.simulate_next_frame(cx) > 0);
        assert_eq!(window.viewport_size(), size(px(WIDTH), px(HEIGHT)));
        for state in [State::Transcribing(SessionId(1)), State::Idle] {
            overlay.update(state, true, cx);
            window.draw(cx).clear(cx);
            assert!(window.simulate_next_frame(cx) > 0);
        }
        overlay.update(State::Idle, false, cx);
        window.draw(cx).clear(cx);
        assert_eq!(window.simulate_next_frame(cx), 0);
        assert!(!overlay.active(cx));
    })
    .unwrap();
    cx.update(|cx| overlay.close(cx));
}

#[gpui_kit::test]
fn material_tint_and_translucent_fallback_share_alpha_without_extra_wash(cx: &mut TestAppContext) {
    let (_sender, levels) = crossbeam_channel::bounded(CAPACITY);
    let overlay = cx.update(|cx| {
        gpui_kit::init(cx);
        super::super::settings_theme::install(cx).unwrap();
        OverlayWindow::open(
            "#E9563D",
            OverlayAppearance::default(),
            levels,
            Instant::now(),
            cx,
        )
        .unwrap()
    });
    cx.update_window(overlay.handle, |_, window, cx| {
        for transparency in [0, 57, 100] {
            overlay.update(State::Idle, false, cx);
            let appearance = OverlayAppearance {
                acrylic: true,
                transparency,
            };
            overlay.appearance("#00ff00", appearance, cx);
            overlay.update(State::Idle, true, cx);
            assert!(overlay.show_pending(cx));
            overlay.material(true, cx);
            assert!(!overlay.show_pending(cx));
            assert_eq!(overlay.view.read(cx).body_alpha(), 0);
            overlay.material(false, cx);
            assert_eq!(
                overlay.view.read(cx).body_alpha(),
                appearance.background_alpha()
            );
            assert_eq!(overlay.view.read(cx).presentation.color, 0x00ff00);
            window.draw(cx).clear(cx);
            window.simulate_next_frame(cx);
        }
    })
    .unwrap();
    cx.update(|cx| overlay.close(cx));
}
