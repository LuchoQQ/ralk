pub mod config;
mod loader;
pub mod props;
pub mod scene_file;
pub mod shader_compiler;

#[allow(unused_imports)]
pub use loader::{AssetLoader, builtin_cube, load_glb, load_multi_glb, MaterialData, MeshData, SceneData, TextureData};
pub use scene_file::{
    load_scene_file, save_scene_file, AudioSourceDef, ColliderDef, DirLightDef, EntityDef,
    PlacedProp, PlayerState, PointLightDef, RigidBodyDef, SceneConfig, SceneFile,
};
pub use props::{load_props_catalog, PropDef, PropPhysics, PropsCatalog};
pub use shader_compiler::{ShaderCompiler, ShaderTarget};
pub use config::{load_config, save_config, AppConfig};
