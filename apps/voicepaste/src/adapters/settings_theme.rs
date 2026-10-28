//! Local Zed One Dark assets projected onto the pinned component vocabulary.
use gpui_kit::component::{Colorize, Theme, ThemeMode, scroll::ScrollbarMode};
use gpui_kit::{App, Hsla, px};

pub fn install(cx: &mut App) -> Result<(), String> {
    let family: serde_json::Value = serde_json::from_str(include_str!("../../assets/one/one.json"))
        .map_err(|e| e.to_string())?;
    let style = &family["themes"][0]["style"];
    let color = |key: &str| -> Result<Hsla, String> {
        Hsla::parse_hex(
            style[key]
                .as_str()
                .ok_or_else(|| format!("Missing One Dark token: {key}"))?,
        )
        .map_err(|e| e.to_string())
    };
    let surface = color("surface.background")?;
    let editor = color("editor.background")?;
    let text = color("text")?;
    let muted = color("text.muted")?;
    let element = color("element.background")?;
    let hover = color("element.hover")?;
    let active = color("element.active")?;
    let accent = color("text.accent")?;
    let border = color("border")?;
    let focus = color("border.focused")?;
    let error = color("error")?;
    let title = color("title_bar.background")?;
    let selection = color("search.match_background")?;
    let thumb = color("scrollbar.thumb.background")?;
    let thumb_hover = color("scrollbar.thumb.hover_background")?;
    cx.text_system()
        .add_fonts(vec![
            std::borrow::Cow::Borrowed(include_bytes!(
                "../../assets/fonts/IBMPlexSans-Regular.ttf"
            )),
            std::borrow::Cow::Borrowed(include_bytes!(
                "../../assets/fonts/IBMPlexSans-SemiBold.ttf"
            )),
        ])
        .map_err(|e| e.to_string())?;
    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |t| {
        t.font_family = "IBM Plex Sans".into();
        t.font_size = px(14.);
        t.radius = px(4.);
        t.radius_lg = px(6.);
        t.focus_ring = false;
        t.scrollbar_mode = ScrollbarMode::Always;
        t.background = editor;
        t.foreground = text;
        t.border = border;
        t.input = border;
        t.ring = focus;
        t.caret = accent;
        t.accent = hover;
        t.accent_foreground = text;
        t.primary = accent;
        t.primary_hover = accent;
        t.primary_active = focus;
        t.primary_foreground = editor;
        t.secondary = element;
        t.secondary_foreground = text;
        t.button = element;
        t.button_hover = hover;
        t.button_active = active;
        t.button_foreground = text;
        t.button_primary = accent;
        t.button_primary_hover = accent;
        t.button_primary_active = focus;
        t.button_primary_foreground = editor;
        t.popover = surface;
        t.popover_foreground = text;
        t.colors.list = surface;
        t.list_hover = hover;
        t.list_active = active;
        t.list_active_border = focus;
        t.muted = element;
        t.muted_foreground = muted;
        t.selection = selection;
        t.slider_bar = accent;
        t.slider_thumb = text;
        t.switch = element;
        t.switch_thumb = text;
        t.danger = error;
        t.danger_foreground = text;
        t.title_bar = title;
        t.title_bar_border = border;
        t.scrollbar = gpui_kit::transparent_black();
        t.scrollbar_thumb = thumb;
        t.scrollbar_thumb_hover = thumb_hover;
    });
    Ok(())
}
