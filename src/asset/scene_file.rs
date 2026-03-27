use anyhow::Result;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// scene.json format
// ---------------------------------------------------------------------------

/// Top-level scene file. Saved/loaded from `scene.json` in the working directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneFile {
    /// Paths to glTF/glb model files to load (relative to cwd).
    pub models: Vec<String>,
    /// Renderable entities: each references global mesh/material indices.
    pub entities: Vec<EntityDef>,
    /// Single directional (sun) light.
    pub directional_light: DirLightDef,
    /// Point lights.
    #[serde(default)]
    pub point_lights: Vec<PointLightDef>,
    /// Lua script paths to load when the scene starts (Fase 26).
    #[serde(default)]
    pub scripts: Vec<String>,
}

/// AudioSource parameters stored in scene.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSourceDef {
    pub sound_path: String,
    pub volume: f32,
    pub looping: bool,
    pub max_distance: f32,
}

/// Physics body type stored in scene.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigidBodyDef {
    /// "dynamic", "static", or "kinematic"
    pub body_type: String,
}

/// Box collider parameters stored in scene.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColliderDef {
    pub half_extents: [f32; 3],
    pub restitution: f32,
    pub friction: f32,
}

/// One renderable entity in the scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDef {
    /// Index into the global mesh array (flattened across all loaded models).
    pub mesh_index: usize,
    /// Index into the global material_sets array.
    pub material_set_index: usize,
    pub position: [f32; 3],
    /// Quaternion stored as [x, y, z, w].
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rigid_body: Option<RigidBodyDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collider: Option<ColliderDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_source: Option<AudioSourceDef>,
}

/// Directional (sun) light parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirLightDef {
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
}

/// Point light parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointLightDef {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub radius: f32,
}

// ---------------------------------------------------------------------------
// I/O helpers
// ---------------------------------------------------------------------------

pub fn load_scene_file(path: &str) -> Result<SceneFile> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read scene file '{}': {}", path, e))?;
    let scene = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse scene file '{}': {}", path, e))?;
    Ok(scene)
}

pub fn save_scene_file(path: &str, scene: &SceneFile) -> Result<()> {
    let content = serde_json::to_string_pretty(scene)
        .map_err(|e| anyhow::anyhow!("Failed to serialize scene: {}", e))?;
    std::fs::write(path, &content)
        .map_err(|e| anyhow::anyhow!("Failed to write scene file '{}': {}", path, e))?;
    log::info!("Scene saved to '{}'", path);
    Ok(())
}
