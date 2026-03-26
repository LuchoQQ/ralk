use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use glam::{Quat, Vec2, Vec3, Vec4};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

mod asset;
mod audio;
mod engine;
mod input;
mod physics;
mod scene;
mod ui;

use asset::{
    AudioSourceDef, ColliderDef, DirLightDef, EntityDef, PointLightDef, RigidBodyDef, SceneData,
    SceneFile, ShaderCompiler, load_multi_glb, load_scene_file, save_scene_file,
};
use audio::{AudioEngine, ensure_sample_sounds};
use physics::PhysicsWorld;
use ash::vk;
use engine::VulkanContext;
use input::InputState;
use scene::{
    AudioSource, BoundingBox, Camera3D, ColliderShapeType, DirectionalLight, LightingUbo,
    MeshRenderer, PhysicsBody, PhysicsBodyType, PhysicsCollider, PointLight, Transform,
    compute_light_mvp, extract_frustum_planes, is_aabb_visible, transform_aabb,
    // Phase 20 additions:
    GizmoAxis, GizmoDrag, GizmoMode,
    build_axis_groups, build_selection_group, drag_axis_dir,
    hit_test_gizmo, ray_aabb, screen_to_ray,
};
use ui::{AudioUiState, DebugSettings, DebugUi, EditorUiState, FrameStats, PhysicsUiState, SceneUiState};

const TICK_RATE: f32 = 1.0 / 60.0;
const GAMEPAD_DEAD_ZONE: f32 = 0.15;
const SCENE_PATH: &str = "scene.json";

struct App {
    window: Option<Arc<Window>>,
    vulkan: Option<VulkanContext>,
    scene_data: SceneData,
    /// Model paths currently loaded (mirrors SceneFile::models, used for save).
    model_paths: Vec<String>,
    camera: Camera3D,
    world: hecs::World,
    physics: PhysicsWorld,
    input: InputState,
    last_frame: Instant,
    mouse_captured: bool,
    debug_ui: Option<DebugUi>,
    shader_compiler: Option<ShaderCompiler>,
    gilrs: Option<gilrs::Gilrs>,
    reload_log: Vec<String>,
    accumulator: f32,
    frame_count: u32,
    fps_accum: f32,
    last_fps: f32,
    last_drawn: usize,
    last_total: usize,
    debug_settings: DebugSettings,
    scene_ui: SceneUiState,
    physics_ui: PhysicsUiState,
    audio_engine: Option<AudioEngine>,
    audio_ui: AudioUiState,
    /// Index of the builtin cube mesh (always the last mesh in scene_data after a load).
    cube_mesh_index: usize,
    // Phase 20: gizmo / object picking
    selected_entity: Option<hecs::Entity>,
    gizmo_mode: GizmoMode,
    gizmo_drag: Option<GizmoDrag>,
    hovered_gizmo_axis: Option<GizmoAxis>,
    mouse_pos: glam::Vec2,
    pending_pick: bool,
    window_size: (u32, u32),
    editor_ui: EditorUiState,
}

impl App {
    fn new() -> Self {
        // Generate placeholder audio assets on first run.
        ensure_sample_sounds();

        let (scene_data, model_paths) = initial_scene_load();
        let cube_mesh_index = scene_data.meshes.len().saturating_sub(1);
        let shader_compiler = ShaderCompiler::new("shaders")
            .map_err(|e| log::warn!("Shader hot-reload disabled: {e}"))
            .ok();
        let gilrs = gilrs::Gilrs::new()
            .map_err(|e| log::warn!("Gamepad support disabled: {e}"))
            .ok();
        if let Some(ref g) = gilrs {
            for (_id, gamepad) in g.gamepads() {
                log::info!("Gamepad connected: {}", gamepad.name());
            }
        }

        let mut physics = PhysicsWorld::new();

        // Static floor: large flat box at y = -1.0 (centered below origin).
        physics.add_static_box(Vec3::new(0.0, -1.0, 0.0), Vec3::new(20.0, 0.5, 20.0), 0.5, 0.8);

        Self {
            window: None,
            vulkan: None,
            scene_data,
            model_paths,
            cube_mesh_index,
            camera: Camera3D::new(1280.0 / 720.0),
            world: hecs::World::new(),
            physics,
            input: InputState::new(),
            last_frame: Instant::now(),
            mouse_captured: false,
            debug_ui: None,
            shader_compiler,
            gilrs,
            reload_log: Vec::new(),
            accumulator: 0.0,
            frame_count: 0,
            fps_accum: 0.0,
            last_fps: 0.0,
            last_drawn: 0,
            last_total: 0,
            debug_settings: DebugSettings { tone_aces: false, msaa_samples: 4, msaa_max: 4 },
            scene_ui: SceneUiState {
                save_clicked: false,
                load_clicked: false,
                status: String::new(),
                model_count: 0,
                entity_count: 0,
            },
            physics_ui: PhysicsUiState {
                spawn_cube_clicked: false,
                show_wireframe: false,
            },
            audio_engine: AudioEngine::new(),
            audio_ui: AudioUiState { master_volume: 1.0, muted: false },
            selected_entity: None,
            gizmo_mode: GizmoMode::Translate,
            gizmo_drag: None,
            hovered_gizmo_axis: None,
            mouse_pos: glam::Vec2::ZERO,
            pending_pick: false,
            window_size: (1280, 720),
            editor_ui: EditorUiState {
                selected_entity: None,
                position: [0.0; 3],
                rotation_euler_deg: [0.0; 3],
                scale: [1.0, 1.0, 1.0],
                gizmo_mode: 0,
                transform_changed: false,
            },
        }
    }

