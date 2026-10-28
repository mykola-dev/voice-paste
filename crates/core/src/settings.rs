use serde::{Deserialize, Serialize};

// Public initial values. Personal prompts belong only in app-local configuration.
pub const INITIAL_SETTINGS: &str = include_str!("../assets/default-settings.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub hotkey: String,
    pub paste_shortcut: PasteShortcut,
    pub model: String,
    pub device: Device,
    pub language_mode: Language,
    pub enable_vad: bool,
    pub keep_in_memory: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub start_at_login: bool,
    pub indicator_color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay_acrylic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay_transparency: Option<u8>,
    pub debug_logging: bool,
    #[serde(default)]
    pub custom_initial_prompt: String,
    pub beam_size: u8,
    pub settings_window_width: f64,
    pub settings_window_height: f64,
    pub settings_window_left: f64,
    pub settings_window_top: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_prompt_height: Option<f64>,
}

fn is_false(value: &bool) -> bool {
    !value
}

/// Shared appearance intent; adapters choose native material support, never defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayAppearance {
    pub acrylic: bool,
    pub transparency: u8,
}
impl Default for OverlayAppearance {
    fn default() -> Self {
        // Approximate the current combined white tint (96 + 20-over) without
        // modifying the frozen snapshot. One common value for both modes.
        Self {
            acrylic: true,
            transparency: 57,
        }
    }
}
impl OverlayAppearance {
    pub fn background_alpha(self) -> u8 {
        ((u16::from(100 - self.transparency.min(100)) * 255 + 50) / 100) as u8
    }
}

