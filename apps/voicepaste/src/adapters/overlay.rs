//! Shared GPUI foreground. Native window policy/material stay in OS adapters.
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
use state::{CORNER_RADIUS, HEIGHT, Presentation, WIDTH, busy_size, triangle};
use std::time::Instant;
use voicepaste_core::{
    State,
    settings::OverlayAppearance,
    waveform::{CAPACITY, Measurement},
};

pub struct OverlayWindow {
    handle: AnyWindowHandle,
    view: Entity<OverlayView>,
}

impl OverlayWindow {
    pub fn open(
        color: &str,
        appearance: OverlayAppearance,
        levels: Receiver<Measurement>,
        clock: Instant,
        cx: &mut App,
    ) -> Result<Self, String> {
        let view = cx.new(|_| OverlayView {
            presentation: Presentation::new(color, appearance, clock.elapsed()),
            levels,
            clock,
            material: false,
            show_pending: false,
        });
        let root = view.clone();
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(WIDTH), px(HEIGHT)),
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
        Ok(Self { handle, view })
    }

    pub fn handle(&self) -> AnyWindowHandle {
        self.handle
    }

    pub fn update(&self, state: State, downloading: bool, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            let was_active = view.presentation.active;
            view.presentation
                .update(state, downloading, view.clock.elapsed());
            if !view.presentation.active {
                view.show_pending = false;
            } else if !was_active {
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

    pub fn active(&self, cx: &App) -> bool {
        self.view.read(cx).presentation.active
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
}

impl Render for OverlayView {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let now = self.clock.elapsed();
        for value in self.levels.try_iter().take(CAPACITY) {
            self.presentation.measurement(value, now);
        }
        self.presentation.wave.advance(now);
        let elapsed = self.presentation.elapsed(now);
        let recording = matches!(self.presentation.state, State::Recording(_));
        let active = self.presentation.active;
        if active {
            window.request_animation_frame();
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
        div().size_full().relative().when(active, |root| {
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
                                | (255. * (1. - 0.7 * triangle(elapsed.as_secs_f64(), 0.8))).round()
                                    as u32,
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
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x111111))
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
            .when(!recording, |root| {
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
        })
    }
}

impl OverlayView {
    fn body_alpha(&self) -> u8 {
        if self.material {
            0
        } else {
            self.presentation.appearance.background_alpha()
        }
    }
}
