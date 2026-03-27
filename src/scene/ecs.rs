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
pub struct AudioSource {
    pub sound_path: String,
    pub volume: f32,
    pub looping: bool,
    pub max_distance: f32,
    pub handle: Option<SoundHandle>,
}

// ---------------------------------------------------------------------------
// Vehicle (Fase 30)
// ---------------------------------------------------------------------------

pub struct Vehicle {
    pub current_speed: f32,
    pub current_rpm: f32,
    pub max_rpm: f32,
    pub max_speed: f32,
    pub acceleration_input: f32,
    pub brake_input: f32,
    pub is_skidding: bool,
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

pub struct Checkpoint {
    pub index:          u32,
    pub is_finish_line: bool,
    pub trigger_radius: f32,
}

pub struct StreetLight {
    pub base_intensity: f32,
}

pub struct PhysicsCollider {
    #[allow(dead_code)]
    pub handle: ColliderHandle,
    pub shape: ColliderShapeType,
    pub half_extents: Vec3,
}

// ---------------------------------------------------------------------------
// Fase 38 — Parent-Child Hierarchy
// ---------------------------------------------------------------------------

/// Links this entity to its parent. When the parent moves, this entity
/// inherits the parent's world transform (world = parent_world * local).
pub struct Parent {
    pub entity: hecs::Entity,
}

/// List of child entities. Maintained in sync with `Parent` by
/// `attach_child` / `detach_child` helpers in main.rs.
pub struct Children {
    pub entities: Vec<hecs::Entity>,
}

/// Cached world-space transform matrix, computed from the local `Transform`
/// plus the parent chain. Updated once per frame before rendering.
/// Entities without a `Parent` have `matrix == transform.to_mat4()`.
pub struct WorldTransform {
    pub matrix: Mat4,
}

// ---------------------------------------------------------------------------
// Fase 39 — Prefabs
// ---------------------------------------------------------------------------

/// Marks an entity as an instance of a saved prefab.
pub struct PrefabInstance {
    pub prefab_path: String,
}

// ---------------------------------------------------------------------------
// Fase 40 — Particle System
// ---------------------------------------------------------------------------

/// One live particle (CPU-side only, not an ECS component).
#[derive(Clone)]
pub struct Particle {
    pub position:   Vec3,
    pub velocity:   Vec3,
    pub age:        f32,
    pub lifetime:   f32,
    pub start_size: f32,
    pub end_size:   f32,
    pub start_color: [f32; 4],
    pub end_color:   [f32; 4],
}

/// Emission shape for a `ParticleEmitter`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EmitterShape {
    Point,
    Sphere { radius: f32 },
    Cone   { angle_deg: f32 },
}

/// ECS component: a particle emitter attached to an entity.
pub struct ParticleEmitter {
    pub max_particles:     usize,
    pub spawn_rate:        f32,    // particles per second
    pub lifetime_min:      f32,
    pub lifetime_max:      f32,
    pub initial_velocity:  Vec3,
    pub velocity_randomness: f32,
    pub gravity_factor:    f32,
    pub start_size_min:    f32,
    pub start_size_max:    f32,
    pub end_size_min:      f32,
    pub end_size_max:      f32,
    pub start_color:       [f32; 4],
    pub end_color:         [f32; 4],
    pub shape:             EmitterShape,
    pub blend_additive:    bool,
    pub enabled:           bool,
    // Internal state
    pub particles:         Vec<Particle>,
    pub spawn_accum:       f32,
}

impl ParticleEmitter {
    /// Fire preset — orange flame rising.
    pub fn fire_preset() -> Self {
        Self {
            max_particles:     200,
            spawn_rate:        30.0,
            lifetime_min:      0.3,
            lifetime_max:      0.8,
            initial_velocity:  Vec3::new(0.0, 2.0, 0.0),
            velocity_randomness: 0.5,
            gravity_factor:    -0.5,
            start_size_min:    0.15,
            start_size_max:    0.30,
            end_size_min:      0.0,
            end_size_max:      0.05,
            start_color:       [1.0, 0.6, 0.1, 1.0],
            end_color:         [1.0, 0.0, 0.0, 0.0],
            shape:             EmitterShape::Point,
            blend_additive:    true,
            enabled:           true,
            particles:         Vec::new(),
            spawn_accum:       0.0,
        }
    }

