//! GPUI owns the complete Settings layout, input state, scroll viewport and frame.
//! This adapter submits drafts; it never saves files or changes the worker itself.
use gpui_kit::component::{
    ActiveTheme, Colorize, Disableable, IndexPath, Selectable, TitleBar,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    scroll::{Scrollbar, ScrollbarMode},
    select::{SearchableVec, Select, SelectState},
    slider::{Slider, SliderState},
};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, Bounds, Context, Entity, FontWeight, Pixels, ScrollHandle,
    Subscription, TestSupportExt, Window, WindowBounds, WindowOptions, div, point, prelude::*, px,
    size,
};
use std::{cell::Cell, rc::Rc};
use voicepaste_core::settings::{
    Device, HOTKEY_CHOICES, Hotkey, Language, PasteShortcut, Settings,
};

type Choice = Entity<SelectState<SearchableVec<String>>>;

pub struct SettingsWindow {
    handle: AnyWindowHandle,
    view: Entity<SettingsView>,
    closed: Rc<Cell<bool>>,
    pub id: u64,
}

impl SettingsWindow {
    pub fn open(base: Settings, id: u64, log_path: &str, cx: &mut App) -> Result<Self, String> {
        let closed = Rc::new(Cell::new(false));
        let close = closed.clone();
        let bounds = initial_bounds(&base, cx);
        let models = voicepaste_core::models::catalog()?
            .into_iter()
            .map(|m| voicepaste_core::models::display_name(&m.name))
            .collect();
        let (handle, view) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui_kit::TitlebarOptions {
                    title: Some("VoicePaste Settings".into()),
                    ..TitleBar::title_bar_options()
                }),
                window_min_size: Some(size(px(480.), px(420.))),
                // Present placement without the backend's synthetic-Alt activation.
                // platform shows/foregrounds this HWND after releasing the App borrow.
                focus: false,
                ..TitleBar::window_options()
            },
            cx,
            |window, cx| {
                window.on_window_should_close(cx, move |_, _| {
                    close.set(true);
                    true
                });
                cx.new(|cx| {
                    SettingsView::new(
                        base,
                        models,
                        log_path.to_owned(),
                        closed.clone(),
                        window,
                        cx,
                    )
                })
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            handle,
            view,
            closed,
            id,
        })
    }
    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }
    pub fn native_handle(&self, cx: &mut App) -> Result<isize, String> {
        self.handle
            .update(cx, |_, window, _| {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                match HasWindowHandle::window_handle(window)
                    .map_err(|e| e.to_string())?
                    .as_raw()
                {
                    RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get()),
                    _ => Err("Settings does not have a Windows window handle".into()),
                }
            })
            .map_err(|e| e.to_string())?
    }
    pub fn take_save(&self, cx: &mut App) -> Option<Settings> {
        self.view.update(cx, |view, _| view.request.take())
    }
    pub fn saved(&self, result: &Result<Settings, String>, cx: &mut App) {
        if self.is_closed() {
            return;
        }
        let close_after_save = self.view.update(cx, |view, cx| {
            view.busy = false;
            view.error = result.as_ref().err().cloned().unwrap_or_default();
            cx.notify();
            view.close_after_save
        });
        if result.is_ok() && close_after_save {
            self.close(cx);
        }
    }
    pub fn close(&self, cx: &mut App) {
        if !self.closed.replace(true) {
            let _ = self
                .handle
                .update(cx, |_, window, _| window.remove_window());
        }
    }
    pub fn model_query(&self, cx: &App) -> Option<String> {
        self.view.read(cx).model_query.clone()
    }
    pub fn model_queried(&self, cx: &mut App) {
        self.view.update(cx, |view, _| view.model_query = None);
    }
    pub fn availability(&self, model: &str, ready: bool, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            if view.status_model == model {
                view.status = if ready {
                    "The model is ready"
                } else {
                    "The model needs to be downloaded"
                }
                .into();
                cx.notify();
            }
        });
    }
    pub fn memory(&self, message: &str, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            if view.memory != message {
                view.memory = message.into();
                cx.notify();
            }
        });
    }
}

