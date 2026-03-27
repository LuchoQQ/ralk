use anyhow::Result;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Props catalog format (assets/props/default_props.json)
// ---------------------------------------------------------------------------

/// Physics behavior of a prop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PropPhysics {
    Dynamic,
    Static,
    None,
}

impl Default for PropPhysics {
    fn default() -> Self { PropPhysics::Static }
}

/// A single entry in the props catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropDef {
    pub id: String,
    pub name: String,
    pub model: String,
    #[serde(default)]
    pub thumbnail: String,
    #[serde(default)]
    pub physics: PropPhysics,
    #[serde(default = "default_collider")]
    pub collider: String,
    #[serde(default = "default_category")]
    pub category: String,
    /// Default spawn scale [x, y, z]. Omit for 1×1×1.
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
}

fn default_collider() -> String { "box".to_string() }
fn default_category() -> String { "objetos".to_string() }
fn default_scale() -> [f32; 3] { [1.0, 1.0, 1.0] }

/// Top-level props catalog file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropsCatalog {
    pub props: Vec<PropDef>,
}

pub fn load_props_catalog(path: &str) -> Result<PropsCatalog> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Could not read props catalog '{}': {}", path, e))?;
    let catalog = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Could not parse props catalog '{}': {}", path, e))?;
    Ok(catalog)
}
