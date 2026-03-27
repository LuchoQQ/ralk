pub mod config;
mod loader;
pub mod prefab;
pub mod props;
pub mod scene_file;
pub mod shader_compiler;

#[allow(unused_imports)]
pub use loader::{AssetLoader, builtin_cube, load_glb, load_multi_glb, MaterialData, MeshData, SceneData, TextureData};
pub use scene_file::{
    load_scene_file, save_scene_file,
    AudioSourceDef, ColliderDef, DirLightDef, EntityDef,
    MaterialOverrideDef, PlacedProp, PlayerState, PointLightDef,
    PropertyAnimatorDef, RigidBodyDef, SceneConfig, SceneFile,
    TriggerZoneDef,
};
pub use props::{load_props_catalog, PropDef, PropPhysics, PropsCatalog};
pub use shader_compiler::{ShaderCompiler, ShaderTarget};
pub use config::{load_config, save_config, AppConfig};
pub use prefab::{
    build_prefab_from_selection, load_prefab, save_prefab, scan_prefabs, PrefabEntityDef, PrefabFile,
};