    fn capture_mouse(&mut self) {
        let Some(window) = &self.window else { return };
        let grabbed = window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
            .is_ok();
        if grabbed {
            window.set_cursor_visible(false);
            self.mouse_captured = true;
        }
    }

    fn release_mouse(&mut self) {
        let Some(window) = &self.window else { return };
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
        self.mouse_captured = false;
    }

    /// Spawn entities from a `SceneFile`. Requires GPU already initialized with
    /// matching `scene_data` (same models in the same order).
    fn spawn_from_scene_file(&mut self, sf: &SceneFile) {
        let mesh_count = self.scene_data.meshes.len();
        let mat_count = self.scene_data.materials.len();
        let default_mat = mat_count; // last descriptor set is the default

        for ent in &sf.entities {
            let mesh_idx = ent.mesh_index.min(mesh_count.saturating_sub(1));
            let mat_idx = ent.material_set_index.min(default_mat);
            let Some(mesh_data) = self.scene_data.meshes.get(mesh_idx) else { continue };

            let position = Vec3::from(ent.position);
            let rotation = Quat::from_array(ent.rotation);
            let transform = Transform {
                position,
                rotation,
                scale: Vec3::from(ent.scale),
            };
            let bbox = BoundingBox { min: mesh_data.aabb_min, max: mesh_data.aabb_max };

            // If the entity has physics data, attach rigid body + collider.
            if let (Some(rb_def), Some(col_def)) = (&ent.rigid_body, &ent.collider) {
                let half_extents = Vec3::from(col_def.half_extents);
                let (body_handle, collider_handle) = match rb_def.body_type.as_str() {
                    "dynamic" => self.physics.add_dynamic_box(
                        position,
                        half_extents,
                        col_def.restitution,
                        col_def.friction,
                    ),
                    "static" => self.physics.add_static_box(
                        position,
                        half_extents,
                        col_def.restitution,
                        col_def.friction,
                    ),
                    _ => self.physics.add_dynamic_box(
                        position,
                        half_extents,
                        col_def.restitution,
                        col_def.friction,
                    ),
                };
                let body_type = if rb_def.body_type == "static" {
                    PhysicsBodyType::Static
                } else {
                    PhysicsBodyType::Dynamic
                };
                if let Some(aud_def) = &ent.audio_source {
                    self.world.spawn((
                        transform,
                        MeshRenderer { mesh_index: mesh_idx, material_set_index: mat_idx },
                        bbox,
                        PhysicsBody { handle: body_handle, body_type },
                        PhysicsCollider {
                            handle: collider_handle,
                            shape: ColliderShapeType::Box,
                            half_extents,
                        },
                        AudioSource {
                            sound_path: aud_def.sound_path.clone(),
                            volume: aud_def.volume,
                            looping: aud_def.looping,
                            max_distance: aud_def.max_distance,
                            handle: None,
                        },
                    ));
                } else {
                    self.world.spawn((
                        transform,
                        MeshRenderer { mesh_index: mesh_idx, material_set_index: mat_idx },
                        bbox,
                        PhysicsBody { handle: body_handle, body_type },
                        PhysicsCollider {
                            handle: collider_handle,
                            shape: ColliderShapeType::Box,
                            half_extents,
                        },
                    ));
                }
            } else if let Some(aud_def) = &ent.audio_source {
                self.world.spawn((
                    transform,
                    MeshRenderer { mesh_index: mesh_idx, material_set_index: mat_idx },
                    bbox,
                    AudioSource {
                        sound_path: aud_def.sound_path.clone(),
                        volume: aud_def.volume,
                        looping: aud_def.looping,
                        max_distance: aud_def.max_distance,
                        handle: None,
                    },
                ));
            } else {
                self.world.spawn((
                    transform,
                    MeshRenderer { mesh_index: mesh_idx, material_set_index: mat_idx },
                    bbox,
                ));
            }
        }

        for light in &sf.point_lights {
            self.world.spawn((
                Transform::from_position(Vec3::from(light.position)),
                PointLight {
                    color: Vec3::from(light.color),
                    intensity: light.intensity,
                    radius: light.radius,
                },
            ));
        }

        let dl = &sf.directional_light;
        self.world.spawn((
            Transform::from_position(Vec3::ZERO),
            DirectionalLight {
                direction: Vec3::from(dl.direction).normalize(),
                color: Vec3::from(dl.color),
                intensity: dl.intensity,
            },
        ));
    }

