use crate::scene::{
    DirectionalLight, EasingType, MaterialOverride, MeshRenderer, ParticleEmitter,
    PointLight, PropertyAnimator, Transform, TriggerAction, TriggerShape, TriggerZone,
};
use super::{
    AudioUiState, DayNightUiState, DebugSettings, EditorUiState, FrameStats,
    GameHudState, GameStateKind, MainMenuUiState, PhysicsUiState, PropsUiState,
    SceneUiState, ScriptingUiState, SettingsUiState, VehicleAudioUiState,
};

// ---------------------------------------------------------------------------
// Main menu (Fase 34)
// ---------------------------------------------------------------------------

pub fn main_menu(ctx: &egui::Context, state: &mut MainMenuUiState) {
    egui::CentralPanel::default().show(ctx, |ui| {
        let total = ui.available_size();
        ui.allocate_ui_with_layout(
            total,
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                ui.add_space(total.y * 0.2);
                ui.label(egui::RichText::new("ralk").size(72.0).strong());
                ui.add_space(20.0);

                let btn_size = egui::Vec2::new(200.0, 40.0);

                let continue_btn = ui.add_enabled(
                    state.has_last_session,
                    egui::Button::new(egui::RichText::new("Continuar").size(18.0))
                        .min_size(btn_size),
                );
                if continue_btn.clicked() {
                    state.action.continue_game = true;
                }

                ui.add_space(8.0);
                if ui.add(egui::Button::new(egui::RichText::new("Nueva escena").size(18.0)).min_size(btn_size)).clicked() {
                    state.action.new_scene = true;
                }

                if !state.saved_scenes.is_empty() {
                    ui.add_space(8.0);
                    ui.collapsing("Cargar escena", |ui| {
                        for name in &state.saved_scenes {
                            if ui.button(name).clicked() {
                                state.action.load_scene = Some(name.clone());
                            }
                        }
                    });
                }

                ui.add_space(20.0);
                if ui.add(egui::Button::new(egui::RichText::new("Salir").size(16.0)).min_size(egui::Vec2::new(120.0, 30.0))).clicked() {
                    state.action.quit = true;
                }

                if !state.has_last_session {
                    ui.add_space(12.0);
                    ui.colored_label(
                        egui::Color32::from_gray(130),
                        "Sin sesión guardada. Creá una escena nueva.",
                    );
                }
            },
        );
    });
}

// ---------------------------------------------------------------------------
// Settings / scene creation (Fase 35)
// ---------------------------------------------------------------------------

pub fn settings_panel(ctx: &egui::Context, state: &mut SettingsUiState) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("Nueva escena").size(32.0).strong());
        });
        ui.add_space(16.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Nombre:");
                ui.text_edit_singleline(&mut state.scene_name);
            });
            ui.add_space(8.0);

            ui.label("Skybox:");
            if state.skybox_options.is_empty() {
                ui.colored_label(egui::Color32::YELLOW, "No hay skyboxes en assets/skyboxes/");
            } else {
                egui::ComboBox::from_id_salt("skybox_select")
                    .selected_text(basename(&state.skybox_options[state.selected_skybox]))
                    .show_ui(ui, |ui| {
                        for (i, path) in state.skybox_options.iter().enumerate() {
                            ui.selectable_value(&mut state.selected_skybox, i, basename(path));
                        }
                    });
            }

            ui.add_space(4.0);
            ui.label("Terreno:");
            if state.terrain_options.is_empty() {
                ui.colored_label(egui::Color32::YELLOW, "No hay terrenos en assets/terrains/");
            } else {
                egui::ComboBox::from_id_salt("terrain_select")
                    .selected_text(basename(&state.terrain_options[state.selected_terrain]))
                    .show_ui(ui, |ui| {
                        for (i, path) in state.terrain_options.iter().enumerate() {
                            ui.selectable_value(&mut state.selected_terrain, i, basename(path));
                        }
                    });
            }

            ui.add_space(4.0);
            ui.label("Personaje:");
            if state.character_options.is_empty() {
                ui.colored_label(egui::Color32::from_gray(150), "(capsule placeholder)");
            } else {
                egui::ComboBox::from_id_salt("char_select")
                    .selected_text(basename(&state.character_options[state.selected_character]))
                    .show_ui(ui, |ui| {
                        for (i, path) in state.character_options.iter().enumerate() {
                            ui.selectable_value(&mut state.selected_character, i, basename(path));
                        }
                    });
            }

            ui.add_space(4.0);
            ui.label("Catálogo de props:");
            if state.catalog_options.is_empty() {
                ui.colored_label(egui::Color32::YELLOW, "No hay catálogos en assets/props/");
            } else {
                egui::ComboBox::from_id_salt("catalog_select")
                    .selected_text(basename(&state.catalog_options[state.selected_catalog]))
                    .show_ui(ui, |ui| {
                        for (i, path) in state.catalog_options.iter().enumerate() {
                            ui.selectable_value(&mut state.selected_catalog, i, basename(path));
                        }
                    });
            }

            ui.add_space(8.0);
            ui.collapsing("Gráficos", |ui| {
                ui.horizontal(|ui| {
                    ui.label("MSAA");
                    ui.selectable_value(&mut state.msaa, 1, "Off");
                    ui.selectable_value(&mut state.msaa, 2, "2×");
                    ui.selectable_value(&mut state.msaa, 4, "4×");
                });
                ui.checkbox(&mut state.ssao, "SSAO");
                ui.checkbox(&mut state.bloom, "Bloom");
            });

            ui.add_space(16.0);
            ui.columns(2, |cols| {
                let name_ok = !state.scene_name.trim().is_empty();
                if cols[0].add_enabled(
                    name_ok,
                    egui::Button::new(egui::RichText::new("Crear escena").size(16.0))
                        .min_size(egui::Vec2::new(140.0, 35.0)),
                ).clicked() {
                    state.action_create = true;
                }
                if cols[1].button(egui::RichText::new("Volver").size(16.0)).clicked() {
                    state.action_back = true;
                }
            });
        });
    });
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

