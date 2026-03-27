use serde::{Deserialize, Serialize};
use anyhow::Result;

const CONFIG_PATH: &str = "config.json";

/// Persistent renderer and audio settings saved to `config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // Renderer
    #[serde(default = "default_msaa")]
    pub msaa: u32,
    #[serde(default = "default_true")]
    pub ssao: bool,
    #[serde(default = "default_ssao_strength")]
    pub ssao_strength: f32,
    #[serde(default = "default_ssao_radius")]
    pub ssao_radius: f32,
    #[serde(default = "default_ssao_bias")]
    pub ssao_bias: f32,
    #[serde(default = "default_ssao_power")]
    pub ssao_power: f32,
    #[serde(default = "default_ssao_samples")]
    pub ssao_samples: u32,
    #[serde(default)]
    pub tone_aces: bool,
    #[serde(default = "default_lod")]
    pub lod_distance_step: f32,
    #[serde(default = "default_true")]
    pub bloom: bool,
    #[serde(default = "default_bloom_intensity")]
    pub bloom_intensity: f32,
    #[serde(default = "default_bloom_threshold")]
    pub bloom_threshold: f32,
    #[serde(default = "default_ibl_scale")]
    pub ibl_scale: f32,
    // Audio
    #[serde(default = "default_volume")]
    pub master_volume: f32,
    #[serde(default)]
    pub muted: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            msaa: default_msaa(),
            ssao: true,
            ssao_strength: default_ssao_strength(),
            ssao_radius: default_ssao_radius(),
            ssao_bias: default_ssao_bias(),
            ssao_power: default_ssao_power(),
            ssao_samples: default_ssao_samples(),
            tone_aces: false,
            lod_distance_step: default_lod(),
            bloom: true,
            bloom_intensity: default_bloom_intensity(),
            bloom_threshold: default_bloom_threshold(),
            ibl_scale: default_ibl_scale(),
            master_volume: default_volume(),
            muted: false,
        }
    }
}

fn default_msaa() -> u32 { 4 }
fn default_true() -> bool { true }
fn default_ssao_strength() -> f32 { 1.0 }
fn default_ssao_radius() -> f32 { 0.5 }
fn default_ssao_bias() -> f32 { 0.025 }
fn default_ssao_power() -> f32 { 2.0 }
fn default_ssao_samples() -> u32 { 32 }
fn default_lod() -> f32 { 10.0 }
fn default_bloom_intensity() -> f32 { 0.4 }
fn default_bloom_threshold() -> f32 { 1.2 }
fn default_ibl_scale() -> f32 { 0.2 }
fn default_volume() -> f32 { 1.0 }

pub fn load_config() -> AppConfig {
    std::fs::read_to_string(CONFIG_PATH)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(cfg: &AppConfig) -> Result<()> {
    let s = serde_json::to_string_pretty(cfg)?;
    std::fs::write(CONFIG_PATH, s)?;
    Ok(())
}
