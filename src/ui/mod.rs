pub mod panels;

use winit::window::Window;

/// Stats passed from the main loop to the UI each frame.
pub struct FrameStats {
    pub fps: f32,
    pub frame_ms: f32,
    /// Entities that passed frustum culling and were submitted for rendering.
    pub draw_calls: usize,
    /// Total renderable entities before culling.
    pub total_entities: usize,
    /// Recent shader reload messages (successes and errors).
    pub reload_log: Vec<String>,
}

/// Mutable settings controlled via the egui settings panel.
pub struct DebugSettings {
    /// Tone mapping: false = Reinhard, true = ACES Filmic.
    pub tone_aces: bool,
    /// Current MSAA sample count (1, 2, or 4).
    pub msaa_samples: u32,
    /// Maximum MSAA supported by the hardware.
    pub msaa_max: u32,
}

/// Audio settings controlled via the egui audio panel.
/// Values are synced to AudioEngine each frame by the main loop.
pub struct AudioUiState {
    /// Master volume (0.0 .. 1.0).
    pub master_volume: f32,
    /// Whether all audio output is silenced.
    pub muted: bool,
}

/// State shared between the physics panel and the main loop.
/// Flags are set by the panel and consumed (reset) by the main loop each frame.
pub struct PhysicsUiState {
    /// Set by panel when the user clicks "Spawn Physics Cube".
    pub spawn_cube_clicked: bool,
    /// Toggles rendering of collider wireframes.
    pub show_wireframe: bool,
}

/// State shared between the scene panel and the main loop.
/// The panel sets `save_clicked`/`load_clicked` for one frame; the main loop
/// resets them to `false` before each `build()` call.
pub struct SceneUiState {
    /// Set by panel when the user clicks "Save Scene".
    pub save_clicked: bool,
    /// Set by panel when the user clicks "Load Scene".
    pub load_clicked: bool,
    /// Status message shown in the panel (last save/load result).
    pub status: String,
    /// Number of loaded model files (display-only).
    pub model_count: usize,
    /// Number of renderable entities (display-only).
    pub entity_count: usize,
}

/// State shared between the editor panel and the main loop.
/// Mirrors the selected entity's transform for display/editing in egui.
pub struct EditorUiState {
    pub selected_entity: Option<hecs::Entity>,
    pub position: [f32; 3],
    pub rotation_euler_deg: [f32; 3],  // degrees for display
    pub scale: [f32; 3],
    pub gizmo_mode: u8,  // 0=Translate, 1=Rotate, 2=Scale
    pub transform_changed: bool,  // set by panel when sliders edited
}

/// Wraps egui context + winit state. Renderer lives in VulkanContext to keep
/// Vulkan concerns together.
pub struct DebugUi {
    pub ctx: egui::Context,
    pub winit_state: egui_winit::State,
}

impl DebugUi {
    pub fn new(window: &Window) -> Self {
        let ctx = egui::Context::default();
        let winit_state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        Self { ctx, winit_state }
    }

    /// Feed a winit window event to egui. Returns `consumed` flag — if true,
    /// the event was handled by egui and should not be forwarded to the camera.
    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &winit::event::WindowEvent,
    ) -> bool {
        self.winit_state.on_window_event(window, event).consumed
    }

    /// Build one frame of UI. Returns clipped primitives + texture delta.
    pub fn build(
        &mut self,
        window: &Window,
        stats: &FrameStats,
        world: &mut hecs::World,
        settings: &mut DebugSettings,
        scene: &mut SceneUiState,
        physics: &mut PhysicsUiState,
        audio: &mut AudioUiState,
        editor: &mut EditorUiState,
    ) -> (Vec<egui::ClippedPrimitive>, egui::TexturesDelta) {
        let raw_input = self.winit_state.take_egui_input(window);
        let output = self.ctx.run(raw_input, |ctx| {
            panels::stats_panel(ctx, stats);
            panels::lights_panel(ctx, world);
            panels::settings_panel(ctx, settings);
            panels::scene_panel(ctx, scene);
            panels::physics_panel(ctx, physics);
            panels::audio_panel(ctx, audio);
            panels::editor_panel(ctx, editor);
        });
        self.winit_state.handle_platform_output(window, output.platform_output);
        let clipped = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        (clipped, output.textures_delta)
    }

    pub fn pixels_per_point(&self) -> f32 {
        self.ctx.pixels_per_point()
    }
}