fn initial_bounds(base: &Settings, cx: &App) -> Bounds<Pixels> {
    let desired = Bounds::new(
        point(
            px(base.settings_window_left as f32),
            px(base.settings_window_top as f32),
        ),
        size(
            px(base.settings_window_width as f32),
            px(base.settings_window_height as f32),
        ),
    );
    let displays = cx.displays();
    let display = displays
        .iter()
        .find(|d| d.visible_bounds().intersects(&desired))
        .cloned()
        .or_else(|| cx.primary_display());
    let Some(display) = display else {
        return desired;
    };
    let work = display.visible_bounds();
    let width = desired.size.width.min(work.size.width);
    let height = desired.size.height.min(work.size.height);
    Bounds::new(
        point(
            desired
                .origin
                .x
                .max(work.origin.x)
                .min(work.right() - width),
            desired
                .origin
                .y
                .max(work.origin.y)
                .min(work.bottom() - height),
        ),
        size(width, height),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsPage {
    Keys,
    Transcription,
    Appearance,
    System,
}

impl SettingsPage {
    const ALL: [Self; 4] = [
        Self::Keys,
        Self::Transcription,
        Self::Appearance,
        Self::System,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Keys => "Hotkeys",
            Self::Transcription => "Transcription",
            Self::Appearance => "Appearance",
            Self::System => "System",
        }
    }
}

struct SettingsView {
    draft: Settings,
    hotkey: Choice,
    paste: Choice,
    model: Choice,
    device: Choice,
    language: Choice,
    beam: Entity<SliderState>,
    transparency: Entity<SliderState>,
    prompt: Entity<TextareaState>,
    color: Entity<InputState>,
    picker: Entity<ColorPickerState>,
    page: SettingsPage,
    page_scrolls: [ScrollHandle; 4],
    prompt_height: Pixels,
    status: String,
    status_model: String,
    model_query: Option<String>,
    memory: String,
    error: String,
    log_path: String,
    request: Option<Settings>,
    busy: bool,
    close_after_save: bool,
    closed: Rc<Cell<bool>>,
    _subscriptions: Vec<Subscription>,
}

fn choice(values: Vec<String>, selected: usize, window: &mut Window, cx: &mut App) -> Choice {
    cx.new(|cx| {
        SelectState::new(
            SearchableVec::new(values),
            Some(IndexPath::new(selected)),
            window,
            cx,
        )
        .searchable(false)
    })
}
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).into()).collect()
}

