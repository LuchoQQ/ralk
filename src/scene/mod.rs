mod camera;
mod culling;
mod ecs;
mod gizmo;
mod lights;
mod picking;

pub use camera::{Camera3D, EYE_OFFSET, PLAYER_SPAWN_Y};
pub use culling::{extract_frustum_planes, transform_aabb};
pub use ecs::{
    // Core rendering
    ActiveCamera, AudioSource, BoundingBox, ColliderShapeType, DirectionalLight,
    MeshRenderer, PhysicsBody, PhysicsBodyType, PhysicsCollider, PointLight,
    StreetLight, Transform, Vehicle, Checkpoint,
    // Fase 38: hierarchy
    Parent, Children, WorldTransform,
    // Fase 39: prefabs
    PrefabInstance,
    // Fase 40: particles
    EmitterShape, Particle, ParticleEmitter,
    // Fase 41: animation
    AnimationChannel, AnimationClip, AnimationPlayer, AnimationProperty,
    EasingType, Keyframe, PropertyAnimator,
    // Fase 42: triggers
    TriggerAction, TriggerShape, TriggerZone,
    // Fase 43: terrain
    Terrain, TerrainLayer,
    // Fase 44: material override
    MaterialOverride,
};
pub use gizmo::{
    build_axis_groups, build_selection_group, drag_axis_dir, hit_test_gizmo,
    GizmoAxis, GizmoDrag, GizmoMode, LineGroup,
    world_to_screen as gizmo_world_to_screen,
};
pub use lights::{compute_light_mvp, LightingUbo};
pub use picking::{ray_aabb, screen_to_ray};
