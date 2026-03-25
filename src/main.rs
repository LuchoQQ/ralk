use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

mod asset;
mod engine;
mod input;
mod scene;

use asset::SceneData;
use engine::VulkanContext;
use input::InputState;
use scene::{Camera3D, LightingState};

struct App {
    window: Option<Arc<Window>>,
    vulkan: Option<VulkanContext>,
    scene_data: SceneData,
    camera: Camera3D,
    lights: LightingState,
    input: InputState,
    last_frame: Instant,
    mouse_captured: bool,
}

impl App {
    fn new() -> Self {
        // Try to load a glTF from assets/; fall back to builtin cube.
        let scene_data = load_scene();

        Self {
            window: None,
            vulkan: None,
            scene_data,
            camera: Camera3D::new(1280.0 / 720.0),
            lights: LightingState::default(),
            input: InputState::new(),
            last_frame: Instant::now(),
            mouse_captured: false,
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
}

fn load_scene() -> SceneData {
    // Paths to try, in order. First hit wins.
    let candidates = [
        "assets/DamagedHelmet.glb",
        "assets/Sponza.glb",
        "assets/Box.glb",
    ];

    for path in &candidates {
        match asset::load_glb(path) {
            Ok(scene) => return scene,
            Err(e) => log::debug!("Skipping {path}: {e}"),
        }
    }

    log::info!("No glTF model found in assets/ — using builtin cube");
    asset::builtin_cube()
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
                self.vulkan = Some(vulkan);
                self.window = Some(window);
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
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                if let Some(vulkan) = &mut self.vulkan {
                    vulkan.framebuffer_resized = true;
                }
                if size.width > 0 && size.height > 0 {
                    self.camera.aspect = size.width as f32 / size.height as f32;
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;
                    match code {
                        KeyCode::KeyW => self.input.forward = pressed,
                        KeyCode::KeyS => self.input.backward = pressed,
                        KeyCode::KeyA => self.input.left = pressed,
                        KeyCode::KeyD => self.input.right = pressed,
                        KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                            self.input.sprint = pressed
                        }
                        KeyCode::Escape if pressed => self.release_mouse(),
                        _ => {}
                    }
                }
            }

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if !self.mouse_captured {
                    self.capture_mouse();
                }
            }

            WindowEvent::Focused(false) => {
                self.release_mouse();
                self.input = InputState::new();
            }

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now.duration_since(self.last_frame).as_secs_f32();
                self.last_frame = now;

                self.camera.update(&self.input, dt);
                self.input.clear_frame_deltas();

                let view_proj = self.camera.view_proj();

                if let (Some(vulkan), Some(window)) = (&mut self.vulkan, &self.window) {
                    if let Err(e) = vulkan.draw_frame(window, view_proj, self.camera.position, &self.lights) {
                        log::error!("Frame error: {e:#}");
                        event_loop.exit();
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
        if self.mouse_captured {
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
