/// Prefab system (Fase 39).
///
/// A prefab is a small JSON file in `assets/prefabs/` that stores a group of
/// entities with transforms relative to the group's center.  Spawning a prefab
/// instantiates all entities at a target position + offset.
use anyhow::Result;
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Prefab format
// ---------------------------------------------------------------------------

/// One entity inside a prefab, with transform relative to the group center.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabEntityDef {
    pub mesh_index:         usize,
    pub material_set_index: usize,
    /// Local position offset from the prefab origin.
    pub position_offset: [f32; 3],
    pub rotation: [f32; 4],
    pub scale:    [f32; 3],
    /// Optional display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Parent index within this prefab's entity list (null = root).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_index: Option<usize>,
    /// Prop ID if this entity came from the catalog.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prop_id: Option<String>,
}

/// Top-level prefab file, stored in `assets/prefabs/{name}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabFile {
    pub name:     String,
    pub entities: Vec<PrefabEntityDef>,
}

// ---------------------------------------------------------------------------
// I/O helpers
// ---------------------------------------------------------------------------

pub fn save_prefab(path: &str, prefab: &PrefabFile) -> Result<()> {
    let dir = std::path::Path::new(path).parent().unwrap_or(std::path::Path::new("."));
    let _ = std::fs::create_dir_all(dir);
    let json = serde_json::to_string_pretty(prefab)
        .map_err(|e| anyhow::anyhow!("Failed to serialize prefab: {e}"))?;
    std::fs::write(path, json)
        .map_err(|e| anyhow::anyhow!("Failed to write prefab '{}': {e}", path))?;
    log::info!("Prefab saved to '{path}'");
    Ok(())
}

pub fn load_prefab(path: &str) -> Result<PrefabFile> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read prefab '{}': {e}", path))?;
    serde_json::from_str(&json)
        .map_err(|e| anyhow::anyhow!("Failed to parse prefab '{}': {e}", path))
}

/// Scan `assets/prefabs/` and return a list of (display_name, path) pairs.
pub fn scan_prefabs(dir: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") { continue; }
        if let Ok(pf) = load_prefab(path.to_str().unwrap_or("")) {
            out.push((pf.name.clone(), path.to_string_lossy().into_owned()));
        }
    }
    out
}

/// Compute the centroid of a set of world positions.
pub fn centroid(positions: &[Vec3]) -> Vec3 {
    if positions.is_empty() { return Vec3::ZERO; }
    positions.iter().copied().fold(Vec3::ZERO, |a, b| a + b) / positions.len() as f32
}

/// Build a `PrefabFile` from a list of selected entity data.
pub fn build_prefab_from_selection(
    name: &str,
    entities: &[(usize, usize, Vec3, Quat, Vec3, Option<String>)], // (mesh, mat, pos, rot, scale, name)
) -> PrefabFile {
    let center = centroid(&entities.iter().map(|e| e.2).collect::<Vec<_>>());
    let defs: Vec<PrefabEntityDef> = entities.iter().map(|(mi, mati, pos, rot, sc, nm)| {
        PrefabEntityDef {
            mesh_index: *mi,
            material_set_index: *mati,
            position_offset: (*pos - center).to_array(),
            rotation: rot.to_array(),
            scale: sc.to_array(),
            name: nm.clone(),
            parent_index: None,
            prop_id: None,
        }
    }).collect();
    PrefabFile { name: name.to_string(), entities: defs }
}
