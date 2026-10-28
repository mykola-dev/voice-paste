use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Model {
    pub name: String,
    pub repository: String,
    pub revision: String,
    pub files: Vec<Artifact>,
}
#[derive(Deserialize)]
struct Catalog {
    models: Vec<Model>,
}

pub fn catalog() -> Result<Vec<Model>, String> {
    let catalog: Catalog = serde_json::from_str(include_str!("../assets/model-artifacts.json"))
        .map_err(|e| e.to_string())?;
    Ok(catalog.models)
}
pub fn find(name: &str) -> Result<Model, String> {
    catalog()?
        .into_iter()
        .find(|model| model.name == name)
        .ok_or_else(|| format!("Unsupported model: {name}"))
}

/// Visible label for the Settings model dropdown. Internal ids, saved configs
/// and on-disk model folders stay unchanged; only the label carries the
/// Whisper prefix so the model family is obvious.
pub fn display_name(name: &str) -> String {
    format!("Whisper {name}")
}

/// Reverse of `display_name` for dropdown selections. Labels that do not match
/// a catalog model pass through unchanged, preserving previously accepted
/// custom values instead of corrupting them.
pub fn id_from_display(label: &str) -> String {
    match label.strip_prefix("Whisper ") {
        Some(id) if catalog().is_ok_and(|models| models.iter().any(|model| model.name == id)) => {
            id.to_owned()
        }
        _ => label.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_labels_round_trip_through_catalog_ids() {
        for model in catalog().unwrap() {
            assert_eq!(id_from_display(&display_name(&model.name)), model.name);
        }
    }

    #[test]
    fn unknown_labels_pass_through_unchanged() {
        assert_eq!(id_from_display("large-v2"), "large-v2");
        assert_eq!(id_from_display("custom-model"), "custom-model");
        assert_eq!(
            id_from_display("Whisper custom-model"),
            "Whisper custom-model"
        );
    }
}
