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
