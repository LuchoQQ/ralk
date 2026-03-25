use shaderc;
use std::fs;
use std::path::Path;

fn main() {
    let shader_dir = Path::new("shaders");
    let compiler = shaderc::Compiler::new().expect("Failed to create shaderc compiler");
    let mut options =
        shaderc::CompileOptions::new().expect("Failed to create shaderc compile options");
    options.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_2 as u32,
    );

    for entry in fs::read_dir(shader_dir).expect("Failed to read shaders/ directory") {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();

        let kind = match path.extension().and_then(|e| e.to_str()) {
            Some("vert") => shaderc::ShaderKind::Vertex,
            Some("frag") => shaderc::ShaderKind::Fragment,
            _ => continue,
        };

        println!("cargo:rerun-if-changed={}", path.display());

        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));

        let file_name = path.file_name().unwrap().to_str().unwrap();

        let artifact = compiler
            .compile_into_spirv(&source, kind, file_name, "main", Some(&options))
            .unwrap_or_else(|e| panic!("Shader compilation failed for {file_name}:\n{e}"));

        if artifact.get_num_warnings() > 0 {
            panic!(
                "Shader {file_name} compiled with warnings:\n{}",
                artifact.get_warning_messages()
            );
        }

        let spv_path = path.with_extension(format!(
            "{}.spv",
            path.extension().unwrap().to_str().unwrap()
        ));
        fs::write(&spv_path, artifact.as_binary_u8())
            .unwrap_or_else(|e| panic!("Failed to write {}: {e}", spv_path.display()));

        println!("cargo:warning=Compiled {file_name} -> {}", spv_path.display());
    }
}