    /// Fallback: spawn entities directly from `scene_data` (no scene.json).
    /// Mirrors the old M2 behavior: 3 copies of each mesh at X offsets.
    fn spawn_scene_default(&mut self) {
        let default_mat_idx = self.scene_data.materials.len();

        for x_offset in [-3.0f32, 0.0, 3.0] {
            let translation = glam::Mat4::from_translation(Vec3::new(x_offset, 0.0, 0.0));
            for (mesh_idx, mesh_data) in self.scene_data.meshes.iter().enumerate() {
                let mat_set_idx = mesh_data.material_index.unwrap_or(default_mat_idx);
                let transform = Transform::from_matrix(translation * mesh_data.transform);
                let bbox = BoundingBox { min: mesh_data.aabb_min, max: mesh_data.aabb_max };
                self.world.spawn((
                    transform,
                    MeshRenderer { mesh_index: mesh_idx, material_set_index: mat_set_idx },
                    bbox,
                ));
            }
        }

        self.world.spawn((
            Transform::from_position(Vec3::ZERO),
            DirectionalLight {
                direction: Vec3::new(0.4, -1.0, -0.6).normalize(),
                color: Vec3::ONE,
                intensity: 1.2,
            },
        ));

        self.world.spawn((
            Transform::from_position(Vec3::new(2.0, 2.0, 2.0)),
            PointLight { color: Vec3::new(1.0, 0.9, 0.7), intensity: 3.0, radius: 10.0 },
        ));
    }

    /// Save the current world state to `scene.json`.
    fn save_scene(&self) -> Result<()> {
        let mut entities: Vec<EntityDef> = Vec::new();
        for (entity, (transform, mr)) in self.world.query::<(&Transform, &MeshRenderer)>().iter() {
            // If entity has physics components, persist them.
            let rigid_body = self.world.get::<&PhysicsBody>(entity).ok().map(|pb| RigidBodyDef {
                body_type: match pb.body_type {
                    PhysicsBodyType::Dynamic => "dynamic".to_string(),
                    PhysicsBodyType::Static => "static".to_string(),
                    PhysicsBodyType::Kinematic => "kinematic".to_string(),
                },
            });
            let collider = self.world.get::<&PhysicsCollider>(entity).ok().map(|pc| ColliderDef {
                half_extents: pc.half_extents.to_array(),
                restitution: 0.4,
                friction: 0.5,
            });
            let audio_source = self.world.get::<&AudioSource>(entity).ok().map(|a| AudioSourceDef {
                sound_path: a.sound_path.clone(),
                volume: a.volume,
                looping: a.looping,
                max_distance: a.max_distance,
            });
            entities.push(EntityDef {
                mesh_index: mr.mesh_index,
                material_set_index: mr.material_set_index,
                position: transform.position.to_array(),
                rotation: transform.rotation.to_array(),
                scale: transform.scale.to_array(),
                rigid_body,
                collider,
                audio_source,
            });
        }

        let mut dir_light = DirLightDef {
            direction: [0.4, -1.0, -0.6],
            color: [1.0, 1.0, 1.0],
            intensity: 1.2,
        };
        for (_, light) in self.world.query::<&DirectionalLight>().iter() {
            dir_light = DirLightDef {
                direction: light.direction.to_array(),
                color: light.color.to_array(),
                intensity: light.intensity,
            };
        }

        let mut point_lights: Vec<PointLightDef> = Vec::new();
        for (_, (transform, light)) in self.world.query::<(&Transform, &PointLight)>().iter() {
            point_lights.push(PointLightDef {
                position: transform.position.to_array(),
                color: light.color.to_array(),
                intensity: light.intensity,
                radius: light.radius,
            });
        }

        let sf = SceneFile {
            models: self.model_paths.clone(),
            entities,
            directional_light: dir_light,
            point_lights,
        };
        save_scene_file(SCENE_PATH, &sf)
    }

    /// Load `scene.json`, reload GPU resources, and respawn the ECS world.
    fn load_scene(&mut self) -> Result<()> {
        let sf = load_scene_file(SCENE_PATH)?;
        let (new_scene, _offsets) = load_multi_glb(&sf.models)?;

        if let Some(ref mut vulkan) = self.vulkan {
            vulkan.reload_scene(&new_scene)?;
        }

        self.world = hecs::World::new();
        self.physics = PhysicsWorld::new();
        // Restore the static floor after physics reset.
        self.physics.add_static_box(Vec3::new(0.0, -1.0, 0.0), Vec3::new(20.0, 0.5, 20.0), 0.5, 0.8);

        self.cube_mesh_index = new_scene.meshes.len().saturating_sub(1);
        self.model_paths = sf.models.clone();
        self.scene_data = new_scene;
        self.spawn_from_scene_file(&sf);

        log::info!(
            "Loaded scene: {} models, {} entities",
            sf.models.len(),
            sf.entities.len(),
        );
        Ok(())
    }

    /// Spawn a physics cube in front of the camera.
    fn spawn_physics_cube(&mut self) {
        let spawn_pos = self.camera.position + self.camera.forward() * 3.0 + Vec3::Y * 1.0;
        let half_extents = Vec3::splat(0.5);
        let (body_handle, collider_handle) =
            self.physics.add_dynamic_box(spawn_pos, half_extents, 0.4, 0.5);

        let transform = Transform::from_position(spawn_pos);
        let bbox = BoundingBox { min: -half_extents, max: half_extents };
        self.world.spawn((
            transform,
            MeshRenderer {
                mesh_index: self.cube_mesh_index,
                material_set_index: self.scene_data.materials.len(), // default material
            },
            bbox,
            PhysicsBody { handle: body_handle, body_type: PhysicsBodyType::Dynamic },
            PhysicsCollider {
                handle: collider_handle,
                shape: ColliderShapeType::Box,
                half_extents,
            },
        ));
        log::info!("Spawned physics cube at {:?}", spawn_pos);
    }