pub const HOTKEY_CHOICES: &[(&str, &str)] = &[
    ("ScrollLock", "ScrollLock"),
    ("Numpad0", "Numpad0"),
    ("Right Alt", "RightAlt"),
    ("Right Ctrl", "RightCtrl"),
    ("F8", "F8"),
    ("F12", "F12"),
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Device {
    Cpu,
    CudaOnly,
    CudaAuto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    Auto,
    En,
    Ua,
    Bilingual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PasteShortcut {
    CtrlV,
    CtrlShiftV,
    ShiftInsert,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionOptions {
    pub device: Device,
    pub language_mode: Language,
    pub enable_vad: bool,
    pub custom_initial_prompt: String,
    pub beam_size: u8,
}

impl From<&Settings> for TranscriptionOptions {
    fn from(settings: &Settings) -> Self {
        Self {
            device: settings.device,
            language_mode: settings.language_mode,
            enable_vad: settings.enable_vad,
            custom_initial_prompt: settings.custom_initial_prompt.clone(),
            beam_size: settings.beam_size,
        }
    }
}

impl Settings {
    pub fn overlay_appearance(&self) -> OverlayAppearance {
        let defaults = OverlayAppearance::default();
        OverlayAppearance {
            acrylic: self.overlay_acrylic.unwrap_or(defaults.acrylic),
            transparency: self.overlay_transparency.unwrap_or(defaults.transparency),
        }
    }
    pub fn initial() -> Result<Self, String> {
        Self::from_json(INITIAL_SETTINGS)
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let mut value: serde_json::Value =
            serde_json::from_str(INITIAL_SETTINGS).map_err(|e| e.to_string())?;
        let supplied: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let fields = supplied
            .as_object()
            .ok_or("Settings must be a JSON object")?;
        for (key, field) in fields {
            value[key] = field.clone();
        }
        let settings: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.overlay_transparency.is_some_and(|value| value > 100) {
            return Err("Overlay transparency must be 0..100 percent".into());
        }
        if self
            .initial_prompt_height
            .is_some_and(|height| !height.is_finite() || !(80.0..=600.0).contains(&height))
        {
            return Err("Initial prompt height must be 80..600 logical pixels".into());
        }
        if self.version != 1 {
            return Err("Unsupported settings version".into());
        }
        Hotkey::parse(&self.hotkey)?;
        crate::models::find(&self.model)?;
        if !(1..=10).contains(&self.beam_size) {
            return Err("beamSize must be 1..10".into());
        }
        let color = self.indicator_color.as_bytes();
        if color.len() != 7 || color[0] != b'#' || !color[1..].iter().all(u8::is_ascii_hexdigit) {
            return Err("indicatorColor must be #RRGGBB".into());
        }
        if ![
            self.settings_window_width,
            self.settings_window_height,
            self.settings_window_left,
            self.settings_window_top,
        ]
        .iter()
        .all(|v| v.is_finite())
        {
            return Err("Window geometry must be finite".into());
        }
        if !(480.0..=1400.0).contains(&self.settings_window_width)
            || !(420.0..=1200.0).contains(&self.settings_window_height)
        {
            return Err("Window dimensions outside supported bounds".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub modifiers: u32,
    pub key: u32,
}

impl Hotkey {
    pub fn parse(text: &str) -> Result<Self, String> {
        let parts: Vec<_> = text.split('+').map(str::trim).collect();
        let (key, modifiers) = parts.split_last().ok_or("Empty hotkey")?;
        let mut flags = 0;
        for modifier in modifiers {
            let flag = match modifier.to_ascii_lowercase().as_str() {
                "alt" => 1,
                "ctrl" | "control" => 2,
                "shift" => 4,
                "win" => 8,
                _ => return Err("Unknown hotkey modifier".into()),
            };
            if flags & flag != 0 {
                return Err("Duplicate hotkey modifier".into());
            }
            flags |= flag;
        }
        let upper = key.to_ascii_uppercase();
        let key = match upper.as_str() {
            "SCROLLLOCK" => 0x91,
            "NUMPAD0" => 0x60,
            "RIGHTALT" | "RIGHT ALT" => 0xa5,
            "RIGHTCTRL" | "RIGHT CTRL" => 0xa3,
            "SPACE" => 0x20,
            "INSERT" => 0x2d,
            _ if upper.len() == 1 && upper.as_bytes()[0].is_ascii_alphanumeric() => {
                upper.as_bytes()[0] as u32
            }
            _ => upper
                .strip_prefix('F')
                .and_then(|n| n.parse::<u32>().ok())
                .filter(|n| (1..=24).contains(n))
                .map(|n| 0x70 + n - 1)
                .ok_or("Unsupported hotkey key")?,
        };
        Ok(Self {
            modifiers: flags,
            key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_defaults_round_trip_without_changes() {
        let defaults = Settings::initial().unwrap();
        let actual = serde_json::to_value(&defaults).unwrap();
        let mut expected: serde_json::Value = serde_json::from_str(INITIAL_SETTINGS).unwrap();
        assert!(expected.get("customInitialPrompt").is_none());
        expected["customInitialPrompt"] = serde_json::json!("");
        assert_eq!(
            actual.as_object().unwrap().len(),
            expected.as_object().unwrap().len()
        );
        for (key, value) in expected.as_object().unwrap() {
            if value.is_number() {
                // JSON 638 and 638.0 represent the same DIP width.
                assert_eq!(actual[key].as_f64(), value.as_f64(), "{key}");
            } else {
                assert_eq!(&actual[key], value, "{key}");
            }
        }
        assert_eq!(
            defaults,
            Settings::from_json(&serde_json::to_string(&defaults).unwrap()).unwrap()
        );
    }
    #[test]
    fn public_defaults_have_no_prompt_and_saved_prompts_round_trip_verbatim() {
        assert!(
            Settings::initial()
                .unwrap()
                .custom_initial_prompt
                .is_empty()
        );
        let prompt = "Example vocabulary\r\nПриклад словника";
        let saved =
            Settings::from_json(&serde_json::json!({"customInitialPrompt": prompt}).to_string())
                .unwrap();
        assert_eq!(saved.custom_initial_prompt, prompt);
        assert_eq!(
            TranscriptionOptions::from(&saved).custom_initial_prompt,
            prompt
        );
        assert_eq!(
            Settings::from_json(&serde_json::to_string(&saved).unwrap()).unwrap(),
            saved
        );
    }

    #[test]
    fn partial_settings_preserve_defaults_and_invalid_values_fail() {
        assert_eq!(
            Settings::from_json("{}").unwrap(),
            Settings::initial().unwrap()
        );
        assert!(Settings::from_json(r#"{"beamSize":0}"#).is_err());
        assert!(Settings::from_json(r#"{"enableVad":"false"}"#).is_err());
        assert!(Settings::from_json("[]").is_err());
    }
    #[test]
    fn parses_hotkey_choices() {
        for (_, key) in HOTKEY_CHOICES {
            assert!(Hotkey::parse(key).is_ok(), "{key}");
        }
        assert_eq!(Hotkey::parse("Numpad0").unwrap().key, 0x60);
        assert_eq!(Hotkey::parse("Right Alt").unwrap().key, 0xa5);
        assert_eq!(Hotkey::parse("Right Ctrl").unwrap().key, 0xa3);
        for key in ["ScrollLock", "F8", "F9", "F10", "Ctrl+Alt+Space"] {
            assert!(Hotkey::parse(key).is_ok());
        }
        assert_eq!(
            Hotkey::parse("Ctrl+Alt+Space").unwrap(),
            Hotkey {
                modifiers: 3,
                key: 32
            }
        );
        assert!(Hotkey::parse("Ctrl+Ctrl+F8").is_err());
    }
    #[test]
    fn preferred_overlay_color_is_the_default_without_overwriting_saved_colors() {
        assert_eq!(Settings::initial().unwrap().indicator_color, "#E9563D");
        assert_eq!(
            Settings::from_json("{}").unwrap().indicator_color,
            "#E9563D"
        );
        assert_eq!(
            Settings::from_json(r##"{"indicatorColor":"#123456"}"##)
                .unwrap()
                .indicator_color,
            "#123456"
        );
    }
    #[test]
    fn startup_is_opt_in_and_round_trips_without_changing_old_defaults() {
        assert!(!Settings::initial().unwrap().start_at_login);
        let enabled = Settings::from_json(r#"{"startAtLogin":true}"#).unwrap();
        assert!(enabled.start_at_login);
        assert_eq!(
            Settings::from_json(&serde_json::to_string(&enabled).unwrap()).unwrap(),
            enabled
        );
    }

    #[test]
    fn overlay_appearance_defaults_and_common_transparency_round_trip() {
        let old = Settings::from_json("{}").unwrap();
        assert_eq!(old.overlay_appearance(), OverlayAppearance::default());
        assert!(old.overlay_appearance().acrylic);
        assert_eq!(old.overlay_appearance().transparency, 57);
        for acrylic in [false, true] {
            for transparency in [0, 57, 100] {
                let text = format!(
                    r#"{{"overlayAcrylic":{acrylic},"overlayTransparency":{transparency}}}"#
                );
                let saved = Settings::from_json(&text).unwrap();
                let appearance = saved.overlay_appearance();
                assert_eq!(
                    appearance,
                    OverlayAppearance {
                        acrylic,
                        transparency
                    }
                );
                assert_eq!(
                    appearance.background_alpha(),
                    match transparency {
                        0 => 255,
                        100 => 0,
                        _ => 110,
                    }
                );
                assert_eq!(
                    Settings::from_json(&serde_json::to_string(&saved).unwrap()).unwrap(),
                    saved
                );
                assert_eq!(
                    TranscriptionOptions::from(&saved),
                    TranscriptionOptions::from(&old)
                );
            }
        }
        for invalid in [
            r#"{"overlayTransparency":101}"#,
            r#"{"overlayTransparency":-1}"#,
            r#"{"overlayTransparency":0.5}"#,
            r#"{"overlayAcrylic":"true"}"#,
        ] {
            assert!(Settings::from_json(invalid).is_err(), "{invalid}");
        }
    }
}
