use glam::{Mat4, Quat, Vec3};
use rapier3d::prelude::{ColliderHandle, RigidBodyHandle};
use crate::audio::SoundHandle;

/// World-space transform decomposed into position / rotation / scale.
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    /// Decompose a Mat4 (e.g. from a glTF node) into the three components.
    pub fn from_matrix(mat: Mat4) -> Self {
        let (scale, rotation, position) = mat.to_scale_rotation_translation();
        Self { position, rotation, scale }
    }

    pub fn from_position(position: Vec3) -> Self {
        Self { position, rotation: Quat::IDENTITY, scale: Vec3::ONE }
    }

    pub fn to_mat4(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }
}

/// References a GPU mesh and a material descriptor set (both by index into
/// the arrays stored in VulkanContext).
pub struct MeshRenderer {
    pub mesh_index: usize,
    pub material_set_index: usize,
}

/// A directional (sun) light.  Direction points FROM the light TOWARD the scene.
pub struct DirectionalLight {
    pub direction: Vec3,
    pub color: Vec3,
    pub intensity: f32,
}

/// A point light.  World position comes from the entity's Transform component.
pub struct PointLight {
    pub color: Vec3,
    pub intensity: f32,
    pub radius: f32,
}

/// Local-space axis-aligned bounding box, used for frustum culling.
pub struct BoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

/// Marker: the entity that holds Camera3D is the active camera.
pub struct ActiveCamera;

// ---------------------------------------------------------------------------
// Physics components (Phase 16)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicsBodyType {
    Dynamic,
    Static,
    Kinematic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColliderShapeType {
    Box,
}

/// Links an ECS entity to a rapier3d rigid body.
pub struct PhysicsBody {
    pub handle: RigidBodyHandle,
    pub body_type: PhysicsBodyType,
}

// ---------------------------------------------------------------------------
// Audio components (Phase 17)
// ---------------------------------------------------------------------------

/// Plays a spatial sound tied to this entity's world-space position.
/// The audio system starts playback on the first frame the entity is seen and
/// updates the sink volume every frame based on distance to the camera.
pub struct AudioSource {
    /// Path to the audio file (relative to cwd).
    pub sound_path: String,
    /// Intended volume (0.0 .. 1.0), before distance attenuation.
    pub volume: f32,
    /// Whether to loop the sound indefinitely.
    pub looping: bool,
    /// Maximum audible distance in world units.
    pub max_distance: f32,
    /// Sink handle, filled in by the audio system on first play.
    pub handle: Option<SoundHandle>,
}

// ---------------------------------------------------------------------------
// Vehicle (Fase 30 — audio simulation; full physics in Fase 27)
// ---------------------------------------------------------------------------

/// A vehicle whose audio reacts to simulated RPM, speed, braking, and collisions.
///
/// Fase 30 provides the audio layer; Fase 27 will replace the simple speed/RPM
/// simulation here with proper raycast-suspension physics.
pub struct Vehicle {
    // --- Simulation state ---
    /// Forward speed in m/s (0..max_speed).
    pub current_speed: f32,
    /// Simulated engine RPM (0..max_rpm).
    pub current_rpm: f32,
    /// Maximum RPM (default 7 000).
    pub max_rpm: f32,
    /// Top speed in m/s (default 30 m/s ≈ 108 km/h).
    pub max_speed: f32,
    /// Throttle input written by the input system each frame (0..1).
    pub acceleration_input: f32,
    /// Brake input (0..1).  > 0.3 at speed triggers tyre squeal.
    pub brake_input: f32,
    /// True when the vehicle is sliding (skid sound active).
    pub is_skidding: bool,

    // --- Audio sink handles (None = not yet started) ---
    pub engine_handle: Option<SoundHandle>,
    pub skid_handle:   Option<SoundHandle>,
    pub wind_handle:   Option<SoundHandle>,
}

impl Default for Vehicle {
    fn default() -> Self {
        Self {
            current_speed: 0.0,
            current_rpm:   800.0,
            max_rpm:        7_000.0,
            max_speed:      30.0,
            acceleration_input: 0.0,
            brake_input:    0.0,
            is_skidding:    false,
            engine_handle:  None,
            skid_handle:    None,
            wind_handle:    None,
        }
    }
}

// ---------------------------------------------------------------------------
// Game logic (Fase 31)
// ---------------------------------------------------------------------------

/// A trigger zone that marks a lap checkpoint.  The game system advances
/// `next_checkpoint` when the vehicle position is within `trigger_radius` of
/// this entity's Transform.
pub struct Checkpoint {
    /// Sequential index (0-based). The finish line is the highest index.
    pub index:          u32,
    /// When true, crossing this checkpoint while all others are done = lap complete.
    pub is_finish_line: bool,
    /// Sphere trigger radius in world units.
    pub trigger_radius: f32,
}

/// Marks a PointLight as a street/circuit lamp that turns on automatically at night.
/// The day/night system sets `PointLight::intensity` to `base_intensity` during night
/// (time_of_day in 0.35..0.65) and 0.0 during the day.
pub struct StreetLight {
    /// Intensity when the light is on (night-time).
    pub base_intensity: f32,
}

/// Links an ECS entity to a rapier3d collider (stores shape info for wireframe).
pub struct PhysicsCollider {
    #[allow(dead_code)]
    pub handle: ColliderHandle,
    pub shape: ColliderShapeType,
    /// Half-extents of the box in local space.
    pub half_extents: Vec3,
}