    /// Build a list of line segment vertices representing all box colliders in the world.
    fn build_wireframe_lines(&self) -> Vec<Vec3> {
        let mut lines = Vec::new();
        for (_, (transform, col)) in
            self.world.query::<(&Transform, &PhysicsCollider)>().iter()
        {
            if col.shape != ColliderShapeType::Box {
                continue;
            }
            let hx = col.half_extents.x;
            let hy = col.half_extents.y;
            let hz = col.half_extents.z;

            // 8 corners in local space.
            let local = [
                Vec3::new(-hx, -hy, -hz), Vec3::new( hx, -hy, -hz),
                Vec3::new( hx,  hy, -hz), Vec3::new(-hx,  hy, -hz),
                Vec3::new(-hx, -hy,  hz), Vec3::new( hx, -hy,  hz),
                Vec3::new( hx,  hy,  hz), Vec3::new(-hx,  hy,  hz),
            ];

            // Transform to world space (position + rotation, ignore scale for colliders).
            let world: Vec<Vec3> = local
                .iter()
                .map(|&c| transform.position + transform.rotation * c)
                .collect();

            // 12 edges of a box.
            let edges = [
                (0, 1), (1, 2), (2, 3), (3, 0), // bottom face
                (4, 5), (5, 6), (6, 7), (7, 4), // top face
                (0, 4), (1, 5), (2, 6), (3, 7), // vertical edges
            ];
            for (a, b) in &edges {
                lines.push(world[*a]);
                lines.push(world[*b]);
            }
        }
        lines
    }

    fn build_lighting_ubo(&self) -> LightingUbo {
        let mut dir_dir = Vec3::new(0.4, -1.0, -0.6).normalize();
        let mut dir_color = Vec3::ONE;
        let mut dir_intensity = 1.2f32;
        let mut pt_pos = Vec3::new(2.0, 2.0, 2.0);
        let mut pt_color = Vec3::new(1.0, 0.9, 0.7);
        let mut pt_intensity = 3.0f32;

        for (_, light) in self.world.query::<&DirectionalLight>().iter() {
            dir_dir = light.direction;
            dir_color = light.color;
            dir_intensity = light.intensity;
        }
        for (_, (transform, light)) in self.world.query::<(&Transform, &PointLight)>().iter() {
            pt_pos = transform.position;
            pt_color = light.color;
            pt_intensity = light.intensity;
        }

        let light_view_proj = compute_light_mvp(dir_dir);
        let tone_mode = if self.debug_settings.tone_aces { 1.0_f32 } else { 0.0_f32 };
        LightingUbo {
            dir_light_dir:     dir_dir.extend(0.0).into(),
            dir_light_color:   Vec4::from((dir_color, dir_intensity)).into(),
            point_light_pos:   pt_pos.extend(0.0).into(),
            point_light_color: Vec4::from((pt_color, pt_intensity)).into(),
            camera_pos:        self.camera.position.extend(tone_mode).into(),
            light_mvp:         light_view_proj.to_cols_array(),
        }
    }

    fn build_draw_list(
        &mut self,
        view_proj: glam::Mat4,
    ) -> (Vec<(glam::Mat4, usize, usize)>, usize, usize) {
        let planes = extract_frustum_planes(view_proj);
        let mut list = Vec::new();
        let mut total = 0usize;

        for (_, (transform, mr, bbox)) in
            self.world.query::<(&Transform, &MeshRenderer, &BoundingBox)>().iter()
        {
            total += 1;
            let world_mat = transform.to_mat4();
            let (world_min, world_max) = transform_aabb(bbox.min, bbox.max, world_mat);
            if is_aabb_visible(world_min, world_max, &planes) {
                list.push((world_mat, mr.mesh_index, mr.material_set_index));
            }
        }

        let drawn = list.len();
        (list, drawn, total)
    }

    fn poll_gamepad(&mut self) {
        let Some(ref mut gilrs) = self.gilrs else { return };

        while let Some(event) = gilrs.next_event() {
            gilrs.update(&event);
            match event.event {
                gilrs::EventType::Connected => {
                    log::info!("Gamepad connected: {}", gilrs.gamepad(event.id).name());
                }
                gilrs::EventType::Disconnected => {
                    log::info!("Gamepad disconnected");
                }
                _ => {}
            }
        }

        let mut move_xy = Vec2::ZERO;
        let mut look_xy = Vec2::ZERO;
        for (_id, gamepad) in gilrs.gamepads() {
            let lx = gamepad.axis_data(gilrs::Axis::LeftStickX).map(|a| a.value()).unwrap_or(0.0);
            let ly = gamepad.axis_data(gilrs::Axis::LeftStickY).map(|a| a.value()).unwrap_or(0.0);
            let rx = gamepad.axis_data(gilrs::Axis::RightStickX).map(|a| a.value()).unwrap_or(0.0);
            let ry = gamepad.axis_data(gilrs::Axis::RightStickY).map(|a| a.value()).unwrap_or(0.0);

            move_xy.x = if lx.abs() > GAMEPAD_DEAD_ZONE { lx } else { 0.0 };
            move_xy.y = if ly.abs() > GAMEPAD_DEAD_ZONE { ly } else { 0.0 };
            look_xy.x = if rx.abs() > GAMEPAD_DEAD_ZONE { rx } else { 0.0 };
            look_xy.y = if ry.abs() > GAMEPAD_DEAD_ZONE { ry } else { 0.0 };
            break;
        }

        self.input.gamepad_move = move_xy;
        self.input.gamepad_look = look_xy;
    }
}