    /// Smoke preset — grey rising cloud.
    pub fn smoke_preset() -> Self {
        Self {
            max_particles:     100,
            spawn_rate:        10.0,
            lifetime_min:      1.0,
            lifetime_max:      2.0,
            initial_velocity:  Vec3::new(0.0, 0.8, 0.0),
            velocity_randomness: 0.3,
            gravity_factor:    0.0,
            start_size_min:    0.2,
            start_size_max:    0.4,
            end_size_min:      0.5,
            end_size_max:      0.8,
            start_color:       [0.5, 0.5, 0.5, 0.5],
            end_color:         [0.3, 0.3, 0.3, 0.0],
            shape:             EmitterShape::Point,
            blend_additive:    false,
            enabled:           true,
            particles:         Vec::new(),
            spawn_accum:       0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Fase 41 — Property Animation
// ---------------------------------------------------------------------------

/// Easing function type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EasingType {
    Linear,
    EaseInOut,
}

impl EasingType {
    pub fn apply(self, t: f32) -> f32 {
        match self {
            EasingType::Linear   => t,
            EasingType::EaseInOut => t * t * (3.0 - 2.0 * t),
        }
    }
}

/// Animates one or more fields of an entity's Transform between a start and end value.
/// Triggered by a `TriggerZone` or played directly.
pub struct PropertyAnimator {
    /// Rotation Y start (radians).
    pub from_rot_y: f32,
    /// Rotation Y end (radians).
    pub to_rot_y: f32,
    /// Duration of the animation in seconds.
    pub duration: f32,
    /// Elapsed time since playback started.
    pub elapsed: f32,
    pub easing: EasingType,
    /// Whether the animation is currently playing.
    pub playing: bool,
    /// Whether to loop the animation.
    pub loop_anim: bool,
    /// When true, play in reverse (end → start).
    pub reverse: bool,
}

impl PropertyAnimator {
    pub fn door_open() -> Self {
        Self {
            from_rot_y: 0.0,
            to_rot_y: std::f32::consts::FRAC_PI_2, // 90°
            duration: 0.5,
            elapsed: 0.0,
            easing: EasingType::EaseInOut,
            playing: false,
            loop_anim: false,
            reverse: false,
        }
    }

    /// Normalized progress [0..1], clamped.
    pub fn progress(&self) -> f32 {
        if self.duration <= 0.0 { return 1.0; }
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }

    /// Current interpolated rotation Y (radians).
    pub fn current_rot_y(&self) -> f32 {
        let t = self.easing.apply(self.progress());
        if self.reverse {
            glam::Vec3::lerp(
                glam::Vec3::splat(self.to_rot_y),
                glam::Vec3::splat(self.from_rot_y),
                t,
            ).x
        } else {
            self.from_rot_y + (self.to_rot_y - self.from_rot_y) * t
        }
    }
}

// ---------------------------------------------------------------------------
// Fase 41 — Skeletal Animation (data structure, rendering TODO)
// ---------------------------------------------------------------------------

/// One keyframe value (translation, rotation, or scale).
#[derive(Clone)]
pub struct Keyframe {
    pub time: f32,
    pub value: glam::Vec4, // xyz for T/S, xyzw for R (Quat)
}

/// A channel animates one joint and one property.
#[derive(Clone)]
pub struct AnimationChannel {
    pub joint_index: usize,
    pub property: AnimationProperty,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Clone, Copy, Debug)]
pub enum AnimationProperty {
    Translation,
    Rotation,
    Scale,
}

/// A named animation clip (parsed from glTF).
#[derive(Clone)]
pub struct AnimationClip {
    pub name:     String,
    pub duration: f32,
    pub channels: Vec<AnimationChannel>,
}

/// ECS component: plays animation clips on a glTF model.
pub struct AnimationPlayer {
    pub clips:        Vec<AnimationClip>,
    pub active_clip:  usize,
    pub elapsed:      f32,
    pub speed:        f32,
    pub looping:      bool,
    pub playing:      bool,
    /// Computed joint matrices uploaded to GPU (one per joint, world-space).
    pub joint_matrices: Vec<Mat4>,
}

impl AnimationPlayer {
    pub fn new(clips: Vec<AnimationClip>) -> Self {
        Self {
            clips,
            active_clip: 0,
            elapsed: 0.0,
            speed: 1.0,
            looping: true,
            playing: true,
            joint_matrices: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Fase 42 — Trigger Zones
// ---------------------------------------------------------------------------

/// Shape of a trigger zone.
#[derive(Clone, Copy, Debug)]
pub enum TriggerShape {
    Box,
    Sphere,
}

/// Action executed when a trigger fires.
#[derive(Clone, Debug)]
pub enum TriggerAction {
    PlayAnimation { target_entity_bits: u64 },
    PlaySound     { path: String, volume: f32 },
    ToggleEntity  { target_entity_bits: u64, enabled: bool },
    SpawnPrefab   { prefab_path: String, offset: Vec3 },
}

/// A trigger zone. When the player's AABB overlaps this zone,
/// `on_enter` fires (and `on_exit` when they leave).
pub struct TriggerZone {
    pub shape: TriggerShape,
    /// Half-extents for Box shape, or radius in all components for Sphere.
    pub size:  Vec3,
    pub on_enter: Option<TriggerAction>,
    pub on_exit:  Option<TriggerAction>,
    /// If true, this trigger fires only once.
    pub once: bool,
    /// Set after the first fire when `once` is true.
    pub triggered: bool,
    /// Whether the player was inside last frame (for enter/exit detection).
    pub player_inside: bool,
    pub visible_in_editor: bool,
}

impl TriggerZone {
    pub fn new_box(size: Vec3) -> Self {
        Self {
            shape: TriggerShape::Box,
            size,
            on_enter: None,
            on_exit:  None,
            once: false,
            triggered: false,
            player_inside: false,
            visible_in_editor: true,
        }
    }

    pub fn new_sphere(radius: f32) -> Self {
        Self {
            shape: TriggerShape::Sphere,
            size: Vec3::splat(radius),
            on_enter: None,
            on_exit:  None,
            once: false,
            triggered: false,
            player_inside: false,
            visible_in_editor: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Fase 43 — Terrain with Splatmap
// ---------------------------------------------------------------------------

/// Layer definition for a terrain splatmap channel.
pub struct TerrainLayer {
    pub name:  String,
    pub color: [f32; 3],
}

/// ECS component for a terrain mesh that supports splatmap painting.
pub struct Terrain {
    /// Splatmap resolution (e.g. 512).
    pub resolution: u32,
    /// Splatmap pixel data: resolution×resolution × RGBA8.
    /// R=layer0, G=layer1, B=layer2, A=layer3.
    pub splatmap: Vec<u8>,
    /// Whether the splatmap has been modified since the last GPU upload.
    pub dirty: bool,
    /// Layer color definitions (up to 4).
    pub layers: Vec<TerrainLayer>,
    /// Path where the splatmap PNG is saved.
    pub splatmap_path: String,
    /// GPU texture handle for the blended albedo (updated when dirty).
    pub gpu_texture_index: Option<usize>,
}

impl Terrain {
    pub fn new(resolution: u32, splatmap_path: String) -> Self {
        let n = (resolution * resolution * 4) as usize;
        let mut splatmap = vec![0u8; n];
        // Fill channel 0 (grass) fully — start as all grass.
        for i in (0..n).step_by(4) {
            splatmap[i] = 255;
        }
        Self {
            resolution,
            splatmap,
            dirty: true,
            layers: vec![
                TerrainLayer { name: "Pasto".into(),  color: [0.3, 0.6, 0.2] },
                TerrainLayer { name: "Tierra".into(), color: [0.55, 0.35, 0.20] },
                TerrainLayer { name: "Roca".into(),   color: [0.50, 0.48, 0.44] },
                TerrainLayer { name: "Arena".into(),  color: [0.85, 0.77, 0.55] },
            ],
            splatmap_path,
            gpu_texture_index: None,
        }
    }

    /// Blend the splatmap channels into a flat RGBA8 texture suitable for upload.
    pub fn build_blended_rgba(&self) -> Vec<u8> {
        let n = (self.resolution * self.resolution) as usize;
        let colors: [[f32; 3]; 4] = [
            self.layers.get(0).map(|l| l.color).unwrap_or([0.3, 0.6, 0.2]),
            self.layers.get(1).map(|l| l.color).unwrap_or([0.55, 0.35, 0.2]),
            self.layers.get(2).map(|l| l.color).unwrap_or([0.5, 0.48, 0.44]),
            self.layers.get(3).map(|l| l.color).unwrap_or([0.85, 0.77, 0.55]),
        ];
        let mut out = vec![0u8; n * 4];
        for i in 0..n {
            let base = i * 4;
            let r = self.splatmap[base]     as f32 / 255.0;
            let g = self.splatmap[base + 1] as f32 / 255.0;
            let b = self.splatmap[base + 2] as f32 / 255.0;
            let a = self.splatmap[base + 3] as f32 / 255.0;
            // Blend
            let blend = [
                (colors[0][0]*r + colors[1][0]*g + colors[2][0]*b + colors[3][0]*a).min(1.0),
                (colors[0][1]*r + colors[1][1]*g + colors[2][1]*b + colors[3][1]*a).min(1.0),
                (colors[0][2]*r + colors[1][2]*g + colors[2][2]*b + colors[3][2]*a).min(1.0),
            ];
            out[base]     = (blend[0] * 255.0) as u8;
            out[base + 1] = (blend[1] * 255.0) as u8;
            out[base + 2] = (blend[2] * 255.0) as u8;
            out[base + 3] = 255;
        }
        out
    }

    /// Paint a circular brush stroke on the splatmap.
    /// `cx, cz` are UV coordinates [0..1], `layer` is 0..3, `radius` is UV-space.
    pub fn paint(&mut self, cx: f32, cz: f32, layer: usize, radius: f32, intensity: f32) {
        if layer >= 4 { return; }
        let res = self.resolution as i32;
        let px = (cx * res as f32) as i32;
        let pz = (cz * res as f32) as i32;
        let rad = (radius * res as f32) as i32 + 1;

        for dz in -rad..=rad {
            for dx in -rad..=rad {
                let x = px + dx;
                let z = pz + dz;
                if x < 0 || x >= res || z < 0 || z >= res { continue; }
                let dist = ((dx * dx + dz * dz) as f32).sqrt() / rad as f32;
                if dist > 1.0 { continue; }
                // Soft brush: falloff
                let strength = (1.0 - dist) * intensity;
                let idx = ((z * res + x) * 4) as usize;

                // Apply weight to this channel, reduce others proportionally.
                let old = self.splatmap[idx + layer] as f32 / 255.0;
                let new = (old + strength).min(1.0);
                let added = new - old;
                self.splatmap[idx + layer] = (new * 255.0) as u8;

                // Reduce the other channels proportionally to keep sum ≤ 1.
                let other_sum: f32 = [0,1,2,3].iter()
                    .filter(|&&c| c != layer)
                    .map(|&c| self.splatmap[idx + c] as f32 / 255.0)
                    .sum();
                if other_sum > 0.0 {
                    for c in [0,1,2,3] {
                        if c == layer { continue; }
                        let v = self.splatmap[idx + c] as f32 / 255.0;
                        let reduced = (v - added * (v / other_sum)).max(0.0);
                        self.splatmap[idx + c] = (reduced * 255.0) as u8;
                    }
                }
                self.dirty = true;
            }
        }
    }

    /// Save splatmap as PNG.
    pub fn save_png(&self) -> anyhow::Result<()> {
        use image::{ImageBuffer, Rgba};
        let res = self.resolution;
        let img = ImageBuffer::<Rgba<u8>, _>::from_raw(res, res, self.splatmap.clone())
            .ok_or_else(|| anyhow::anyhow!("Failed to create splatmap image buffer"))?;
        img.save(&self.splatmap_path)
            .map_err(|e| anyhow::anyhow!("Failed to save splatmap PNG: {e}"))?;
        log::info!("Splatmap saved to '{}'", self.splatmap_path);
        Ok(())
    }

    /// Load splatmap from PNG.
    pub fn load_png(&mut self) -> anyhow::Result<()> {
        use image::io::Reader as ImageReader;
        let img = ImageReader::open(&self.splatmap_path)?.decode()?.to_rgba8();
        let (w, h) = img.dimensions();
        if w != self.resolution || h != self.resolution {
            anyhow::bail!("Splatmap size mismatch: expected {}×{}, got {}×{}", self.resolution, self.resolution, w, h);
        }
        self.splatmap = img.into_raw();
        self.dirty = true;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Fase 44 — Material Override
// ---------------------------------------------------------------------------

/// Per-entity material parameter overrides applied on top of the glTF defaults.
/// Changed via the egui material editor; written to the instance SSBO each frame.
#[derive(Clone, Default)]
pub struct MaterialOverride {
    pub base_color_factor:    Option<[f32; 4]>,
    pub metallic_factor:      Option<f32>,
    pub roughness_factor:     Option<f32>,
    pub emissive_factor:      Option<[f32; 3]>,
    pub emissive_intensity:   Option<f32>,
    pub normal_scale:         Option<f32>,
    pub uv_scale:             Option<f32>,
}

impl MaterialOverride {
    /// Build the packed `override_flags` bitmask for the GPU.
    pub fn flags(&self) -> u32 {
        let mut f = 0u32;
        if self.base_color_factor.is_some() { f |= 1; }
        if self.metallic_factor.is_some()   { f |= 2; }
        if self.roughness_factor.is_some()  { f |= 4; }
        if self.emissive_factor.is_some() || self.emissive_intensity.is_some() { f |= 8; }
        f
    }
}