// ---------------------------------------------------------------------------
// In-scene sidebar (pause menu + all debug sections)
// ---------------------------------------------------------------------------

/// The only entry-point for the in-game UI.
///
/// - **Exploring**: renders nothing.
/// - **Paused**: renders a left sidebar with pause controls and all debug sections.
#[allow(clippy::too_many_arguments)]
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
    props:        &mut PropsUiState,
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
                egui::RichText::new("W/S  move   A/D  strafe   Space  jump   Esc  pause")
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
                    ui.add(egui::Slider::new(&mut settings.ibl_scale, 0.0..=1.0)
                        .text("IBL (ambient)")
                        .fixed_decimals(2));
                    ui.separator();
                    ui.checkbox(&mut settings.bloom_enabled, "Bloom");
                    if settings.bloom_enabled {
                        ui.add(egui::Slider::new(&mut settings.bloom_intensity, 0.0..=2.0).text("Intensity").fixed_decimals(2));
                        ui.add(egui::Slider::new(&mut settings.bloom_threshold, 0.1..=2.0).text("Threshold").fixed_decimals(2));
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
                    ui.add_space(4.0);
                    ui.label("Jump force (kg·m/s)");
                    ui.add(
                        egui::Slider::new(&mut physics.jump_force, 0.5..=20.0)
                            .text("Jump force")
                            .fixed_decimals(1),
                    );
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

                // ---- Scene Tree (Fase 38) ----------------------------------
                ui.collapsing("Scene Tree", |ui| {
                    scene_tree_content(ui, world, editor);
                });

                // ---- Material Editor (Fase 44) -----------------------------
                // Aparece para cualquier Mesh seleccionado; permite añadir override.
                if let Some(sel) = editor.selected_entity {
                    if world.get::<&MeshRenderer>(sel).is_ok() {
                        ui.collapsing("Material Override", |ui| {
                            if world.get::<&MaterialOverride>(sel).is_ok() {
                                material_editor_content(ui, world, sel);
                            } else {
                                ui.label(egui::RichText::new("Sin override").color(egui::Color32::from_gray(150)));
                                if ui.button("+ Añadir Material Override").clicked() {
                                    let _ = world.insert_one(sel, MaterialOverride {
                                        base_color_factor: None,
                                        metallic_factor: None,
                                        roughness_factor: None,
                                        emissive_factor: None,
                                        emissive_intensity: None,
                                        normal_scale: None,
                                        uv_scale: None,
                                    });
                                }
                            }
                        });
                    }
                }

                // ---- Particle Emitter (Fase 40) ----------------------------
                if let Some(sel) = editor.selected_entity {
                    ui.collapsing("Particle Emitter", |ui| {
                        if world.get::<&ParticleEmitter>(sel).is_ok() {
                            particle_emitter_content(ui, world, sel);
                        } else {
                            ui.label(egui::RichText::new("Sin emisor").color(egui::Color32::from_gray(150)));
                            ui.horizontal(|ui| {
                                if ui.button("+ Fuego").clicked() {
                                    let _ = world.insert_one(sel, ParticleEmitter::fire_preset());
                                }
                                if ui.button("+ Humo").clicked() {
                                    let _ = world.insert_one(sel, ParticleEmitter::smoke_preset());
                                }
                            });
                        }
                    });
                }

                // ---- Property Animator (Fase 41) ---------------------------
                if let Some(sel) = editor.selected_entity {
                    ui.collapsing("Animator", |ui| {
                        if world.get::<&PropertyAnimator>(sel).is_ok() {
                            property_animator_content(ui, world, sel);
                        } else {
                            ui.label(egui::RichText::new("Sin animador").color(egui::Color32::from_gray(150)));
                            if ui.button("+ Añadir Animator (puerta)").clicked() {
                                let _ = world.insert_one(sel, PropertyAnimator {
                                    from_rot_y: 0.0,
                                    to_rot_y: std::f32::consts::FRAC_PI_2,
                                    duration: 1.0,
                                    elapsed: 0.0,
                                    easing: EasingType::EaseInOut,
                                    playing: false,
                                    loop_anim: false,
                                    reverse: false,
                                });
                            }
                        }
                    });
                }

                // ---- Trigger Zone (Fase 42) --------------------------------
                if let Some(sel) = editor.selected_entity {
                    ui.collapsing("Trigger Zone", |ui| {
                        if world.get::<&TriggerZone>(sel).is_ok() {
                            trigger_zone_content(ui, world, sel);
                        } else {
                            ui.label(egui::RichText::new("Sin trigger").color(egui::Color32::from_gray(150)));
                            if ui.button("+ Añadir Trigger Zone (caja)").clicked() {
                                let _ = world.insert_one(sel, TriggerZone {
                                    shape: TriggerShape::Box,
                                    size: glam::Vec3::new(2.0, 2.0, 2.0),
                                    on_enter: Some(TriggerAction::PlaySound {
                                        path: "assets/audio/click.wav".into(),
                                        volume: 0.5,
                                    }),
                                    on_exit: None,
                                    once: false,
                                    triggered: false,
                                    player_inside: false,
                                    visible_in_editor: true,
                                });
                            }
                        }
                    });
                }

                // ---- Props catalog (Fase 36) --------------------------------
                ui.collapsing("Props (Tab)", |ui| {
                    props_panel_content(ui, props);
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

// ---------------------------------------------------------------------------
// Props panel content (also used as standalone floating window via Tab)
// ---------------------------------------------------------------------------

fn props_panel_content(ui: &mut egui::Ui, props: &mut PropsUiState) {
    // Grid snap controls
    ui.horizontal(|ui| {
        ui.checkbox(&mut props.grid_snap, "Grid snap");
        if props.grid_snap {
            ui.selectable_value(&mut props.grid_size, 0.5, "0.5");
            ui.selectable_value(&mut props.grid_size, 1.0, "1.0");
            ui.selectable_value(&mut props.grid_size, 2.0, "2.0");
        }
    });

    ui.separator();

    // Action buttons
    ui.horizontal(|ui| {
        if ui.button("Borrar").clicked() {
            props.delete_clicked = true;
        }
        if ui.button("Duplicar").clicked() {
            props.duplicate_clicked = true;
        }
        if ui.button("Deshacer").clicked() {
            props.undo_clicked = true;
        }
    });

    ui.separator();

    // Active placement indicator
    if let Some(ref selected) = props.selected_prop.clone() {
        ui.colored_label(egui::Color32::from_rgb(100, 220, 100),
            format!("Colocando: {selected}"));
        ui.label(egui::RichText::new("Click en el suelo para colocar. Esc para cancelar.")
            .size(11.0).color(egui::Color32::from_gray(160)));
        if ui.button("Cancelar").clicked() {
            props.selected_prop = None;
        }
        ui.separator();
    }

    // Search bar
    ui.horizontal(|ui| {
        ui.label("Buscar:");
        ui.text_edit_singleline(&mut props.search);
    });

    // Catalog list
    if props.catalog_entries.is_empty() {
        ui.colored_label(egui::Color32::from_gray(150),
            "Sin catálogo cargado.\nCargá un catálogo desde assets/props/");
        return;
    }

    let search_lower = props.search.to_lowercase();
    let filter_cat = props.category_filter.clone();

    // Collect unique categories for filter tabs
    let mut categories: Vec<String> = props.catalog_entries.iter()
        .map(|(_, _, cat)| cat.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    categories.insert(0, String::new()); // "" = all

    ui.horizontal_wrapped(|ui| {
        let all_selected = filter_cat.is_empty();
        if ui.selectable_label(all_selected, "Todo").clicked() {
            props.category_filter.clear();
        }
        for cat in &categories[1..] {
            if ui.selectable_label(filter_cat == *cat, cat).clicked() {
                props.category_filter = cat.clone();
            }
        }
    });

    ui.add_space(4.0);

    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
        let entries: Vec<(String, String)> = props.catalog_entries.iter()
            .filter(|(_, name, cat)| {
                (filter_cat.is_empty() || *cat == filter_cat)
                    && (search_lower.is_empty() || name.to_lowercase().contains(&search_lower))
            })
            .map(|(id, name, _)| (id.clone(), name.clone()))
            .collect();

        for (id, name) in entries {
            let selected = props.selected_prop.as_deref() == Some(&id);
            if ui.selectable_label(selected, &name).clicked() {
                props.selected_prop = Some(id.clone());
                props.place_clicked = Some(id);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Scene Tree (Fase 38) — flat list with parent indicator
// ---------------------------------------------------------------------------
fn scene_tree_content(ui: &mut egui::Ui, world: &mut hecs::World, editor: &mut EditorUiState) {
    use crate::scene::Parent;

    // Build list with descriptive labels.
    let mut entries: Vec<(hecs::Entity, bool, String)> = world
        .query::<()>()
        .iter()
        .map(|(e, ())| {
            let has_parent = world.get::<&Parent>(e).is_ok();
            // Build a human-readable label from components present.
            let kind = if world.get::<&DirectionalLight>(e).is_ok() {
                "DirLight"
            } else if world.get::<&PointLight>(e).is_ok() {
                "PointLight"
            } else if world.get::<&ParticleEmitter>(e).is_ok() {
                "Particles"
            } else if world.get::<&MeshRenderer>(e).is_ok() {
                "Mesh"
            } else {
                "Entity"
            };
            // Show position if it has a Transform.
            let pos_suffix = if let Ok(t) = world.get::<&Transform>(e) {
                format!(" ({:.1},{:.1},{:.1})", t.position.x, t.position.y, t.position.z)
            } else {
                String::new()
            };
            let indent = if has_parent { "  └ " } else { "" };
            let label = format!("{}[{}] {}{}", indent, e.id(), kind, pos_suffix);
            (e, has_parent, label)
        })
        .collect();
    entries.sort_by_key(|(e, _, _)| e.id());

    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
        for (entity, _, label) in &entries {
            let selected = editor.selected_entity == Some(*entity);
            if ui.selectable_label(selected, label).clicked() {
                editor.selected_entity = Some(*entity);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Material Editor (Fase 44) — roughness/metallic/color/emissive sliders
// ---------------------------------------------------------------------------
fn material_editor_content(ui: &mut egui::Ui, world: &mut hecs::World, entity: hecs::Entity) {
    let Ok(mut ov) = world.get::<&mut MaterialOverride>(entity) else { return };

    ui.label("Base Color");
    let mut has_color = ov.base_color_factor.is_some();
    if ui.checkbox(&mut has_color, "Override").changed() {
        ov.base_color_factor = if has_color { Some([1.0, 1.0, 1.0, 1.0]) } else { None };
    }
    if let Some(ref mut c) = ov.base_color_factor {
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut c[0]).clamp_range(0.0..=1.0).speed(0.01).prefix("R:"));
            ui.add(egui::DragValue::new(&mut c[1]).clamp_range(0.0..=1.0).speed(0.01).prefix("G:"));
            ui.add(egui::DragValue::new(&mut c[2]).clamp_range(0.0..=1.0).speed(0.01).prefix("B:"));
        });
    }

    ui.add_space(4.0);
    let mut has_metallic = ov.metallic_factor.is_some();
    if ui.checkbox(&mut has_metallic, "Metallic").changed() {
        ov.metallic_factor = if has_metallic { Some(0.0) } else { None };
    }
    if let Some(ref mut m) = ov.metallic_factor {
        ui.add(egui::Slider::new(m, 0.0..=1.0).text("Metallic"));
    }

    let mut has_roughness = ov.roughness_factor.is_some();
    if ui.checkbox(&mut has_roughness, "Roughness").changed() {
        ov.roughness_factor = if has_roughness { Some(0.5) } else { None };
    }
    if let Some(ref mut r) = ov.roughness_factor {
        ui.add(egui::Slider::new(r, 0.0..=1.0).text("Roughness"));
    }

    ui.add_space(4.0);
    let mut has_emissive = ov.emissive_factor.is_some();
    if ui.checkbox(&mut has_emissive, "Emissive").changed() {
        ov.emissive_factor = if has_emissive { Some([0.0, 0.0, 0.0]) } else { None };
        ov.emissive_intensity = if has_emissive { Some(1.0) } else { None };
    }
    if let Some(ref mut em) = ov.emissive_factor {
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut em[0]).clamp_range(0.0..=1.0).speed(0.01).prefix("R:"));
            ui.add(egui::DragValue::new(&mut em[1]).clamp_range(0.0..=1.0).speed(0.01).prefix("G:"));
            ui.add(egui::DragValue::new(&mut em[2]).clamp_range(0.0..=1.0).speed(0.01).prefix("B:"));
        });
    }
    if let Some(ref mut ei) = ov.emissive_intensity {
        ui.add(egui::Slider::new(ei, 0.0..=10.0).text("Intensity"));
    }
}

// ---------------------------------------------------------------------------
// Particle Emitter Editor (Fase 40)
// ---------------------------------------------------------------------------
fn particle_emitter_content(ui: &mut egui::Ui, world: &mut hecs::World, entity: hecs::Entity) {
    let Ok(mut em) = world.get::<&mut ParticleEmitter>(entity) else { return };

    ui.checkbox(&mut em.enabled, "Enabled");
    ui.add(egui::Slider::new(&mut em.spawn_rate, 0.0..=200.0).text("Spawn Rate"));
    ui.add(egui::Slider::new(&mut em.lifetime_min, 0.1..=5.0).text("Life Min"));
    ui.add(egui::Slider::new(&mut em.lifetime_max, 0.1..=5.0).text("Life Max"));
    ui.add(egui::Slider::new(&mut em.start_size_min, 0.01..=1.0).text("Size Min"));
    ui.add(egui::Slider::new(&mut em.start_size_max, 0.01..=1.0).text("Size Max"));
    ui.label(format!("Live particles: {}", em.particles.len()));
}

// ---------------------------------------------------------------------------
// Property Animator Editor (Fase 41)
// ---------------------------------------------------------------------------
fn property_animator_content(ui: &mut egui::Ui, world: &mut hecs::World, entity: hecs::Entity) {
    let Ok(mut pa) = world.get::<&mut PropertyAnimator>(entity) else { return };

    ui.add(egui::Slider::new(&mut pa.from_rot_y, -std::f32::consts::PI..=std::f32::consts::PI).text("From Y (rad)"));
    ui.add(egui::Slider::new(&mut pa.to_rot_y, -std::f32::consts::PI..=std::f32::consts::PI).text("To Y (rad)"));
    ui.add(egui::Slider::new(&mut pa.duration, 0.1..=5.0).text("Duration (s)"));
    ui.checkbox(&mut pa.loop_anim, "Loop");
    ui.checkbox(&mut pa.reverse, "Reverse");

    ui.horizontal(|ui| {
        if ui.button("Play").clicked() {
            pa.playing = true;
            pa.elapsed = 0.0;
        }
        if ui.button("Stop").clicked() {
            pa.playing = false;
        }
    });
    ui.add(egui::ProgressBar::new(pa.progress()).text("Progress"));
}

// ---------------------------------------------------------------------------
// Trigger Zone Editor (Fase 42)
// ---------------------------------------------------------------------------
fn trigger_zone_content(ui: &mut egui::Ui, world: &mut hecs::World, entity: hecs::Entity) {
    let Ok(mut tz) = world.get::<&mut TriggerZone>(entity) else { return };

    ui.horizontal(|ui| {
        ui.label("Size:");
        ui.add(egui::DragValue::new(&mut tz.size.x).speed(0.05).prefix("X:"));
        ui.add(egui::DragValue::new(&mut tz.size.y).speed(0.05).prefix("Y:"));
        ui.add(egui::DragValue::new(&mut tz.size.z).speed(0.05).prefix("Z:"));
    });
    ui.checkbox(&mut tz.once, "Fire Once");
    ui.checkbox(&mut tz.visible_in_editor, "Visible");
    ui.label(format!("Player inside: {}", tz.player_inside));
    ui.label(format!("Triggered: {}", tz.triggered));
    if ui.button("Reset").clicked() {
        tz.triggered = false;
        tz.player_inside = false;
    }
}