impl SettingsView {
    fn new(
        draft: Settings,
        models: Vec<String>,
        log_path: String,
        closed: Rc<Cell<bool>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut hotkeys: Vec<String> = HOTKEY_CHOICES
            .iter()
            .map(|(label, _)| (*label).into())
            .collect();
        let selected = HOTKEY_CHOICES
            .iter()
            .position(|(_, value)| Hotkey::parse(value).ok() == Hotkey::parse(&draft.hotkey).ok())
            .unwrap_or_else(|| {
                // Preserve previously accepted custom config values until the user
                // explicitly selects one of the six standard choices.
                hotkeys.push(draft.hotkey.clone());
                hotkeys.len() - 1
            });
        let hotkey = choice(hotkeys, selected, window, cx);
        let paste = choice(
            strings(&["Ctrl+V", "Ctrl+Shift+V", "Shift+Insert"]),
            match draft.paste_shortcut {
                PasteShortcut::CtrlV => 0,
                PasteShortcut::CtrlShiftV => 1,
                PasteShortcut::ShiftInsert => 2,
            },
            window,
            cx,
        );
        let model_index = models
            .iter()
            .position(|m| *m == voicepaste_core::models::display_name(&draft.model))
            .unwrap_or(0);
        let model = choice(models, model_index, window, cx);
        let device = choice(
            strings(&["CUDA (auto CPU fallback)", "CUDA only", "CPU"]),
            match draft.device {
                Device::CudaAuto => 0,
                Device::CudaOnly => 1,
                Device::Cpu => 2,
            },
            window,
            cx,
        );
        let language = choice(
            strings(&["Auto", "English", "Ukrainian", "English + Ukrainian"]),
            match draft.language_mode {
                Language::Auto => 0,
                Language::En => 1,
                Language::Ua => 2,
                Language::Bilingual => 3,
            },
            window,
            cx,
        );
        let beam = cx.new(|_| {
            SliderState::new()
                .min(1.)
                .max(10.)
                .step(1.)
                .default_value(draft.beam_size as f32)
        });
        let transparency = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(draft.overlay_appearance().transparency as f32)
        });
        let prompt = cx.new(|cx| {
            TextareaState::new(window, cx).default_value(draft.custom_initial_prompt.clone())
        });
        let color =
            cx.new(|cx| InputState::new(window, cx).default_value(draft.indicator_color.clone()));
        // The incoming settings were validated by storage. The text field remains
        // authoritative, so an invalid unfinished hex edit is never silently saved.
        let initial_color = gpui_kit::Hsla::parse_hex(&draft.indicator_color).unwrap_or_default();
        let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(initial_color));
        let subscription =
            cx.subscribe_in(&picker, window, |this: &mut Self, _, event, window, cx| {
                if let ColorPickerEvent::Change(Some(value)) = event {
                    let mut value = *value;
                    value.a = 1.; // Overlay contract is opaque #RRGGBB, not RGBA.
                    this.color
                        .update(cx, |input, cx| input.set_value(value.to_hex(), window, cx));
                    cx.notify();
                }
            });
        let mut subscriptions = vec![subscription];
        subscriptions.push(cx.subscribe_in(
            &color,
            window,
            |this: &mut Self, input, event, window, cx| {
                if let InputEvent::Change = event {
                    let value = input.read(cx).value();
                    if value.len() == 7
                        && let Ok(value) = gpui_kit::Hsla::parse_hex(&value)
                    {
                        this.picker
                            .update(cx, |picker, cx| picker.set_value(value, window, cx));
                    }
                }
            },
        ));
        for control in [&hotkey, &paste, &device, &language] {
            subscriptions.push(cx.observe(control, |_, _, cx| cx.notify()));
        }
        subscriptions.push(cx.observe(&model, |this, model, cx| {
            if let Some(selected) = model.read(cx).selected_value() {
                let id = voicepaste_core::models::id_from_display(selected);
                if this.status_model != id {
                    this.status_model = id.clone();
                    this.model_query = Some(id);
                    this.status = "Checking model…".into();
                }
            }
            cx.notify();
        }));
        subscriptions.push(cx.observe(&beam, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&transparency, |_, _, cx| cx.notify()));
        let prompt_height = px(draft.initial_prompt_height.unwrap_or(140.) as f32);
        let status_model = draft.model.clone();
        Self {
            draft,
            hotkey,
            paste,
            model,
            device,
            language,
            beam,
            transparency,
            prompt,
            color,
            picker,
            page: SettingsPage::Keys,
            page_scrolls: std::array::from_fn(|_| ScrollHandle::new()),
            prompt_height,
            status: "Checking model…".into(),
            model_query: Some(status_model.clone()),
            status_model,
            memory: "RAM: measuring host and worker…".into(),
            error: String::new(),
            log_path,
            request: None,
            busy: false,
            close_after_save: false,
            closed,
            _subscriptions: subscriptions,
        }
    }
    fn save(&mut self, close_after_save: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let result = self.read(window, cx);
        match result {
            Ok(draft) => {
                self.request = Some(draft);
                self.busy = true;
                self.close_after_save = close_after_save;
                self.error.clear();
            }
            Err(error) => self.error = error,
        }
        cx.notify();
    }
    fn scroll(&self) -> &ScrollHandle {
        &self.page_scrolls[self.page as usize]
    }
    fn read(&self, window: &Window, cx: &App) -> Result<Settings, String> {
        let mut draft = self.draft.clone();
        if draft.initial_prompt_height.is_some() || self.prompt_height != px(140.) {
            draft.initial_prompt_height = Some(self.prompt_height.as_f32() as f64);
        }
        let index = |choice: &Choice| {
            choice
                .read(cx)
                .selected_index(cx)
                .map(|i| i.row)
                .ok_or("Select a value")
        };
        draft.hotkey = HOTKEY_CHOICES
            .get(index(&self.hotkey)?)
            .map(|(_, value)| (*value).to_owned())
            .unwrap_or_else(|| self.draft.hotkey.clone());
        draft.paste_shortcut = match index(&self.paste)? {
            0 => PasteShortcut::CtrlV,
            1 => PasteShortcut::CtrlShiftV,
            _ => PasteShortcut::ShiftInsert,
        };
        draft.model = voicepaste_core::models::id_from_display(
            self.model
                .read(cx)
                .selected_value()
                .ok_or("Select a model")?,
        );
        draft.device = match index(&self.device)? {
            0 => Device::CudaAuto,
            1 => Device::CudaOnly,
            _ => Device::Cpu,
        };
        draft.language_mode = match index(&self.language)? {
            0 => Language::Auto,
            1 => Language::En,
            2 => Language::Ua,
            _ => Language::Bilingual,
        };
        draft.beam_size = self.beam.read(cx).value().start() as u8;
        draft.custom_initial_prompt = self.prompt.read(cx).value().to_string();
        draft.indicator_color = self.color.read(cx).value().to_string();
        let transparency = self.transparency.read(cx).value().start().round() as u8;
        if draft.overlay_transparency.is_some()
            || transparency != draft.overlay_appearance().transparency
        {
            draft.overlay_transparency = Some(transparency);
        }
        let bounds = window.window_bounds().get_bounds();
        draft.settings_window_width = bounds.size.width.as_f32().clamp(480., 1400.) as f64;
        draft.settings_window_height = bounds.size.height.as_f32().clamp(420., 1200.) as f64;
        draft.settings_window_left = bounds.origin.x.as_f32() as f64;
        draft.settings_window_top = bounds.origin.y.as_f32() as f64;
        draft.validate()?;
        Ok(draft)
    }
    fn cancel(&self, window: &mut Window) {
        self.closed.set(true);
        window.remove_window();
    }
}

