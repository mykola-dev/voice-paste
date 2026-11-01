//! Shared GPUI foreground. Native window policy/material stay in OS adapters.
mod frames;
#[cfg(target_os = "macos")]
pub mod macos;
mod motion;
mod state;
#[cfg(test)]
mod tests;
#[cfg(target_os = "windows")]
pub mod windows;

use crossbeam_channel::Receiver;
use gpui_kit::{
    AnyWindowHandle, App, Bounds, Context, Entity, FontWeight, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, canvas, div, point, prelude::*, px, quad, rgb, size,
};
use state::{
    CORNER_RADIUS, HEIGHT, Presentation, TRAVEL, VIEWPORT_WIDTH, WIDTH, busy_size, triangle,
};
use std::{rc::Rc, task::Poll, time::Instant};
use voicepaste_core::{
    State,
    settings::OverlayAppearance,
    waveform::{CAPACITY, Measurement},
};

pub struct OverlayWindow {
    handle: AnyWindowHandle,
    view: Entity<OverlayView>,
    frames: Rc<frames::Frames>,
}

impl OverlayWindow {
    pub fn open(
        color: &str,
        appearance: OverlayAppearance,
        levels: Receiver<Measurement>,
        clock: Instant,
        cx: &mut App,
    ) -> Result<Self, String> {
        let frames = Rc::new(frames::Frames::default());
        let view = cx.new(|_| OverlayView {
            presentation: Presentation::new(color, appearance, clock.elapsed()),
            levels,
            clock,
            material: false,
            show_pending: false,
            motion: motion::Motion::default(),
            opacity: 0.,
            frames: frames.clone(),
            cadence: frames::Cadence::default(),
            frame_queued: false,
        });
        let root = view.clone();
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(VIEWPORT_WIDTH), px(HEIGHT)),
                    ))),
                    titlebar: None,
                    kind: WindowKind::PopUp,
                    focus: false,
                    show: false,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    // This window is intentionally never active. Do not throttle its animation.
                    inactive_frame_interval: None,
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                |_, _| root,
            )
            .map_err(|e| e.to_string())?
            .into();
        Ok(Self {
            handle,
            view,
            frames,
        })
    }

    pub fn handle(&self) -> AnyWindowHandle {
        self.handle
    }

    pub fn update(&self, state: State, downloading: bool, cx: &mut App) {
        self.update_status(state, downloading, "Preparing model...", cx);
    }

    /// Preparation, not transport activity, owns the provisioning lifetime.
    /// The runtime message already contains measured byte progress when available.
    pub fn update_status(&self, state: State, preparing: bool, message: &str, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            let was_active = view.presentation.active;
            let appearance = view.presentation.appearance;
            view.presentation
                .update_status(state, preparing, message, view.clock.elapsed());
            if !was_active && view.opacity > 0. {
                // Reversal is not a new native show/material preparation.
                view.presentation.appearance = appearance;
            }
            view.motion
                .target(view.presentation.active, view.clock.elapsed());
            if !view.presentation.active {
                view.show_pending = false;
            } else if !was_active && view.opacity == 0. {
                // Preserve a hide/show boundary even if several runtime events
                // are coalesced before the outer native synchronization step.
                view.show_pending = true;
            }
            cx.notify();
        });
    }

    pub fn appearance(&self, color: &str, appearance: OverlayAppearance, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            view.presentation.settings(color, appearance);
            cx.notify();
        });
    }

    /// Snapshot under App; apply native mouse policy only after releasing App.
    pub fn provisioning_interactive(&self, cx: &App) -> bool {
        self.view.read(cx).presentation.interactive()
    }

    /// Single bounded intent; the session retains/retries cancellation on backpressure.
    pub fn take_cancel(&self, cx: &mut App) -> Option<u64> {
        self.view
            .update(cx, |view, _| view.presentation.take_cancel())
    }

    pub fn set_preparation_generation(&self, generation: u64, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            view.presentation.set_preparation_generation(generation);
            cx.notify();
        });
    }

    pub fn active(&self, cx: &App) -> bool {
        let view = self.view.read(cx);
        view.presentation.active || view.opacity > 0.
    }

    /// Read the last rendered sample; the host pump must never advance time.
    pub fn animation(&self, cx: &App) -> f32 {
        self.view.read(cx).opacity
    }

    /// Wake the native lifecycle after GPUI releases its render/App borrows.
    pub fn poll_frame(&self, cx: &mut std::task::Context<'_>) -> Poll<()> {
        self.frames.poll(cx)
    }
    pub fn current_appearance(&self, cx: &App) -> OverlayAppearance {
        self.view.read(cx).presentation.appearance
    }

    pub fn show_pending(&self, cx: &App) -> bool {
        self.view.read(cx).show_pending
    }

    pub fn material(&self, enabled: bool, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            view.material = enabled;
            view.show_pending = false;
            cx.notify();
        });
    }

    pub fn close(self, cx: &mut App) {
        let _ = self
            .handle
            .update(cx, |_, window, _| window.remove_window());
    }
}

struct OverlayView {
    presentation: Presentation,
    levels: Receiver<Measurement>,
    clock: Instant,
    material: bool,
    show_pending: bool,
    motion: motion::Motion,
    opacity: f32,
    frames: Rc<frames::Frames>,
    cadence: frames::Cadence,
    frame_queued: bool,
}

