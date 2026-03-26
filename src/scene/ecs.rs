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

/// Links an ECS entity to a rapier3d collider (stores shape info for wireframe).
pub struct PhysicsCollider {
    #[allow(dead_code)]
    pub handle: ColliderHandle,
    pub shape: ColliderShapeType,
    /// Half-extents of the box in local space.
    pub half_extents: Vec3,
}
