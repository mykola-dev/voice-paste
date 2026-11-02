//! Presentation data selected at composition boundaries, never separate layouts.
use voicepaste_core::settings::SettingsPlatform;

pub(super) struct Profile {
    pub platform: SettingsPlatform,
    pub cpu_hint: &'static str,
    pub startup_hint: &'static str,
    pub memory_hint: &'static str,
    /// CT2 names its weight artifact; the GGML catalog contains one weight file.
    pub weight_path: Option<&'static str>,
    pub weight_description: &'static str,
}

#[cfg(target_os = "windows")]
pub(super) const CURRENT: Profile = Profile {
    platform: SettingsPlatform::Windows,
    cpu_hint: "CPU INT8; GPU not used.",
    startup_hint: "Current Windows user only. Applied with OK or Apply.",
    memory_hint: "Resident RAM is memory currently held in physical RAM. Model size is the selected model.bin file size, not RAM or VRAM usage; it is shown even before download.",
    weight_path: Some("model.bin"),
    weight_description: "",
};

#[cfg(target_os = "macos")]
pub(super) const CURRENT: Profile = Profile {
    platform: SettingsPlatform::Macos,
    cpu_hint: "CPU inference; full FP16 model weights, no GPU offload.",
    startup_hint: "Current macOS user only. Applied with OK or Apply; takes effect on next sign-in.",
    memory_hint: "Host and STT worker resident RAM are measured separately. GGML file size is not total RAM or Metal memory usage.",
    weight_path: None,
    weight_description: " (full FP16 GGML)",
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_profile_identifies_one_pinned_weight_per_supported_model() {
        for name in ["large-v2", "large-v3", "large-v3-turbo"] {
            let model = voicepaste_core::models::find_for(name, CURRENT.platform).unwrap();
            let weights: Vec<_> = model
                .files
                .iter()
                .filter(|file| CURRENT.weight_path.is_none_or(|path| file.path == path))
                .collect();
            assert_eq!(weights.len(), 1);
            assert!(weights[0].bytes > 0);
            let label = super::super::model_size_label(name);
            assert!(label.contains(&format!("{:.2} GiB", weights[0].bytes as f64 / 1073741824.)));
        }
    }
}
