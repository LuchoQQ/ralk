use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc;

use anyhow::Result;
use mlua::prelude::*;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

// ---------------------------------------------------------------------------
// Bootstrap Lua — sets up the engine API and internal state tables.
// ---------------------------------------------------------------------------

const BOOTSTRAP: &str = r#"
_engine_cmds   = {}
_engine_timers = {}
engine = {}

function engine.spawn(opts)
    local pos = opts.position or {0, 0, 0}
    table.insert(_engine_cmds, {
        type = "spawn_cube",
        x = pos[1] or 0,
        y = pos[2] or 0,
        z = pos[3] or 0,
    })
end

function engine.destroy(id)
    table.insert(_engine_cmds, { type = "destroy", id = id })
end

function engine.set_position(id, pos)
    table.insert(_engine_cmds, {
        type = "set_position",
        id = id,
        x = pos[1] or 0,
        y = pos[2] or 0,
        z = pos[3] or 0,
    })
end

function engine.play_sound(path, volume)
    table.insert(_engine_cmds, {
        type = "play_sound",
        path = path,
        volume = volume or 1.0,
    })
end

function engine.log(msg)
    table.insert(_engine_cmds, { type = "log", msg = tostring(msg) })
end

function engine.every(interval, fn)
    table.insert(_engine_timers, {
        interval = interval,
        elapsed  = 0.0,
        callback = fn,
    })
end

--- Called by Rust each frame. Ticks all registered timers.
function _engine_tick(dt)
    for _, timer in ipairs(_engine_timers) do
        timer.elapsed = timer.elapsed + dt
        if timer.elapsed >= timer.interval then
            timer.elapsed = timer.elapsed - timer.interval
            local ok, err = pcall(timer.callback)
            if not ok then
                table.insert(_engine_cmds, {
                    type = "log",
                    msg  = "Timer error: " .. tostring(err),
                })
            end
        end
    end
end
"#;

// ---------------------------------------------------------------------------
// Script commands produced by the Lua VM — consumed by main loop.
// ---------------------------------------------------------------------------

pub enum ScriptCommand {
    SpawnCube { position: [f32; 3] },
    DestroyEntity { id: u64 },
    SetPosition { id: u64, position: [f32; 3] },
    PlaySound { path: String, volume: f32 },
    Log { message: String },
}

// ---------------------------------------------------------------------------
// Per-script metadata (shown in the egui scripting panel).
// ---------------------------------------------------------------------------

pub struct ScriptInfo {
    pub path:       String,
    pub enabled:    bool,
    pub last_error: Option<String>,
}

// ---------------------------------------------------------------------------
// ScriptEngine
// ---------------------------------------------------------------------------

pub struct ScriptEngine {
    lua:       Lua,
    pub scripts:   Vec<ScriptInfo>,
    reload_rx: mpsc::Receiver<notify::Event>,
    /// Kept alive; dropping it stops the watcher.
    _watcher:  RecommendedWatcher,
    /// Recent log lines from scripts (capped at 50).
    pub log_lines: VecDeque<String>,
}

