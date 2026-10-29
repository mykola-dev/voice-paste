use super::*;
use gpui_kit::TestAppContext;
use voicepaste_core::SessionId;

#[gpui_kit::test]
fn slide_moves_panel_pixels_not_viewport_and_keeps_completed_entry_visible(
    cx: &mut TestAppContext,
) {
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
    for scale in [1., 1.75, 2.] {
        cx.simulate_window_scale_factor_change(overlay.handle, scale);
        cx.update_window(overlay.handle, |_, window, cx| {
            overlay.update(State::Recording(SessionId(1)), false, cx);
            overlay.material(false, cx);
            for millis in [20, 80, 220, 400, 600] {
                overlay.view.update(cx, |view, cx| {
                    view.motion = motion::Motion::default();
                    view.motion.target(true, std::time::Duration::ZERO);
                    view.clock = Instant::now() - std::time::Duration::from_millis(millis);
                    cx.notify();
                });
                window.draw(cx).clear(cx);
                let opacity = overlay.animation(cx);
                let quads = window.painted_quads();
                let panel = quads
                    .iter()
                    .find(|quad| {
                        (quad.bounds.size.width.as_f32() - WIDTH * scale).abs() < 0.1
                            && (quad.bounds.size.height.as_f32() - HEIGHT * scale).abs() < 0.1
                    })
                    .expect("the recording panel must remain painted");
                assert!(
                    (panel.bounds.origin.x.as_f32() - TRAVEL * (1. - opacity) * scale).abs() < 1.
                );
                // GPUI paints the body and border strips as separate quads.
                assert!(
                    quads
                        .iter()
                        .any(|quad| (quad.border_color.a - opacity).abs() < 0.01)
                );
                assert!(
                    (panel.content_mask.bounds.size.width.as_f32() - VIEWPORT_WIDTH * scale).abs()
                        < 1.
                );
                assert_eq!(window.viewport_size(), size(px(VIEWPORT_WIDTH), px(HEIGHT)));
                assert!(overlay.active(cx));
            }
        })
        .unwrap();
    }
    cx.update(|cx| overlay.close(cx));
}

#[gpui_kit::test]
fn exit_retains_foreground_until_complete_and_reentry_does_not_remount(cx: &mut TestAppContext) {
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
        overlay.update(State::Recording(SessionId(1)), false, cx);
        overlay.material(false, cx);
        overlay.view.update(cx, |view, _| {
            view.clock = Instant::now() - std::time::Duration::from_secs(1);
        });
        assert_eq!(
            overlay.animation(cx),
            0.,
            "host reads cannot advance a frame"
        );
        window.draw(cx).clear(cx);
        assert_eq!(overlay.animation(cx), 1.);
        overlay.update(State::Idle, false, cx);
        assert!(overlay.active(cx));
        assert_eq!(
            overlay.view.read(cx).presentation.state,
            State::Recording(SessionId(1))
        );
        window.draw(cx).clear(cx);
        assert!(window.simulate_next_frame(cx) > 0);
        overlay.update(State::Recording(SessionId(2)), false, cx);
        assert!(
            !overlay.show_pending(cx),
            "reversal must not hide/prepare/show"
        );
        assert!(overlay.animation(cx) > 0.9);
        overlay.update(State::Idle, false, cx);
        overlay.view.update(cx, |view, _| {
            view.clock = Instant::now() - std::time::Duration::from_secs(2);
        });
        window.draw(cx).clear(cx);
        assert_eq!(overlay.animation(cx), 0.);
        assert!(!overlay.active(cx));
        window.draw(cx).clear(cx);
        assert_eq!(window.simulate_next_frame(cx), 0);
    })
    .unwrap();
    cx.update(|cx| overlay.close(cx));
}

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
        assert_eq!(window.viewport_size(), size(px(VIEWPORT_WIDTH), px(HEIGHT)));
        for state in [State::Transcribing(SessionId(1)), State::Idle] {
            overlay.update(state, true, cx);
            window.draw(cx).clear(cx);
            assert!(window.simulate_next_frame(cx) > 0);
        }
        overlay.update(State::Idle, false, cx);
        overlay.view.update(cx, |view, _| {
            view.clock -= std::time::Duration::from_secs(1);
        });
        window.draw(cx).clear(cx);
        // A previously queued, rate-gated callback may still arrive. It must
        // stop without notifying/rearming once the final exit frame is drawn.
        window.simulate_next_frame(cx);
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
            overlay.view.update(cx, |view, _| {
                view.clock -= std::time::Duration::from_secs(1);
            });
            window.draw(cx).clear(cx);
            let appearance = OverlayAppearance {
                acrylic: true,
                transparency,
                ..OverlayAppearance::default()
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
