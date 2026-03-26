use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Which pipeline this shader pair belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderTarget {
    Main,   // triangle.vert + triangle.frag
    Shadow, // shadow.vert  + shadow.frag
    Skybox, // skybox.vert  + skybox.frag
}

impl ShaderTarget {
    fn vert_name(self) -> &'static str {
        match self {
            Self::Main   => "triangle.vert",
            Self::Shadow => "shadow.vert",
            Self::Skybox => "skybox.vert",
        }
    }

    fn frag_name(self) -> &'static str {
        match self {
            Self::Main   => "triangle.frag",
            Self::Shadow => "shadow.frag",
            Self::Skybox => "skybox.frag",
        }
    }

    /// Map a filename stem (e.g. "triangle.frag") to its ShaderTarget.
    fn from_filename(name: &str) -> Option<Self> {
        match name {
            "triangle.vert" | "triangle.frag" => Some(Self::Main),
            "shadow.vert"   | "shadow.frag"   => Some(Self::Shadow),
            "skybox.vert"   | "skybox.frag"   => Some(Self::Skybox),
            _ => None,
        }
    }
}

/// Watches `shaders/` and recompiles GLSL → SPIR-V on file changes.
pub struct ShaderCompiler {
    compiler: shaderc::Compiler,
    shader_dir: PathBuf,
    // `_watcher` must be kept alive; dropping it stops the watch.
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Event>,
    /// Errors from the last `check_changes()` call. Cleared on each call.
    pub errors: Vec<String>,
}

impl ShaderCompiler {
    /// Start watching `shader_dir`. Returns Err only if the watcher can't be set up.
    pub fn new(shader_dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let shader_dir = shader_dir.as_ref().to_path_buf();
        let compiler = shaderc::Compiler::new()
            .ok_or_else(|| anyhow::anyhow!("Failed to create shaderc compiler"))?;

        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                tx.send(event).ok();
            }
        })?;
        watcher.watch(&shader_dir, RecursiveMode::NonRecursive)?;
        log::info!("Shader watcher started on {}", shader_dir.display());

        Ok(Self { compiler, shader_dir, _watcher: watcher, rx, errors: Vec::new() })
    }

    /// Poll for file-system events. If a shader file changed, compiles the full
    /// vert+frag pair and returns `Some((target, vert_spv, frag_spv))`.
    /// Returns `None` if nothing changed or if compilation failed (errors go to `self.errors`).
    pub fn check_changes(&mut self) -> Option<(ShaderTarget, Vec<u8>, Vec<u8>)> {
        self.errors.clear();

        // Drain all pending events and collect unique targets.
        let mut targets = std::collections::HashSet::new();
        loop {
            match self.rx.try_recv() {
                Ok(event) => {
                    // Only react to create/modify/rename events.
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {}
                        _ => continue,
                    }
                    for path in &event.paths {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if let Some(target) = ShaderTarget::from_filename(name) {
                                targets.insert(target as u8); // HashSet needs Hash; use u8 discriminant
                                log::debug!("Shader change detected: {name}");
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }

        if targets.is_empty() {
            return None;
        }

        // Compile each changed target and return the first successful one.
        // (Usually only one changes at a time.)
        let all_targets = [ShaderTarget::Main, ShaderTarget::Shadow, ShaderTarget::Skybox];
        for target in &all_targets {
            let discriminant = *target as u8;
            if !targets.contains(&discriminant) {
                continue;
            }

            match self.compile_pair(*target) {
                Ok((vert_spv, frag_spv)) => {
                    log::info!("✓ Reloaded: {}", target.frag_name());
                    return Some((*target, vert_spv, frag_spv));
                }
                Err(e) => {
                    let msg = format!("✗ {}: {e}", target.frag_name());
                    log::warn!("{msg}");
                    self.errors.push(msg);
                }
            }
        }

        None
    }

    fn compile_pair(&self, target: ShaderTarget) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        let vert_spv = self.compile_file(target.vert_name(), shaderc::ShaderKind::Vertex)?;
        let frag_spv = self.compile_file(target.frag_name(), shaderc::ShaderKind::Fragment)?;
        Ok((vert_spv, frag_spv))
    }

    fn compile_file(&self, filename: &str, kind: shaderc::ShaderKind) -> anyhow::Result<Vec<u8>> {
        let path = self.shader_dir.join(filename);
        let source = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Cannot read {filename}: {e}"))?;

        let mut options = shaderc::CompileOptions::new()
            .ok_or_else(|| anyhow::anyhow!("Failed to create compile options"))?;
        options.set_target_env(
            shaderc::TargetEnv::Vulkan,
            shaderc::EnvVersion::Vulkan1_2 as u32,
        );

        let artifact = self.compiler
            .compile_into_spirv(&source, kind, filename, "main", Some(&options))
            .map_err(|e| {
                // shaderc errors include filename + line — pass them through as-is.
                anyhow::anyhow!("{e}")
            })?;

        Ok(artifact.as_binary_u8().to_vec())
    }
}