impl Render for OverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = self.clock.elapsed();
        self.opacity = self.motion.sample(now);
        self.frames.publish();
        for value in self.levels.try_iter().take(CAPACITY) {
            self.presentation.measurement(value, now);
        }
        self.presentation.wave.advance(now);
        let elapsed = self.presentation.elapsed(now);
        let recording = matches!(self.presentation.state, State::Recording(_));
        let provisioning = self.presentation.provisioning.clone();
        let cancel_requested = self.presentation.cancel_requested;
        let active = self.presentation.active || self.opacity > 0.;
        if active {
            self.request_frame(window, cx);
        }
        // At most one narrow quad per device pixel, not an RGBA bitmap upload.
        let columns = (WIDTH * window.scale_factor()).ceil().clamp(1., 2048.) as usize;
        let profile = if active && recording {
            self.presentation.wave.profile(columns)
        } else {
            Vec::new()
        };
        let color = self.presentation.color;
        let body_alpha = self.body_alpha();
        let seconds = elapsed.as_secs();
        // Animate the panel, never the native foreground window. The fixed
        // transparent viewport clips its path at the work-area edge, so neither GPUI nor
        // Windows can feed an animated HWND position back into the anchor.
        let panel = div()
            .w(px(WIDTH))
            .h(px(HEIGHT))
            .relative()
            .left(px(TRAVEL * (1. - self.opacity)))
            .opacity(self.opacity)
            .when(active, |root| {
                root.child(
                    div()
                        .absolute()
                        .size_full()
                        .rounded(px(CORNER_RADIUS))
                        .border_1()
                        .border_color(rgb(0x999999))
                        .bg(gpui_kit::rgba(0xffffff00 | u32::from(body_alpha))),
                )
                .when(recording, |root| {
                    root.child(
                        div()
                            .absolute()
                            .left(px(78.))
                            .top(px(22.))
                            .size(px(12.))
                            .rounded_full()
                            .bg(gpui_kit::rgba(
                                0xff000000
                                    | (255. * (1. - 0.7 * triangle(elapsed.as_secs_f64(), 0.8)))
                                        .round() as u32,
                            )),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(98.))
                            .top(px(17.))
                            .h(px(22.))
                            .text_size(px(16.))
                            .line_height(px(22.))
                            .font_family("IBM Plex Sans")
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0x222222))
                            .child(format!("{:02}:{:02}", seconds / 60, seconds % 60)),
                    )
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                let step = WIDTH / columns as f32;
                                for (index, column) in profile.into_iter().enumerate() {
                                    let half = 0.5 + column.level * 14.5;
                                    window.paint_quad(quad(
                                        Bounds::new(
                                            bounds.origin
                                                + point(px(index as f32 * step), px(15. - half)),
                                            size(px(step), px(half * 2.)),
                                        ),
                                        px(0.),
                                        rgb(color).alpha(column.opacity),
                                        px(0.),
                                        gpui_kit::transparent_black(),
                                        gpui_kit::BorderStyle::Solid,
                                    ));
                                }
                            },
                        )
                        .absolute()
                        .left(px(0.))
                        .top(px(49.))
                        .w(px(WIDTH))
                        .h(px(30.)),
                    )
                })
                .when(!recording && provisioning.is_none(), |root| {
                    root.children((0..5).map(|bar| {
                        let height = 40. * busy_size(elapsed.as_secs_f64(), bar);
                        div()
                            .absolute()
                            .left(px(93. + bar as f32 * 8.))
                            .top(px(50. - height / 2.))
                            .w(px(6.))
                            .h(px(height))
                            .bg(rgb(color))
                    }))
                })
                .when(!recording, |root| {
                    root.children(provisioning.map(|message| {
                        let (stage, progress) = message
                            .split_once(" — ")
                            .map_or((message.as_str(), None), |(stage, progress)| {
                                (stage, Some(progress))
                            });
                        div()
                            .absolute()
                            .inset_0()
                            .px(px(14.))
                            .pt(px(8.))
                            .pb(px(30.))
                            .flex()
                            .flex_col()
                            .justify_center()
                            .gap(px(2.))
                            .overflow_hidden()
                            .font_family("IBM Plex Sans")
                            .text_color(rgb(0x222222))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(px(16.))
                                    .max_h(px(32.))
                                    .overflow_hidden()
                                    .child(stage.to_owned()),
                            )
                            .children(progress.map(|progress| {
                                div()
                                    .text_size(px(13.))
                                    .line_height(px(18.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(progress.to_owned())
                            }))
                            .child(
                                div()
                                    .id("cancel-provisioning")
                                    .absolute()
                                    .right(px(14.))
                                    .bottom(px(7.))
                                    .px(px(8.))
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(rgb(0x777777))
                                    .text_size(px(12.))
                                    .line_height(px(20.))
                                    .child(if cancel_requested {
                                        "Cancelling..."
                                    } else {
                                        "Cancel"
                                    })
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.presentation.request_cancel();
                                        cx.notify();
                                    })),
                            )
                    }))
                })
            });
        div().size_full().overflow_hidden().child(panel)
    }
}

impl OverlayView {
    fn request_frame(&mut self, window: &Window, cx: &Context<Self>) {
        if self.frame_queued {
            return;
        }
        self.frame_queued = true;
        let view = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let _ = view.update(cx, |view, cx| {
                view.frame_queued = false;
                if !view.presentation.active && view.opacity == 0. {
                    return;
                }
                if view
                    .cadence
                    .due(view.clock.elapsed(), window.refresh_interval(cx))
                {
                    cx.notify();
                } else {
                    view.request_frame(window, cx);
                }
            });
        });
    }

    fn body_alpha(&self) -> u8 {
        if self.material {
            0
        } else {
            self.presentation.appearance.background_alpha()
        }
    }
}
