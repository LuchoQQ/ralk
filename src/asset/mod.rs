mod loader;
pub mod scene_file;
pub mod shader_compiler;

#[allow(unused_imports)]
pub use loader::{AssetLoader, builtin_cube, load_glb, load_multi_glb, MaterialData, MeshData, SceneData, TextureData};
pub use scene_file::{
    load_scene_file, save_scene_file, AudioSourceDef, ColliderDef, DirLightDef, EntityDef,
    PointLightDef, RigidBodyDef, SceneFile,
};
pub use shader_compiler::{ShaderCompiler, ShaderTarget};
