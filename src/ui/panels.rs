use crate::scene::{DirectionalLight, PointLight, Transform};
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
