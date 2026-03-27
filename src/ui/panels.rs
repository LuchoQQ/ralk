use crate::scene::{DirectionalLight, PointLight, Transform};
use super::{AudioUiState, DayNightUiState, DebugSettings, EditorUiState, FrameStats,
            GameHudState, GameStateKind, PhysicsUiState, SceneUiState, ScriptingUiState,
            VehicleAudioUiState};

/// The only entry-point for the in-game UI.
///
/// - **Exploring**: renders nothing.
/// - **Paused**: renders a left sidebar with pause controls and all debug sections.
pub fn sidebar(
    ctx:          &egui::Context,
    stats:        &FrameStats,
    world:        &mut hecs::World,
    settings:     &mut DebugSettings,
    scene:        &mut SceneUiState,
    physics:      &mut PhysicsUiState,
    audio:        &mut AudioUiState,
    editor:       &mut EditorUiState,
    scripting:    &ScriptingUiState,
    day_night:    &mut DayNightUiState,
    _vehicle_audio: &mut VehicleAudioUiState,
    game_hud:     &mut GameHudState,
) {
    if !matches!(game_hud.kind, GameStateKind::Paused) {
        return;
    }

    egui::SidePanel::left("sidebar")
        .resizable(true)
        .min_width(220.0)
        .max_width(400.0)
        .show(ctx, |ui| {
            // ---- pause controls ------------------------------------------------
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("PAUSED").size(28.0).strong());
            });
            ui.add_space(8.0);
            ui.columns(2, |cols| {
                if cols[0].button(egui::RichText::new("Resume").size(15.0)).clicked() {
                    game_hud.action.resume = true;
                }
                if cols[1].button(egui::RichText::new("Quit").size(15.0)).clicked() {
                    game_hud.action.quit = true;
                }
            });
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("W/S  move   A/D  strafe   Esc  pause")
                    .size(11.0)
                    .color(egui::Color32::from_gray(150)),
            );

            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                // ---- Stats -------------------------------------------------
                ui.collapsing("Stats", |ui| {
                    ui.label(format!("FPS:    {:.0}", stats.fps));
                    ui.label(format!("Frame:  {:.2} ms", stats.frame_ms));
                    let culled = stats.total_entities.saturating_sub(stats.draw_calls);
                    ui.label(format!(
                        "Drawn:  {}/{} ({} culled)",
                        stats.draw_calls, stats.total_entities, culled
                    ));
                    // GPU timings
                    if stats.gpu_timings.available {
                        ui.separator();
                        ui.label(format!("GPU:    {:.2} ms", stats.gpu_timings.total_ms));
                        for (name, ms) in &stats.gpu_timings.passes {
                            ui.label(format!("  {:<12} {:.3} ms", name, ms));
                        }
                    }
                    // Shader reload log
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

                // ---- Renderer ----------------------------------------------
                ui.collapsing("Renderer", |ui| {
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
                    ui.separator();
                    ui.checkbox(&mut settings.ssao_enabled, "SSAO");
                    if settings.ssao_enabled {
                        if settings.msaa_samples > 1 {
                            ui.colored_label(egui::Color32::YELLOW, "SSAO disabled (MSAA on)");
                        } else {
                            ui.add(egui::Slider::new(&mut settings.ssao_strength,     0.0..=1.0 ).text("Strength"));
                            ui.add(egui::Slider::new(&mut settings.ssao_radius,       0.05..=1.0).text("Radius"));
                            ui.add(egui::Slider::new(&mut settings.ssao_bias,       0.001..=0.05).text("Bias"));
                            ui.add(egui::Slider::new(&mut settings.ssao_power,        0.5..=4.0 ).text("Power"));
                            ui.add(egui::Slider::new(&mut settings.ssao_sample_count,  8..=32   ).text("Samples"));
                        }
                    }
                    ui.separator();
                    ui.label("LOD");
                    ui.add(
                        egui::Slider::new(&mut settings.lod_distance_step, 0.0..=50.0)
                            .text("Distance step (m)")
                            .fixed_decimals(1),
                    );
                    if settings.lod_distance_step == 0.0 {
                        ui.colored_label(egui::Color32::YELLOW, "LOD disabled");
                    }
                });

                // ---- Lights ------------------------------------------------
                ui.collapsing("Lights", |ui| {
                    ui.collapsing("Directional", |ui| {
                        for (_, light) in world.query_mut::<&mut DirectionalLight>() {
                            let mut dir = [light.direction.x, light.direction.y, light.direction.z];
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
                    ui.collapsing("Point light", |ui| {
                        for (_, (transform, light)) in
                            world.query_mut::<(&mut Transform, &mut PointLight)>()
                        {
                            ui.add(egui::Slider::new(&mut transform.position.x, -20.0..=20.0).text("X"));
                            ui.add(egui::Slider::new(&mut transform.position.y, -20.0..=20.0).text("Y"));
                            ui.add(egui::Slider::new(&mut transform.position.z, -20.0..=20.0).text("Z"));
                            let mut color = [light.color.x, light.color.y, light.color.z];
                            if ui.color_edit_button_rgb(&mut color).changed() {
                                light.color = glam::Vec3::from(color);
                            }
                            ui.add(egui::Slider::new(&mut light.intensity, 0.0..=20.0).text("Intensity"));
                            ui.add(egui::Slider::new(&mut light.radius,    0.1..=50.0).text("Radius"));
                        }
                    });
                });

                // ---- Day/Night ---------------------------------------------
                ui.collapsing("Day/Night", |ui| {
                    ui.checkbox(&mut day_night.auto_cycle, "Auto cycle");
                    let label = match day_night.time_of_day {
                        t if t < 0.12 => "Noon",
                        t if t < 0.27 => "Afternoon",
                        t if t < 0.35 => "Sunset",
                        t if t < 0.45 => "Dusk",
                        t if t < 0.55 => "Midnight",
                        t if t < 0.65 => "Night",
                        t if t < 0.73 => "Pre-dawn",
                        t if t < 0.80 => "Sunrise",
                        _ => "Morning",
                    };
                    ui.label(format!("Time: {:.2} — {label}", day_night.time_of_day));
                    ui.add(egui::Slider::new(&mut day_night.time_of_day, 0.0..=1.0).show_value(false));
                    ui.add(
                        egui::Slider::new(&mut day_night.cycle_duration, 30.0..=600.0)
                            .text("Cycle (s)")
                            .fixed_decimals(0),
                    );
                });

                // ---- Physics -----------------------------------------------
                ui.collapsing("Physics", |ui| {
                    if ui.button("Spawn Cube").clicked() {
                        physics.spawn_cube_clicked = true;
                    }
                    ui.checkbox(&mut physics.show_wireframe, "Show Collider Wireframes");
                });

                // ---- Audio -------------------------------------------------
                ui.collapsing("Audio", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Volume");
                        ui.add(egui::Slider::new(&mut audio.master_volume, 0.0..=1.0).show_value(false));
                        ui.label(format!("{:.0}%", audio.master_volume * 100.0));
                    });
                    ui.checkbox(&mut audio.muted, "Mute");
                });

                // ---- Scene -------------------------------------------------
                ui.collapsing("Scene", |ui| {
                    ui.label(format!("Models: {}  Entities: {}", scene.model_count, scene.entity_count));
                    ui.horizontal(|ui| {
                        let loading = scene.is_loading;
                        if ui.add_enabled(!loading, egui::Button::new("Save")).clicked() {
                            scene.save_clicked = true;
                        }
                        if ui.add_enabled(!loading, egui::Button::new("Load")).clicked() {
                            scene.load_clicked = true;
                        }
                        if loading { ui.spinner(); }
                    });
                    if !scene.status.is_empty() {
                        let color = if scene.status.starts_with('✓') {
                            egui::Color32::from_rgb(100, 220, 100)
                        } else {
                            egui::Color32::from_rgb(255, 130, 100)
                        };
                        ui.colored_label(color, &scene.status);
                    }
                });

                // ---- Editor ------------------------------------------------
                ui.collapsing("Editor", |ui| {
                    if editor.selected_entity.is_none() {
                        ui.label("No entity selected.");
                        ui.label(
                            egui::RichText::new("Release mouse (Esc), then click an object.")
                                .size(11.0)
                                .color(egui::Color32::from_gray(160)),
                        );
                    } else {
                        let mut changed = false;
                        ui.horizontal(|ui| {
                            ui.label("Pos");
                            changed |= ui.add(egui::DragValue::new(&mut editor.position[0]).speed(0.01).prefix("X:")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut editor.position[1]).speed(0.01).prefix("Y:")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut editor.position[2]).speed(0.01).prefix("Z:")).changed();
                        });
                        ui.horizontal(|ui| {
                            ui.label("Rot");
                            changed |= ui.add(egui::DragValue::new(&mut editor.rotation_euler_deg[0]).speed(0.5).suffix("°").prefix("X:")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut editor.rotation_euler_deg[1]).speed(0.5).suffix("°").prefix("Y:")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut editor.rotation_euler_deg[2]).speed(0.5).suffix("°").prefix("Z:")).changed();
                        });
                        ui.horizontal(|ui| {
                            ui.label("Scl");
                            changed |= ui.add(egui::DragValue::new(&mut editor.scale[0]).speed(0.01).prefix("X:")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut editor.scale[1]).speed(0.01).prefix("Y:")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut editor.scale[2]).speed(0.01).prefix("Z:")).changed();
                        });
                        if changed { editor.transform_changed = true; }
                        ui.separator();
                        ui.label("Gizmo (W/E/R)");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut editor.gizmo_mode, 0, "Translate");
                            ui.selectable_value(&mut editor.gizmo_mode, 1, "Rotate");
                            ui.selectable_value(&mut editor.gizmo_mode, 2, "Scale");
                        });
                    }
                });

                // ---- Scripts -----------------------------------------------
                if !scripting.scripts.is_empty() || !scripting.log_lines.is_empty() {
                    ui.collapsing("Scripts", |ui| {
                        for (path, enabled, error) in &scripting.scripts {
                            ui.horizontal(|ui| {
                                let (marker, color) = if *enabled {
                                    ("●", egui::Color32::from_rgb(80, 200, 80))
                                } else {
                                    ("○", egui::Color32::from_rgb(200, 80, 80))
                                };
                                ui.colored_label(color, marker);
                                ui.label(path);
                            });
                            if let Some(err) = error {
                                ui.colored_label(egui::Color32::from_rgb(255, 100, 100), err);
                            }
                        }
                        if !scripting.log_lines.is_empty() {
                            ui.separator();
                            egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                                for line in &scripting.log_lines {
                                    ui.label(line);
                                }
                            });
                        }
                    });
                }
            });
        });
}