impl ScriptEngine {
    pub fn new() -> Result<Self> {
        let lua = Lua::new();

        // Install the engine bootstrap (API + internal tables).
        lua.load(BOOTSTRAP)
            .set_name("bootstrap")
            .exec()
            .map_err(|e| anyhow::anyhow!("Lua bootstrap error: {e}"))?;

        let (tx, rx) = mpsc::channel::<notify::Event>();
        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            })
            .map_err(|e| anyhow::anyhow!("Notify watcher error: {e}"))?;

        if std::path::Path::new("scripts").exists() {
            if let Err(e) =
                watcher.watch(std::path::Path::new("scripts"), RecursiveMode::NonRecursive)
            {
                log::warn!("Could not watch scripts/: {e}");
            }
        }

        Ok(Self {
            lua,
            scripts: Vec::new(),
            reload_rx: rx,
            _watcher: watcher,
            log_lines: VecDeque::new(),
        })
    }

    // -----------------------------------------------------------------------
    // Script loading
    // -----------------------------------------------------------------------

    /// Load (or reload) a script from `path`. On error, logs and marks the
    /// script as disabled — does NOT crash the engine.
    pub fn load_script(&mut self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(source) => match self.lua.load(&source).set_name(path).exec() {
                Ok(()) => {
                    log::info!("Script loaded: {path}");
                    if let Some(info) = self.scripts.iter_mut().find(|s| s.path == path) {
                        info.enabled = true;
                        info.last_error = None;
                    } else {
                        self.scripts.push(ScriptInfo {
                            path:       path.to_string(),
                            enabled:    true,
                            last_error: None,
                        });
                    }
                }
                Err(e) => {
                    let msg = format!("[{path}] {e}");
                    log::warn!("Script error: {msg}");
                    self.push_log(msg.clone());
                    if let Some(info) = self.scripts.iter_mut().find(|s| s.path == path) {
                        info.enabled = false;
                        info.last_error = Some(e.to_string());
                    } else {
                        self.scripts.push(ScriptInfo {
                            path:       path.to_string(),
                            enabled:    false,
                            last_error: Some(e.to_string()),
                        });
                    }
                }
            },
            Err(e) => {
                log::warn!("Cannot read script '{path}': {e}");
            }
        }
    }

    /// Reset timer / command state and load a new set of scripts.
    /// Called when a scene is (re)loaded.
    pub fn reload_scripts(&mut self, paths: &[String]) {
        let _ = self.lua.load("_engine_cmds = {}; _engine_timers = {}").exec();
        self.scripts.clear();
        for path in paths {
            self.load_script(path);
        }
    }

    // -----------------------------------------------------------------------
    // Hot-reload
    // -----------------------------------------------------------------------

    /// Poll the file-system watcher and reload any changed `.lua` files.
    pub fn poll_reload(&mut self) {
        let mut changed: Vec<PathBuf> = Vec::new();
        while let Ok(event) = self.reload_rx.try_recv() {
            for path in event.paths {
                if path.extension().map(|e| e == "lua").unwrap_or(false) {
                    changed.push(path);
                }
            }
        }
        for path in changed {
            let path_str = path.to_string_lossy().replace('\\', "/");
            let is_known = self.scripts.iter().any(|s| {
                std::path::Path::new(&s.path).canonicalize().ok()
                    == path.canonicalize().ok()
            });
            if is_known {
                log::info!("Hot-reloading: {path_str}");
                let _ = self.lua.load("_engine_cmds = {}; _engine_timers = {}").exec();
                self.load_script(&path_str);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Per-frame update
    // -----------------------------------------------------------------------

    /// Tick all timers and drain the command queue. Returns commands to apply.
    pub fn update(&mut self, dt: f32) -> Vec<ScriptCommand> {
        // Tick timers via _engine_tick(dt).
        let tick_result: LuaResult<LuaFunction> = self.lua.globals().get("_engine_tick");
        if let Ok(tick) = tick_result {
            if let Err(e) = tick.call::<()>(dt) {
                let msg = format!("_engine_tick error: {e}");
                log::warn!("{msg}");
                self.push_log(msg);
            }
        }

        // Drain _engine_cmds table.
        let cmds_result: LuaResult<LuaTable> = self.lua.globals().get("_engine_cmds");
        let cmds_table = match cmds_result {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };

        let mut commands = Vec::new();
        let len = cmds_table.raw_len();
        for i in 1..=len {
            let cmd: LuaTable = match cmds_table.raw_get(i) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let cmd_type: String = cmd.get("type").unwrap_or_default();
            match cmd_type.as_str() {
                "spawn_cube" => {
                    let x: f32 = cmd.get("x").unwrap_or(0.0);
                    let y: f32 = cmd.get("y").unwrap_or(0.0);
                    let z: f32 = cmd.get("z").unwrap_or(0.0);
                    commands.push(ScriptCommand::SpawnCube { position: [x, y, z] });
                }
                "destroy" => {
                    let id: u64 = cmd.get("id").unwrap_or(0);
                    commands.push(ScriptCommand::DestroyEntity { id });
                }
                "set_position" => {
                    let id: u64 = cmd.get("id").unwrap_or(0);
                    let x: f32 = cmd.get("x").unwrap_or(0.0);
                    let y: f32 = cmd.get("y").unwrap_or(0.0);
                    let z: f32 = cmd.get("z").unwrap_or(0.0);
                    commands.push(ScriptCommand::SetPosition { id, position: [x, y, z] });
                }
                "play_sound" => {
                    let path: String = cmd.get("path").unwrap_or_default();
                    let volume: f32 = cmd.get("volume").unwrap_or(1.0);
                    commands.push(ScriptCommand::PlaySound { path, volume });
                }
                "log" => {
                    let msg: String = cmd.get("msg").unwrap_or_default();
                    self.push_log(msg.clone());
                    commands.push(ScriptCommand::Log { message: msg });
                }
                other if !other.is_empty() => {
                    log::warn!("Unknown script command: {other}");
                }
                _ => {}
            }
        }

        // Reset the command table for the next frame.
        let _ = self.lua.load("_engine_cmds = {}").exec();

        commands
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn push_log(&mut self, msg: String) {
        if self.log_lines.len() >= 50 {
            self.log_lines.pop_front();
        }
        self.log_lines.push_back(msg);
    }
}
