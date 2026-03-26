use crate::scene::{DirectionalLight, PointLight, Transform};
use super::{AudioUiState, DebugSettings, FrameStats, PhysicsUiState, SceneUiState};

/// Stats panel: FPS, frame time, culling stats, shader reload log.
pub fn stats_panel(ctx: &egui::Context, stats: &FrameStats) {
    egui::Window::new("Stats")
        .default_pos([10.0, 10.0])
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!("FPS: {:.0}", stats.fps));
            ui.label(format!("Frame: {:.2} ms", stats.frame_ms));
            let culled = stats.total_entities.saturating_sub(stats.draw_calls);
            ui.label(format!(
                "Rendered: {}/{} ({} culled)",
                stats.draw_calls, stats.total_entities, culled
            ));
            if !stats.reload_log.is_empty() {
                ui.separator();
                for msg in &stats.reload_log {
                    let color = if msg.starts_with('✓') {
                        egui::Color32::from_rgb(100, 220, 100)
                    } else {
                        egui::Color32::from_rgb(255, 100, 100)
                    };
                    ui.colored_label(color, msg);
                }
            }
        });
}

/// Settings panel: MSAA toggle, tone mapping toggle.
pub fn settings_panel(ctx: &egui::Context, settings: &mut DebugSettings) {
    egui::Window::new("Settings")
        .default_pos([10.0, 400.0])
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Tone mapping");
            ui.horizontal(|ui| {
                ui.radio_value(&mut settings.tone_aces, false, "Reinhard");
                ui.radio_value(&mut settings.tone_aces, true, "ACES");
            });

            ui.separator();
            ui.label("MSAA");
            ui.horizontal(|ui| {
                ui.radio_value(&mut settings.msaa_samples, 1, "Off");
                if settings.msaa_max >= 2 {
                    ui.radio_value(&mut settings.msaa_samples, 2, "2×");
                }
                if settings.msaa_max >= 4 {
                    ui.radio_value(&mut settings.msaa_samples, 4, "4×");
                }
            });
        });
}

/// Scene panel: Save / Load buttons and status.
pub fn scene_panel(ctx: &egui::Context, state: &mut SceneUiState) {
    egui::Window::new("Scene")
        .default_pos([10.0, 560.0])
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!("Models: {}  |  Entities: {}", state.model_count, state.entity_count));
            ui.horizontal(|ui| {
                if ui.button("Save Scene").clicked() {
                    state.save_clicked = true;
                }
                if ui.button("Load Scene").clicked() {
                    state.load_clicked = true;
                }
            });
            if !state.status.is_empty() {
                let color = if state.status.starts_with('✓') {
                    egui::Color32::from_rgb(100, 220, 100)
                } else {
                    egui::Color32::from_rgb(255, 130, 100)
                };
                ui.colored_label(color, &state.status);
            }
        });
}

/// Physics panel: spawn physics objects and toggle debug wireframes.
pub fn physics_panel(ctx: &egui::Context, state: &mut PhysicsUiState) {
    egui::Window::new("Physics")
        .default_pos([10.0, 680.0])
        .resizable(false)
        .show(ctx, |ui| {
            if ui.button("Spawn Physics Cube").clicked() {
                state.spawn_cube_clicked = true;
            }
            ui.separator();
            ui.checkbox(&mut state.show_wireframe, "Show Collider Wireframes");
        });
}

/// Audio panel: master volume slider and mute toggle.
pub fn audio_panel(ctx: &egui::Context, state: &mut AudioUiState) {
    egui::Window::new("Audio")
        .default_pos([10.0, 780.0])
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Volume");
                ui.add(egui::Slider::new(&mut state.master_volume, 0.0f32..=1.0).show_value(false));
                ui.label(format!("{:.0}%", state.master_volume * 100.0));
            });
            ui.checkbox(&mut state.muted, "Mute");
        });
}

/// Lights panel: sliders for directional and point lights.
/// Mutations go directly into ECS components — changes are visible next frame.
pub fn lights_panel(ctx: &egui::Context, world: &mut hecs::World) {
    egui::Window::new("Lights")
        .default_pos([10.0, 120.0])
        .show(ctx, |ui| {
            // Directional light
            ui.collapsing("Directional", |ui| {
                for (_, light) in world.query_mut::<&mut DirectionalLight>() {
                    let mut dir = [light.direction.x, light.direction.y, light.direction.z];
                    ui.label("Direction");
                    if ui.add(egui::Slider::new(&mut dir[0], -1.0..=1.0).text("X")).changed()
                        | ui.add(egui::Slider::new(&mut dir[1], -1.0..=1.0).text("Y")).changed()
                        | ui.add(egui::Slider::new(&mut dir[2], -1.0..=1.0).text("Z")).changed()
                    {
                        let v = glam::Vec3::from(dir);
                        if v.length_squared() > 0.0001 {
                            light.direction = v.normalize();
                        }
                    }

                    let mut color = [light.color.x, light.color.y, light.color.z];
                    if ui.color_edit_button_rgb(&mut color).changed() {
                        light.color = glam::Vec3::from(color);
                    }
                    ui.add(egui::Slider::new(&mut light.intensity, 0.0..=5.0).text("Intensity"));
                }
            });

            // Point light
            ui.collapsing("Point light", |ui| {
                for (_, (transform, light)) in
                    world.query_mut::<(&mut Transform, &mut PointLight)>()
                {
                    ui.label("Position");
                    ui.add(egui::Slider::new(&mut transform.position.x, -10.0..=10.0).text("X"));
                    ui.add(egui::Slider::new(&mut transform.position.y, -10.0..=10.0).text("Y"));
                    ui.add(egui::Slider::new(&mut transform.position.z, -10.0..=10.0).text("Z"));

                    let mut color = [light.color.x, light.color.y, light.color.z];
                    if ui.color_edit_button_rgb(&mut color).changed() {
                        light.color = glam::Vec3::from(color);
                    }
                    ui.add(egui::Slider::new(&mut light.intensity, 0.0..=20.0).text("Intensity"));
                    ui.add(egui::Slider::new(&mut light.radius, 0.1..=50.0).text("Radius"));
                }
            });
        });
}
