use anyhow::Result;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// scene.json format
// ---------------------------------------------------------------------------

/// Scene configuration: which assets to use.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SceneConfig {
    #[serde(default)]
    pub skybox: String,
    #[serde(default)]
    pub terrain: String,
    #[serde(default)]
    pub character: String,
    #[serde(default)]
    pub props_catalog: String,
}

/// Player state saved/restored between sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub position: [f32; 3],
    pub camera_yaw: f32,
    pub camera_pitch: f32,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self { position: [0.0, 0.9, 5.0], camera_yaw: 0.0, camera_pitch: 0.0 }
    }
}

/// A placed prop instance in the scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedProp {
    pub prop_id: String,
    /// Model path (resolved from catalog at load time).
    #[serde(default)]
    pub model: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

/// Top-level scene file. Saved/loaded from `scenes/{name}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneFile {
    /// Scene configuration (skybox, terrain, character, props catalog).
    #[serde(default)]
    pub config: SceneConfig,
    /// Player state (position, camera orientation).
    #[serde(default)]
    pub player: PlayerState,
    /// Time of day (0..1).
    #[serde(default)]
    pub time_of_day: f32,
    /// Day/night cycle speed multiplier.
    #[serde(default = "default_day_night_speed")]
    pub day_night_speed: f32,
    /// Paths to glTF/glb model files to load (relative to cwd).
    pub models: Vec<String>,
    /// Renderable entities: each references global mesh/material indices.
    pub entities: Vec<EntityDef>,
    /// Placed props (from catalog, by prop_id).
    #[serde(default)]
    pub placed_props: Vec<PlacedProp>,
    /// Single directional (sun) light.
    pub directional_light: DirLightDef,
    /// Point lights.
    #[serde(default)]
    pub point_lights: Vec<PointLightDef>,
    /// Lua script paths to load when the scene starts (Fase 26).
    #[serde(default)]
    pub scripts: Vec<String>,
}

fn default_day_night_speed() -> f32 { 1.0 }

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
