---
name: M4 Phase Progress
description: Which Milestone 4 phases are complete and key decisions made
type: project
---

Fase 21 (SSAO) done. Normals reconstructed from depth (Opción B). SSAO disabled when MSAA > 1.

Fase 22 (GPU Profiling) done. Timestamp queries double-buffered; pipeline stats; egui bar chart.

Fase 23 (GPU-driven rendering) done. Mega buffers, instance SSBO, cull.comp compute shader, indirect draw grouped by material. LightingUbo 304 bytes (view_proj + frustum_planes).

Fase 24 (LOD system) done. meshopt 0.3, 4 LOD levels per mesh in mega index buffer. GpuMeshInfo 48 bytes. LOD selection in compute shader. Push constant 8 bytes {instance_count, lod_distance_step}. Shadow always LOD 0.

Fase 25 (Async asset pipeline) done. AssetLoader in src/asset/loader.rs: request_load → std::thread::spawn(load_multi_glb) → mpsc channel. poll_complete() non-blocking; main loop calls it every frame. load_scene() is now non-blocking. apply_loaded_scene() does the GPU upload + ECS rebuild. SceneUiState.is_loading synced.

Fase 26 (Lua scripting) done as of 2026-03-26.
- src/scripting/mod.rs: ScriptEngine with mlua 0.10 (lua54, vendored)
- Bootstrap Lua embedded in Rust as const str — sets up _engine_cmds, _engine_timers, engine API table
- Pure command queue approach: Lua pushes to _engine_cmds, Rust drains after _engine_tick(dt)
- engine.spawn/destroy/set_position/play_sound/log/every API
- Hot-reload via notify watcher on scripts/ directory
- Error handling: script error → ScriptInfo.last_error, script disabled, engine keeps running
- scene.json has "scripts": [...] field (serde default = [])
- Panel "Scripts" in egui: ●/○ per script, scrollable log
- scripts/game.lua: spawns random cubes every 2s via engine.every
- Collision callbacks and engine.get_position: deferred to M5

Fase 29 (Day/Night Cycle) done as of 2026-03-26.
- DayNightUiState in ui/mod.rs: time_of_day, auto_cycle, cycle_duration (default 180s)
- update_day_night(dt) in main.rs: advances time, updates ECS DirectionalLight (dir + color + intensity) and StreetLight PointLights
- Sun direction: rotates in XZ plane via angle=2π*t, formula (sin*0.6, -cos, 0.3).normalize()
- Color/intensity: piecewise-linear keyframes via lerp4() helper, 11 control points noon→night→noon
- Sky tint: sample_sky_tint(t) → [f32;4] passed as push constant to skybox.frag at offset 64 (fragment stage)
- skybox.frag: tints cubemap sample by skyTint.rgb * skyTint.a before Reinhard
- StreetLight component in ecs.rs: PointLights that auto-on at night (t 0.35..0.65)
- 4 street lights spawned at corners (±4, 3, ±4) in resumed()
- Day/Night egui panel with time slider + auto-cycle toggle + cycle duration

Fase 30 (Vehicle Audio) done as of 2026-03-27.
- Vehicle ECS component in ecs.rs: current_speed, current_rpm, max_rpm, max_speed, acceleration_input, brake_input, is_skidding + audio handles (engine/skid/wind)
- update_vehicle_audio(dt) in main.rs: simulates RPM+speed from W/S/Space input, drives 3 audio channels
- Engine: looping sawtooth WAV (120Hz harmonics), pitch = 0.4 + (rpm/max_rpm)*1.6
- Wind: looping beating-frequencies WAV, volume = (speed/max_speed)*0.8
- Skid: looping 850Hz AM buzz, started when brake>0.3 && speed>4, stopped otherwise
- Placeholder WAVs generated at startup: engine_loop.wav, skid.wav, wind_loop.wav
- Space = emergency brake (full skid), S = partial brake
- VehicleAudioUiState: per-channel volume sliders in egui panel
- Impact sounds: scaled by effects_volume channel

Fase 31 (Checkpoints + Game States + HUD) done as of 2026-03-27.
- GameState enum (Menu/Countdown/Racing/Paused/Finished) + GameSession struct in main.rs
- GameHudState / GameAction / GameStateKind in ui/mod.rs
- update_game(dt) in main.rs: consumes overlay actions, advances countdown/timer, detects checkpoint sphere overlap (camera pos as vehicle proxy until Fase 27)
- 4 Checkpoint entities spawned in resumed(): diamond layout (0,0,6), (7,0,0), (0,0,-6), (-7,0,0/finish)
- Checkpoint trigger radius 2.5 m; finish line = index 3
- Countdown starts at 4.0 s, "GO!" shows from 0 down to -0.5 s (then → Racing)
- Vehicle input blocked (can_drive = state==Racing) in update_vehicle_audio
- Escape: Racing→Paused, Paused→Racing + recapture mouse
- HUD: game_hud_panel (timer top-center, speed+RPM bottom-right) + game_overlay_panel (centered Area per state)
- Overlay panels: Menu (PLAY/QUIT), Countdown (3-2-1-GO!), Paused (RESUME/RESTART/MENU), Finished (time+best+RESTART/MENU)
- Quit from Menu → exit_requested = true → event_loop.exit()
- Vehicle input (accel/brake) gated on GameState::Racing

Fase 32 (Polish + Packaging) done as of 2026-03-27.
- [[example]] name = "driving_game" path = "src/main.rs" in Cargo.toml
- assets/LICENSES.md, README.md (full rewrite), examples/driving_game/README.md added

Pivot (2026-03-27): Racing game → open exploration sandbox.
- GameState: Menu/Countdown/Racing/Paused/Finished → Exploring/Paused
- GameSession: stripped to {state, exit_requested}
- GameHudState/GameStateKind/GameAction simplified to match (no timer/checkpoint fields)
- spawn_scene_default() replaced by spawn_sandbox_scene(): 40×40 ground, 4 walls, 4 pillars, 6 static obstacles, 11 dynamic props, 3 model instances, 2 point lights
- Escape: Exploring→Paused (release mouse), Paused→Exploring (capture mouse)
- Can drive when GameState::Exploring (not Paused)
- Checkpoints removed; best-lap persistence (load/save_best_lap) removed
- scene/mod.rs: Checkpoint removed from re-exports

**Why:** User wants free exploration rather than lap racing.
**How to apply:** No racing features remain. Vehicle drives freely in the sandbox world.