fn hint(value: impl Into<gpui_kit::SharedString>, cx: &App) -> impl IntoElement {
    div()
        .text_size(px(12.))
        .text_color(cx.theme().muted_foreground)
        .child(value.into())
}
fn settings_group(children: Vec<AnyElement>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .py(px(16.))
        .children(children)
}
fn row(label: &'static str, control: impl IntoElement, narrow: bool) -> impl IntoElement {
    div()
        .flex()
        .gap(px(12.))
        .w_full()
        .when(narrow, |d| d.flex_col())
        .when(!narrow, |d| d.items_start())
        .child(
            div()
                .when(!narrow, |d| d.w(px(168.)).flex_shrink_0().pt(px(6.)))
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .when(narrow, |d| d.w_full())
                .child(control),
        )
}

#[derive(Clone)]
struct PromptResize {
    origin: Rc<Cell<Pixels>>,
    height: Pixels,
}
impl Render for PromptResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar_width = if window.viewport_size().width < px(700.) {
            px(144.)
        } else {
            px(180.)
        };
        let narrow = window.viewport_size().width - sidebar_width < px(600.);
        let device = self
            .device
            .read(cx)
            .selected_index(cx)
            .map(|i| i.row)
            .unwrap_or(0);
        let language = self
            .language
            .read(cx)
            .selected_index(cx)
            .map(|i| i.row)
            .unwrap_or(0);
        let beam = self.beam.read(cx).value().start() as u8;
        let sections = match self.page {
            SettingsPage::Keys => vec![
                settings_group(
                    vec![
                        row(
                            "Toggle recording",
                            Select::new(&self.hotkey).w_full().disabled(self.busy),
                            narrow,
                        )
                        .into_any_element(),
                        row(
                            "Paste shortcut",
                            Select::new(&self.paste).w_full().disabled(self.busy),
                            narrow,
                        )
                        .into_any_element(),
                    ],
                )
                .into_any_element(),
            ],
            SettingsPage::Transcription => {
                vec![
                    settings_group(
                        vec![
                    row(
                        "Model",
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .id("model-dropdown")
                                    .test_support()
                                    .child(Select::new(&self.model).w_full().disabled(self.busy)),
                            )
                            .when(!self.status.is_empty(), |d| {
                                d.child(
                                    div()
                                        .id("model-status")
                                        .test_support()
                                        .child(hint(self.status.clone(), cx)),
                                )
                            }),
                        narrow,
                    )
                    .into_any_element(),
                    row(
                        "Device",
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(Select::new(&self.device).w_full().disabled(self.busy))
                            .child(hint(
                                match device {
                                    0 => "Prefer CUDA FP16; report CPU INT8 fallback.",
                                    1 => "Requires NVIDIA CUDA; no CPU fallback.",
                                    _ => "CPU INT8; GPU not used.",
                                },
                                cx,
                            )),
                        narrow,
                    )
                    .into_any_element(),
                    row(
                        "Language mode",
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(Select::new(&self.language).w_full().disabled(self.busy))
                            .child(hint(
                                match language {
                                    0 => "Detect the spoken language automatically.",
                                    1 => "Force English transcription.",
                                    2 => "Force Ukrainian spelling and token suppression.",
                                    _ => "Detect English/Ukrainian; retry Russian as Ukrainian.",
                                },
                                cx,
                            )),
                        narrow,
                    )
                    .into_any_element(),
                    row(
                        "Beam size",
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        Slider::new(&self.beam)
                                            .accessibility_label("Beam size")
                                            .disabled(self.busy)
                                            .flex_1(),
                                    )
                                    .child(beam.to_string()),
                            )
                            .child(hint("1: fastest; higher: more search", cx)),
                        narrow,
                    )
                    .into_any_element(),
                    Checkbox::new("vad")
                        .label("Enable VAD (silence trim)")
                        .checked(self.draft.enable_vad)
                        .disabled(self.busy)
                        .on_change(cx.listener(|this, value, _, cx| {
                            this.draft.enable_vad = *value;
                            cx.notify();
                        }))
                        .into_any_element(),
                ],
                    )
                    .into_any_element(),
                    settings_group(
                        vec![
                            div().child("Custom initial prompt").into_any_element(),
                            hint(
                                "Appended to the initial prompt to guide vocabulary or formatting.",
                                cx,
                            )
                            .into_any_element(),
                            div()
                                .flex()
                                .flex_col()
                                .w_full()
                                .child(
                                    Textarea::new(&self.prompt)
                                        .aria_label("Custom initial prompt")
                                        .disabled(self.busy)
                                        .h(self.prompt_height)
                                        .w_full(),
                                )
                                .child(
                                    div()
                                        .id("prompt-resize")
                                        .test_support()
                                        .h(px(12.))
                                        .w_full()
                                        .cursor_row_resize()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(div().h(px(2.)).w(px(32.)).bg(cx.theme().border))
                                        .on_drag(
                                            PromptResize {
                                                origin: Rc::new(Cell::new(px(0.))),
                                                height: self.prompt_height,
                                            },
                                            |drag, _, window, cx| {
                                                // GPUI's drag value is the original payload;
                                                // the constructor's point is element-relative.
                                                // Share the actual window-coordinate anchor,
                                                // not a modified value of the drag preview.
                                                drag.origin.set(window.mouse_position().y);
                                                cx.new(|_| drag.clone())
                                            },
                                        )
                                        .on_drag_move::<PromptResize>(cx.listener(
                                            |this,
                                             event: &gpui_kit::DragMoveEvent<PromptResize>,
                                             _,
                                             cx| {
                                                let drag = event.drag(cx);
                                                this.prompt_height = (drag.height
                                                    + event.event.position.y
                                                    - drag.origin.get())
                                                .clamp(px(80.), px(600.));
                                                cx.notify();
                                            },
                                        )),
                                )
                                .into_any_element(),
                        ],
                    )
                    .into_any_element(),
                ]
            }
            SettingsPage::Appearance => vec![
                settings_group(
                    vec![
                        div().id("overlay-acrylic").test_support().child(
                            Checkbox::new("acrylic")
                                .label("Acrylic")
                                .checked(self.draft.overlay_appearance().acrylic)
                                .disabled(self.busy)
                                .on_change(cx.listener(|this, value, _, cx| {
                                    this.draft.overlay_acrylic = Some(*value);
                                    cx.notify();
                                })),
                        ).into_any_element(),
                        hint("Frosted background where supported; otherwise uses translucency.", cx).into_any_element(),
                        row(
                            "Background transparency",
                            div().id("overlay-transparency").test_support().flex().flex_col().gap_2()
                                .child(div().flex().items_center().gap_3()
                                    .child(Slider::new(&self.transparency)
                                        .accessibility_label("Background transparency")
                                        .disabled(self.busy).flex_1())
                                    .child(format!("{}%", self.transparency.read(cx).value().start().round() as u8)))
                                .child(hint("0%: opaque tint · 100%: no background. Acrylic keeps blur between the endpoints. Applies to both modes; timer and waveform stay sharp.", cx)),
                            narrow,
                        ).into_any_element(),
                        row(
                            "Indicator color",
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    Input::new(&self.color)
                                        .aria_label("Indicator color hex")
                                        .disabled(self.busy)
                                        .w(px(112.)),
                                )
                                .child(ColorPicker::new(&self.picker).label("Choose…"))
                                .child(
                                    Button::new("reset-color")
                                        .label("Reset")
                                        .disabled(self.busy)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            match Settings::initial() {
                                                Ok(initial) => {
                                                    this.color.update(cx, |input, cx| {
                                                        input.set_value(
                                                            initial.indicator_color.clone(),
                                                            window,
                                                            cx,
                                                        )
                                                    });
                                                    if let Ok(color) = gpui_kit::Hsla::parse_hex(
                                                        &initial.indicator_color,
                                                    ) {
                                                        this.picker.update(cx, |picker, cx| {
                                                            picker.set_value(color, window, cx)
                                                        });
                                                    }
                                                }
                                                Err(error) => this.error = error,
                                            }
                                            cx.notify();
                                        })),
                                ),
                            narrow,
                        )
                        .into_any_element(),
                    ],
                )
                .into_any_element(),
            ],
            SettingsPage::System => vec![
                settings_group(
                    vec![
                        Checkbox::new("start-at-login")
                            .label("Start VoicePaste when I sign in")
                            .checked(self.draft.start_at_login)
                            .disabled(self.busy)
                            .on_change(cx.listener(|this, value, _, cx| {
                                this.draft.start_at_login = *value;
                                cx.notify();
                            }))
                            .into_any_element(),
                        hint("Current Windows user only. Applied with OK or Apply.", cx)
                            .into_any_element(),
                    ],
                )
                .into_any_element(),
                settings_group(
                    vec![
                        Checkbox::new("resident")
                            .label("Keep in memory")
                            .checked(self.draft.keep_in_memory)
                            .disabled(self.busy)
                            .on_change(cx.listener(|this, value, _, cx| {
                                this.draft.keep_in_memory = *value;
                                cx.notify();
                            }))
                            .into_any_element(),
                        div()
                            .id("memory-report")
                            .test_support()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .children(self.memory.lines().map(|line| {
                                let (label, value) = line.split_once(':').unwrap_or((line, ""));
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .justify_between()
                                    .gap(px(8.))
                                    .child(hint(label.to_owned(), cx))
                                    .child(value.trim().to_owned())
                            }))
                            .into_any_element(),
                        hint("Resident RAM is memory currently held in physical RAM. The combined value may count shared pages twice; it is not GPU memory.", cx).into_any_element(),
                    ],
                ).into_any_element(),
                settings_group(
                    vec![
                        Checkbox::new("logging")
                            .label("Enable debug logging")
                            .checked(self.draft.debug_logging)
                            .disabled(self.busy)
                            .on_change(cx.listener(|this, value, _, cx| {
                                this.draft.debug_logging = *value;
                                cx.notify();
                            }))
                            .into_any_element(),
                        hint(self.log_path.clone(), cx).into_any_element(),
                    ],
                )
                .into_any_element(),
            ],
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .font_family(cx.theme().font_family.clone())
            .text_size(px(14.))
            .child(
                TitleBar::new()
                    .bg(cx.theme().title_bar)
                    .child("VoicePaste Settings"),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("settings-navigation")
                            .test_support()
                            .w(sidebar_width)
                            .h_full()
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .px(px(8.))
                            .py(px(20.))
                            .bg(cx.theme().title_bar)
                            .border_r_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .px(px(12.))
                                    .pb(px(12.))
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Settings"),
                            )
                            .children(SettingsPage::ALL.into_iter().map(|page| {
                                Button::new(("settings-page", page as usize))
                                    .ghost()
                                    .label(page.label())
                                    .accessibility_label(page.label())
                                    .selected(self.page == page)
                                    .w_full()
                                    .justify_start()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.page = page;
                                        cx.notify();
                                    }))
                            })),
                    )
                    .child(
                        div()
                            .id("settings-detail")
                            .test_support()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_hidden()
                            .child(
                                div()
                                    .id("settings-scroll")
                                    .test_support()
                                    .size_full()
                                    .overflow_y_scroll()
                                    .track_scroll(self.scroll())
                                    .child(
                                        div()
                                            .px(px(24.))
                                            .pt(px(20.))
                                            .pb(px(20.))
                                            .w_full()
                                            .max_w(px(768.))
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_size(px(20.))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .pb(px(8.))
                                                    .child(self.page.label()),
                                            )
                                            .children(sections),
                                    ),
                            )
                            .child(Scrollbar::vertical(self.scroll()).mode(ScrollbarMode::Always)),
                    ),
            )
            .child(
                div()
                    .id("settings-footer")
                    .test_support()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .flex_shrink_0()
                    .p(px(12.))
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .when(!self.error.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(cx.theme().danger)
                                .child(self.error.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(12.))
                            .child(
                                Button::new("ok")
                                    .primary()
                                    .min_w(px(80.))
                                    .label("OK")
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.save(true, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("cancel")
                                    .min_w(px(80.))
                                    .label("Cancel")
                                    .on_click(
                                        cx.listener(|this, _, window, _| this.cancel(window)),
                                    ),
                            )
                            .child(
                                Button::new("apply")
                                    .min_w(px(80.))
                                    .label(if self.busy { "Saving…" } else { "Apply" })
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.save(false, window, cx)
                                    })),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests;
