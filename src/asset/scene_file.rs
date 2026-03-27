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
    /// Fase 38: index of parent entity in the entities array (null = root).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_index: Option<usize>,
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

/// Property animation parameters stored in scene.json (Fase 41).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyAnimatorDef {
    pub from_rot_y: f32,
    pub to_rot_y: f32,
    pub duration: f32,
    #[serde(default)]
    pub looping: bool,
}

/// Trigger zone parameters stored in scene.json (Fase 42).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerZoneDef {
    /// "box" or "sphere"
    pub shape: String,
    pub size: [f32; 3],
    /// "play_animation", "play_sound", "toggle_entity"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_enter_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_enter_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_enter_sound_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_enter_sound_volume: Option<f32>,
    #[serde(default)]
    pub once: bool,
}

/// Material override parameters stored in scene.json (Fase 44).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialOverrideDef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_color: Option<[f32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metallic: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roughness: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emissive: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emissive_intensity: Option<f32>,
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
    /// Optional display name for the scene tree (Fase 38).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Fase 38: index of parent entity in the entities array (null = root).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rigid_body: Option<RigidBodyDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collider: Option<ColliderDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_source: Option<AudioSourceDef>,
    /// Fase 41: property animation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_animator: Option<PropertyAnimatorDef>,
    /// Fase 42: trigger zone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_zone: Option<TriggerZoneDef>,
    /// Fase 44: material override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material_override: Option<MaterialOverrideDef>,
    /// Fase 39: prefab this entity came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefab_path: Option<String>,
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
