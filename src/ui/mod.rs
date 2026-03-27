pub mod panels;

use winit::window::Window;

/// GPU profiler data passed to the UI each frame.
pub struct GpuTimings {
    /// Whether timestamp queries are supported on this device.
    pub available: bool,
    /// Per-pass timings (name + ms). Empty if `available` is false.
    pub passes: Vec<(String, f32)>,
    /// Total GPU time for the frame (sum of all pass timings), in ms.
    pub total_ms: f32,
    /// Whether pipeline statistics queries are supported.
    pub stats_available: bool,
    pub vertex_invocations: u64,
    pub fragment_invocations: u64,
    pub clipping_primitives: u64,
}

impl Default for GpuTimings {
    fn default() -> Self {
        Self {
            available: false,
            passes: Vec::new(),
            total_ms: 0.0,
            stats_available: false,
            vertex_invocations: 0,
            fragment_invocations: 0,
            clipping_primitives: 0,
        }
    }
}

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
    /// GPU profiler results.
    pub gpu_timings: GpuTimings,
}

/// Mutable settings controlled via the egui settings panel.
pub struct DebugSettings {
    /// Tone mapping: false = Reinhard, true = ACES Filmic.
    pub tone_aces: bool,
    /// Current MSAA sample count (1, 2, or 4).
    pub msaa_samples: u32,
    /// Maximum MSAA supported by the hardware.
    pub msaa_max: u32,
    // SSAO settings
    pub ssao_enabled: bool,
    pub ssao_radius: f32,
    pub ssao_bias: f32,
    pub ssao_power: f32,
    pub ssao_strength: f32,
    pub ssao_sample_count: u32,
    // LOD settings (Fase 24)
    /// Distance step per LOD level. LOD = floor(dist / step). 0 = LOD disabled (always LOD 0).
    pub lod_distance_step: f32,
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
    /// True while an async asset load is in progress (Fase 25).
    pub is_loading: bool,
}

/// Per-channel volume controls for vehicle audio (Fase 30).
pub struct VehicleAudioUiState {
    /// Engine loop volume (0..1).
    pub engine_volume: f32,
    /// Tyre skid volume (0..1).
    pub skid_volume:   f32,
    /// Wind/turbulence volume ceiling (0..1).
    pub wind_volume:   f32,
    /// One-shot impact effects volume (0..1).
    pub effects_volume: f32,
}

/// Day/night cycle controls exposed to the egui panel.
/// Acts as the authoritative state for the cycle: the panel edits it directly,
/// and the main loop's day/night system reads from it every frame.
pub struct DayNightUiState {
    /// Current time of day: 0.0 = noon, 0.25 = sunset, 0.5 = midnight, 0.75 = sunrise.
    pub time_of_day: f32,
    /// When true, `time_of_day` advances automatically each frame.
    pub auto_cycle: bool,
    /// Seconds for one full cycle (default 180 = 3 minutes).
    pub cycle_duration: f32,
}

/// State shared between the scripting panel and the main loop.
pub struct ScriptingUiState {
    /// List of (path, enabled, last_error) for each loaded script.
    pub scripts: Vec<(String, bool, Option<String>)>,
    /// Recent log messages emitted by scripts (capped at 50).
    pub log_lines: Vec<String>,
}

/// Game mode (exploration sandbox).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameStateKind {
    #[default]
    Exploring,
    Paused,
}

/// One-frame actions emitted by the pause overlay.
#[derive(Debug, Default)]
pub struct GameAction {
    /// Resume from Paused → Exploring.
    pub resume: bool,
    /// Exit the application.
    pub quit:   bool,
}

/// State shared between the HUD / overlay panels and the main loop.
pub struct GameHudState {
    pub kind:      GameStateKind,
    /// Current speed in km/h (from Vehicle simulation).
    pub speed_kmh: f32,
    /// Current engine RPM.
    pub rpm:       f32,
    /// Maximum engine RPM (for gauge scaling).
    pub max_rpm:   f32,
    /// One-frame actions set by the overlay panel.
    pub action:    GameAction,
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
        scripting: &ScriptingUiState,
        day_night: &mut DayNightUiState,
        vehicle_audio: &mut VehicleAudioUiState,
        game_hud: &mut GameHudState,
    ) -> (Vec<egui::ClippedPrimitive>, egui::TexturesDelta) {
        let raw_input = self.winit_state.take_egui_input(window);
        let output = self.ctx.run(raw_input, |ctx| {
            panels::sidebar(
                ctx, stats, world, settings, scene, physics, audio,
                editor, scripting, day_night, vehicle_audio, game_hud,
            );
        });
        self.winit_state.handle_platform_output(window, output.platform_output);
        let clipped = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        (clipped, output.textures_delta)
    }

    pub fn pixels_per_point(&self) -> f32 {
        self.ctx.pixels_per_point()
    }
}