/// Load scene from `scene.json` if it exists, otherwise fall back to single-model heuristic.
/// Returns `(scene_data, model_paths)`.
fn initial_scene_load() -> (SceneData, Vec<String>) {
    if let Ok(sf) = load_scene_file(SCENE_PATH) {
        log::info!("Loading scene from '{SCENE_PATH}' ({} model(s))", sf.models.len());
        match load_multi_glb(&sf.models) {
            Ok((scene, _)) => return (scene, sf.models),
            Err(e) => log::warn!("Failed to load models from scene file: {e}"),
        }
    }

    // Fallback: try known model paths, use builtin cube if nothing found.
    let candidates = [
        "assets/DamagedHelmet.glb",
        "assets/Sponza.glb",
        "assets/Box.glb",
    ];
    for path in &candidates {
        match asset::load_glb(path) {
            Ok(scene) => {
                log::info!("Loaded fallback model: {path}");
                return (scene, vec![path.to_string()]);
            }
            Err(e) => log::debug!("Skipping {path}: {e}"),
        }
    }

    log::info!("No glTF model found in assets/ — using builtin cube");
    (asset::builtin_cube(), vec![])
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("ralk")
            .with_inner_size(LogicalSize::new(1280u32, 720u32));

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("Failed to create window: {e}");
                event_loop.exit();
                return;
            }
        };

        match VulkanContext::new(&window, &self.scene_data) {
            Ok(vulkan) => {
                self.debug_settings.msaa_samples = vulkan.msaa_samples().as_raw();
                self.debug_settings.msaa_max = vulkan.msaa_max().as_raw();
                self.vulkan = Some(vulkan);
                self.debug_ui = Some(DebugUi::new(&window));
                self.window = Some(window);

                // Spawn initial scene: from scene.json if it exists, else fallback.
                if let Ok(sf) = load_scene_file(SCENE_PATH) {
                    self.spawn_from_scene_file(&sf);
                } else {
                    self.spawn_scene_default();
                }

                // Ambient audio source (loops throughout the session).
                self.world.spawn((
                    Transform::from_position(Vec3::ZERO),
                    AudioSource {
                        sound_path: "assets/sounds/ambient.wav".to_string(),
                        volume: 0.3,
                        looping: true,
                        max_distance: 50.0,
                        handle: None,
                    },
                ));

                self.scene_ui.model_count = self.model_paths.len();
                self.scene_ui.entity_count = self.world.len() as usize;

                self.capture_mouse();
            }
            Err(e) => {
                log::error!("Failed to initialize Vulkan: {e:#}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let egui_consumed = if let (Some(ui), Some(window)) = (&mut self.debug_ui, &self.window) {
            ui.on_window_event(window, &event)
        } else {
            false
        };

        if let Some(ui) = &self.debug_ui {
            self.input.set_captured(ui.ctx.wants_keyboard_input() || ui.ctx.wants_pointer_input());
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let Some(vulkan) = &mut self.vulkan {
                    vulkan.framebuffer_resized = true;
                }
                if size.width > 0 && size.height > 0 {
                    self.camera.aspect = size.width as f32 / size.height as f32;
                    self.window_size = (size.width, size.height);
                }
            }

            WindowEvent::KeyboardInput { event, .. } if !egui_consumed => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;
                    match code {
                        KeyCode::KeyW if self.mouse_captured => self.input.forward = pressed,
                        KeyCode::KeyW if !self.mouse_captured && pressed => self.gizmo_mode = GizmoMode::Translate,
                        KeyCode::KeyS if self.mouse_captured => self.input.backward = pressed,
                        KeyCode::KeyA if self.mouse_captured => self.input.left = pressed,
                        KeyCode::KeyD if self.mouse_captured => self.input.right = pressed,
                        KeyCode::KeyE if !self.mouse_captured && pressed => self.gizmo_mode = GizmoMode::Rotate,
                        KeyCode::KeyR if !self.mouse_captured && pressed => self.gizmo_mode = GizmoMode::Scale,
                        KeyCode::ShiftLeft | KeyCode::ShiftRight => self.input.sprint = pressed,
                        KeyCode::Escape if pressed => self.release_mouse(),
                        _ => {}
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                let new_pos = glam::Vec2::new(position.x as f32, position.y as f32);
                let delta = new_pos - self.mouse_pos;
                self.mouse_pos = new_pos;

                // Apply gizmo drag if active.
                if let Some(ref drag) = self.gizmo_drag {
                    let projected = delta.dot(drag.axis_screen_dir);
                    if drag.pixels_per_unit > 0.0 {
                        let movement = projected / drag.pixels_per_unit;
                        let axis_dir = drag_axis_dir(drag.axis);
                        if let Some(entity) = self.selected_entity {
                            match drag.mode {
                                GizmoMode::Translate => {
                                    if let Ok(mut transform) = self.world.get::<&mut Transform>(entity) {
                                        transform.position += axis_dir * movement;
                                    }
                                }
                                GizmoMode::Scale => {
                                    if let Ok(mut transform) = self.world.get::<&mut Transform>(entity) {
                                        let idx = match drag.axis { GizmoAxis::X => 0, GizmoAxis::Y => 1, GizmoAxis::Z => 2 };
                                        transform.scale[idx] = (transform.scale[idx] + movement).max(0.01);
                                    }
                                }
                                GizmoMode::Rotate => {
                                    // Map mouse delta to angle: projected pixels / (pixels for 2π)
                                    let angle = projected / drag.pixels_per_unit.max(1.0) * 0.05;
                                    if let Ok(mut transform) = self.world.get::<&mut Transform>(entity) {
                                        let q = glam::Quat::from_axis_angle(axis_dir, angle);
                                        transform.rotation = (q * transform.rotation).normalize();
                                    }
                                }
                            }
                        }
                    }
                }

                // Update hovered gizmo axis when not dragging and not mouse-captured.
                if self.gizmo_drag.is_none() && !self.mouse_captured {
                    if let Some(entity) = self.selected_entity {
                        if let Ok(transform) = self.world.get::<&Transform>(entity) {
                            let vp = self.camera.view_proj();
                            let ws = self.window_size;
                            let screen = glam::Vec2::new(ws.0 as f32, ws.1 as f32);
                            self.hovered_gizmo_axis = hit_test_gizmo(
                                self.mouse_pos,
                                transform.position,
                                self.gizmo_mode,
                                vp,
                                screen,
                                14.0,
                            ).map(|(ax, ..)| ax);
                        } else {
                            self.hovered_gizmo_axis = None;
                        }
                    } else {
                        self.hovered_gizmo_axis = None;
                    }
                }
            }

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if !egui_consumed => {
                if !self.mouse_captured {
                    if self.hovered_gizmo_axis.is_some() {
                        // Start gizmo drag.
                        if let Some(entity) = self.selected_entity {
                            if let Ok(transform) = self.world.get::<&Transform>(entity) {
                                let vp = self.camera.view_proj();
                                let ws = self.window_size;
                                let screen = glam::Vec2::new(ws.0 as f32, ws.1 as f32);
                                if let Some((ax, dir, ppu)) = hit_test_gizmo(
                                    self.mouse_pos, transform.position, self.gizmo_mode, vp, screen, 14.0,
                                ) {
                                    self.gizmo_drag = Some(GizmoDrag {
                                        axis: ax, mode: self.gizmo_mode,
                                        axis_screen_dir: dir, pixels_per_unit: ppu,
                                    });
                                }
                            }
                        }
                    } else {
                        // Queue a pick.
                        self.pending_pick = true;
                    }
                }
            }

            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                self.gizmo_drag = None;
            }

            WindowEvent::Focused(false) => {
                self.release_mouse();
                self.input = InputState::new();
            }

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;

                self.fps_accum += dt;
                self.frame_count += 1;
                if self.fps_accum >= 1.0 {
                    self.last_fps = self.frame_count as f32 / self.fps_accum;
                    self.fps_accum = 0.0;
                    self.frame_count = 0;
                }

                // Shader hot-reload.
                if let Some(ref mut sc) = self.shader_compiler {
                    if let Some((target, vert_spv, frag_spv)) = sc.check_changes() {
                        if let Some(ref mut vulkan) = self.vulkan {
                            match vulkan.recreate_pipeline(target, &vert_spv, &frag_spv) {
                                Ok(()) => {
                                    self.reload_log.push(format!("✓ Reloaded: {:?}", target));
                                }
                                Err(e) => {
                                    let msg = format!("✗ Pipeline: {e}");
                                    log::error!("{msg}");
                                    self.reload_log.push(msg);
                                }
                            }
                        }
                    }
                    for err in sc.errors.drain(..) {
                        self.reload_log.push(err);
                    }
                    if self.reload_log.len() > 5 {
                        self.reload_log.drain(0..self.reload_log.len() - 5);
                    }
                }

                self.poll_gamepad();

                self.accumulator += dt;
                let mut all_impacts: Vec<Vec3> = Vec::new();
                while self.accumulator >= TICK_RATE {
                    self.camera.update(&self.input, TICK_RATE);

                    // Sync kinematic ECS transforms → rapier before step.
                    for (_, (transform, body)) in
                        self.world.query::<(&Transform, &PhysicsBody)>().iter()
                    {
                        if body.body_type == PhysicsBodyType::Kinematic {
                            self.physics.set_kinematic_pose(
                                body.handle,
                                transform.position,
                                transform.rotation,
                            );
                        }
                    }

                    let step_impacts = self.physics.step_and_collect_impacts(TICK_RATE);
                    all_impacts.extend(step_impacts);

                    // Sync dynamic rapier bodies → ECS transforms after step.
                    for (_, (transform, body)) in
                        self.world.query::<(&mut Transform, &PhysicsBody)>().iter()
                    {
                        if body.body_type == PhysicsBodyType::Dynamic {
                            if let Some((pos, rot)) = self.physics.get_dynamic_pose(body.handle) {
                                transform.position = pos;
                                transform.rotation = rot;
                            }
                        }
                    }

                    self.accumulator -= TICK_RATE;
                }
                self.input.clear_frame_deltas();

                // Play impact sounds — volume attenuated by distance to camera.
                if let Some(ref mut engine) = self.audio_engine {
                    let cam_pos = self.camera.position;
                    for impact_pos in all_impacts {
                        engine.play_spatial(
                            "assets/sounds/impact.wav",
                            impact_pos,
                            cam_pos,
                            20.0,
                        );
                    }
                }

                // Audio source system: start looping sounds on first frame, update spatial volume.
                if let Some(ref mut engine) = self.audio_engine {
                    let cam_pos = self.camera.position;
                    for (_, (transform, audio)) in
                        self.world.query::<(&Transform, &mut AudioSource)>().iter()
                    {
                        if audio.handle.is_none() {
                            audio.handle = engine.play_sound(&audio.sound_path, audio.volume, audio.looping);
                        }
                        if let Some(handle) = audio.handle {
                            let dist = (transform.position - cam_pos).length();
                            let vol = (1.0 - dist / audio.max_distance).clamp(0.0, 1.0) * audio.volume;
                            engine.set_volume(handle, vol);
                        }
                    }
                }

                // Sync UI audio settings → engine.
                if let Some(ref mut engine) = self.audio_engine {
                    engine.set_master_volume(self.audio_ui.master_volume);
                    engine.set_muted(self.audio_ui.muted);
                }

                let view_proj = self.camera.view_proj();
                let lighting_ubo = self.build_lighting_ubo();
                let (draw_list, drawn, total) = self.build_draw_list(view_proj);
                self.last_drawn = drawn;
                self.last_total = total;

                // Object picking (ray-AABB) — executed when user clicked in editor mode.
                if self.pending_pick {
                    self.pending_pick = false;
                    let ws = self.window_size;
                    let (ray_o, ray_d) = screen_to_ray(self.mouse_pos, ws, &self.camera);
                    let mut nearest: Option<(f32, hecs::Entity)> = None;
                    for (entity, (transform, bbox)) in
                        self.world.query::<(&Transform, &BoundingBox)>().iter()
                    {
                        let world_mat = transform.to_mat4();
                        let (wmin, wmax) = transform_aabb(bbox.min, bbox.max, world_mat);
                        if let Some(t) = ray_aabb(ray_o, ray_d, wmin, wmax) {
                            if nearest.as_ref().map(|(nt, _)| t < *nt).unwrap_or(true) {
                                nearest = Some((t, entity));
                            }
                        }
                    }
                    self.selected_entity = nearest.map(|(_, e)| e);
                    self.hovered_gizmo_axis = None;
                    // Click on empty space → recapture mouse (return to camera mode).
                    if self.selected_entity.is_none() {
                        self.capture_mouse();
                    }
                }

                // Apply transform edits from egui sliders (must run BEFORE ECS→UI sync,
                // so the user's edited values are written to ECS first, then read back).
                if self.editor_ui.transform_changed {
                    self.editor_ui.transform_changed = false;
                    if let Some(entity) = self.selected_entity {
                        if let Ok(mut transform) = self.world.get::<&mut Transform>(entity) {
                            transform.position = glam::Vec3::from(self.editor_ui.position);
                            let [rx, ry, rz] = self.editor_ui.rotation_euler_deg.map(f32::to_radians);
                            transform.rotation = glam::Quat::from_euler(glam::EulerRot::XYZ, rx, ry, rz);
                            transform.scale = glam::Vec3::from(self.editor_ui.scale);
                        }
                    }
                }

                // Sync selected entity state to editor UI (reads ECS after any pending edits).
                if let Some(entity) = self.selected_entity {
                    if let Ok(transform) = self.world.get::<&Transform>(entity) {
                        self.editor_ui.selected_entity = Some(entity);
                        self.editor_ui.position = transform.position.to_array();
                        let (ex, ey, ez) = transform.rotation.to_euler(glam::EulerRot::XYZ);
                        self.editor_ui.rotation_euler_deg = [
                            ex.to_degrees(), ey.to_degrees(), ez.to_degrees()
                        ];
                        self.editor_ui.scale = transform.scale.to_array();
                    }
                    // Sync gizmo mode from editor UI.
                    self.gizmo_mode = match self.editor_ui.gizmo_mode {
                        1 => GizmoMode::Rotate,
                        2 => GizmoMode::Scale,
                        _ => GizmoMode::Translate,
                    };
                    // Also keep editor_ui.gizmo_mode in sync if changed by keyboard.
                    self.editor_ui.gizmo_mode = match self.gizmo_mode {
                        GizmoMode::Translate => 0,
                        GizmoMode::Rotate    => 1,
                        GizmoMode::Scale     => 2,
                    };
                } else {
                    self.editor_ui.selected_entity = None;
                }

                // Build gizmo line groups for rendering.
                let (gizmo_verts, gizmo_groups): (Vec<glam::Vec3>, Vec<(u32, u32, [f32; 4])>) = {
                    let mut all_verts: Vec<glam::Vec3> = Vec::new();
                    let mut groups: Vec<(u32, u32, [f32; 4])> = Vec::new();
                    if let Some(entity) = self.selected_entity {
                        if let Ok(transform) = self.world.get::<&Transform>(entity) {
                            // Selection highlight box.
                            let sel_group = if let Ok(bbox) = self.world.get::<&BoundingBox>(entity) {
                                build_selection_group(transform.position, bbox.min, bbox.max)
                            } else {
                                build_selection_group(transform.position, glam::Vec3::splat(-0.5), glam::Vec3::splat(0.5))
                            };
                            let start = all_verts.len() as u32;
                            all_verts.extend_from_slice(&sel_group.vertices);
                            groups.push((start, sel_group.vertices.len() as u32, sel_group.color));
                            // Gizmo axes.
                            let vp = self.camera.view_proj();
                            let ws = self.window_size;
                            let screen = glam::Vec2::new(ws.0 as f32, ws.1 as f32);
                            // Update hovered axis (in case entity moved).
                            if self.gizmo_drag.is_none() && !self.mouse_captured {
                                self.hovered_gizmo_axis = hit_test_gizmo(
                                    self.mouse_pos, transform.position, self.gizmo_mode, vp, screen, 14.0,
                                ).map(|(ax, ..)| ax);
                            }
                            let axis_groups = build_axis_groups(transform.position, self.gizmo_mode, self.hovered_gizmo_axis);
                            for g in axis_groups {
                                let start = all_verts.len() as u32;
                                all_verts.extend_from_slice(&g.vertices);
                                groups.push((start, g.vertices.len() as u32, g.color));
                            }
                        }
                    }
                    (all_verts, groups)
                };

                // Reset per-frame action flags before building UI.
                self.scene_ui.save_clicked = false;
                self.scene_ui.load_clicked = false;
                self.scene_ui.model_count = self.model_paths.len();
                self.scene_ui.entity_count = total;
                self.physics_ui.spawn_cube_clicked = false;
                self.editor_ui.transform_changed = false;

                let prev_msaa = self.debug_settings.msaa_samples;
                let (egui_primitives, egui_textures_delta) =
                    if let (Some(ui), Some(window)) = (&mut self.debug_ui, &self.window) {
                        let stats = FrameStats {
                            fps: self.last_fps,
                            frame_ms: dt * 1000.0,
                            draw_calls: self.last_drawn,
                            total_entities: self.last_total,
                            reload_log: self.reload_log.clone(),
                        };
                        ui.build(window, &stats, &mut self.world, &mut self.debug_settings, &mut self.scene_ui, &mut self.physics_ui, &mut self.audio_ui, &mut self.editor_ui)
                    } else {
                        (vec![], egui::TexturesDelta::default())
                    };

                // Handle save (no GPU involvement — safe any time).
                let pending_save = self.scene_ui.save_clicked;
                // Defer load to after draw_frame so the current draw_list is not stale.
                let pending_load = self.scene_ui.load_clicked;

                // Spawn physics cube immediately (before draw, so it appears next frame).
                if self.physics_ui.spawn_cube_clicked {
                    self.spawn_physics_cube();
                }

                // Build wireframe line list for collider debug visualization.
                let wireframe_lines = if self.physics_ui.show_wireframe {
                    self.build_wireframe_lines()
                } else {
                    Vec::new()
                };
                let show_wireframe = self.physics_ui.show_wireframe;

                if pending_save {
                    match self.save_scene() {
                        Ok(()) => {
                            self.scene_ui.status = format!(
                                "✓ Saved ({} entities)",
                                self.world.query::<&MeshRenderer>().iter().count()
                            );
                        }
                        Err(e) => {
                            self.scene_ui.status = format!("✗ Save failed: {e}");
                            log::error!("Save scene failed: {e:#}");
                        }
                    }
                }

                // Apply MSAA change.
                if self.debug_settings.msaa_samples != prev_msaa {
                    if let Some(ref mut vulkan) = self.vulkan {
                        let new_samples =
                            vk::SampleCountFlags::from_raw(self.debug_settings.msaa_samples);
                        if let Err(e) = vulkan.set_msaa_samples(new_samples) {
                            log::error!("MSAA change failed: {e:#}");
                            self.debug_settings.msaa_samples = prev_msaa;
                        }
                    }
                }

                let egui_ppp =
                    self.debug_ui.as_ref().map(|u| u.pixels_per_point()).unwrap_or(1.0);

                if let (Some(vulkan), Some(window)) = (&mut self.vulkan, &self.window) {
                    if let Err(e) = vulkan.draw_frame(
                        window,
                        view_proj,
                        lighting_ubo,
                        &draw_list,
                        &wireframe_lines,
                        show_wireframe,
                        &gizmo_verts,
                        &gizmo_groups,
                        &egui_primitives,
                        egui_textures_delta,
                        egui_ppp,
                        true,  // bloom_enabled
                        0.8,   // bloom_intensity
                        1.0,   // bloom_threshold
                        self.debug_settings.tone_aces,
                    ) {
                        log::error!("Frame error: {e:#}");
                        event_loop.exit();
                    }
                }

                // Scene load: runs after draw_frame so the current draw_list isn't
                // invalidated mid-frame. Next frame will see the new GPU resources + world.
                if pending_load {
                    match self.load_scene() {
                        Ok(()) => {
                            self.scene_ui.status = format!(
                                "✓ Loaded ({} models, {} entities)",
                                self.model_paths.len(),
                                self.world.query::<&MeshRenderer>().iter().count(),
                            );
                        }
                        Err(e) => {
                            self.scene_ui.status = format!("✗ Load failed: {e}");
                            log::error!("Load scene failed: {e:#}");
                        }
                    }
                }
            }

            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if self.mouse_captured && !self.input.ui_captured {
            if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
                self.input.mouse_delta.x += dx as f32;
                self.input.mouse_delta.y += dy as f32;
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(mut vulkan) = self.vulkan.take() {
            vulkan.destroy();
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();

    let event_loop = EventLoop::new()?;
    let mut app = App::new();
    event_loop.run_app(&mut app)?;

    Ok(())
}
