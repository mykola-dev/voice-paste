//! First native launch gate, not a dictation adapter or a successful STT stub.
//! No configuration, model, microphone, hotkey or clipboard effects are connected yet.
use gpui_kit::{Bounds, Context, Window, WindowBounds, WindowOptions, div, prelude::*, px, size};
use std::{cell::RefCell, rc::Rc};

struct LaunchPreview;

impl Render for LaunchPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .p_8()
            .gap_4()
            .bg(gpui_kit::rgb(0x282c33))
            .text_color(gpui_kit::rgb(0xdce0e5))
            .font_family("IBM Plex Sans")
            .child(div().text_xl().child("VoicePaste — macOS launch preview"))
            .child("Apple Silicon · GPUI / Metal · One Dark")
            .child("Launch test only. Recording, Settings, menu bar, transcription and paste are not connected yet.")
            .child("No models are downloaded, no permissions are requested and existing data is not changed.")
            .child("Ad-hoc signed; not notarized. If company policy blocks this app, report the message without changing security settings.")
            .child("Please report whether this window opens, text is sharp, resizing works, and closing the window exits the app.")
            .child(format!(
                "Build: {}",
                option_env!("VOICEPASTE_BUILD_SHA").unwrap_or("local / unknown")
            ))
    }
}

pub fn run() -> Result<(), String> {
    if std::env::args_os().len() != 1 {
        return Err(
            "This launch-only preview accepts no CLI modes; dictation is not implemented yet"
                .into(),
        );
    }
    let outcome = Rc::new(RefCell::new(Ok(())));
    let result = outcome.clone();
    // Match the shared session's lifetime rule: the dispatcher outlives globals.
    let platform = gpui_kit::platform::current_platform(false);
    gpui_kit::Application::with_platform(platform.clone())
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let opened = crate::settings_theme::install(cx).and_then(|()| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                            None,
                            size(px(680.), px(440.)),
                            cx,
                        ))),
                        window_min_size: Some(size(px(560.), px(400.))),
                        titlebar: Some(gpui_kit::TitlebarOptions {
                            title: Some("VoicePaste Launch Preview".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    cx,
                    |window, cx| {
                        window.on_window_should_close(cx, |_, cx| {
                            cx.quit();
                            true
                        });
                        cx.new(|_| LaunchPreview)
                    },
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
            });
            if let Err(error) = opened {
                *result.borrow_mut() = Err(error);
                cx.quit();
            }
        });
    drop(platform);
    outcome.borrow().clone()
}
