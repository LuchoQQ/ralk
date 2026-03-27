use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

use glam::Vec3;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

// ---------------------------------------------------------------------------
// SoundHandle — opaque index into AudioEngine's sink pool
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SoundHandle(pub usize);

// ---------------------------------------------------------------------------
// AudioEngine
// ---------------------------------------------------------------------------

/// Manages all audio playback via rodio.
///
/// Holds the `OutputStream` that keeps the audio device alive.  Sinks are
/// stored in a pool indexed by `SoundHandle`.  Fire-and-forget one-shot sounds
/// use `Sink::detach()` and leave no handle.
pub struct AudioEngine {
    // Kept alive to prevent the audio device from closing.
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    // Tracked sinks: (sink, intended_volume).  `None` = slot free.
    sinks: Vec<Option<(Sink, f32)>>,
    master_volume: f32,
    muted: bool,
}

impl AudioEngine {
    /// Try to open the default audio output device.  Returns `None` if no device is available.
    pub fn new() -> Option<Self> {
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| log::warn!("Audio disabled: {e}"))
            .ok()?;
        Some(Self {
            _stream: stream,
            stream_handle,
            sinks: Vec::new(),
            master_volume: 1.0,
            muted: false,
        })
    }

    // ---------------------------------------------------------------------------
    // Playback
    // ---------------------------------------------------------------------------

    /// Play a sound file.  If `looping` is true, repeats until stopped.
    /// Returns a handle that can be used to control the sink later.
    pub fn play_sound(&mut self, path: &str, volume: f32, looping: bool) -> Option<SoundHandle> {
        let sink = Sink::try_new(&self.stream_handle)
            .map_err(|e| log::warn!("Could not create sink: {e}"))
            .ok()?;

        let file = File::open(path)
            .map_err(|e| log::warn!("Audio file '{path}' not found: {e}"))
            .ok()?;
        let source = Decoder::new(BufReader::new(file))
            .map_err(|e| log::warn!("Could not decode '{path}': {e}"))
            .ok()?;

        if looping {
            sink.append(source.repeat_infinite());
        } else {
            sink.append(source);
        }
        sink.set_volume(self.effective_volume(volume));

        let idx = self.alloc_slot();
        self.sinks[idx] = Some((sink, volume));
        Some(SoundHandle(idx))
    }

    /// Play a one-shot spatial sound attenuated by distance.
    /// Attenuation: `volume = (1 - distance/max_distance).clamp(0,1)`.
    /// The sink is detached (fire-and-forget) — no handle is returned.
    pub fn play_spatial(&mut self, path: &str, position: Vec3, listener_pos: Vec3, max_distance: f32) {
        let distance = (position - listener_pos).length();
        if distance >= max_distance {
            return;
        }
        let spatial_vol = (1.0 - distance / max_distance).clamp(0.0, 1.0);

        let sink = match Sink::try_new(&self.stream_handle) {
            Ok(s) => s,
            Err(e) => { log::warn!("Could not create impact sink: {e}"); return; }
        };
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => { log::warn!("Audio '{path}' not found: {e}"); return; }
        };
        let source = match Decoder::new(BufReader::new(file)) {
            Ok(s) => s,
            Err(e) => { log::warn!("Could not decode '{path}': {e}"); return; }
        };

        sink.set_volume(self.effective_volume(spatial_vol));
        sink.append(source);
        sink.detach(); // background playback — no handle needed
    }

    // ---------------------------------------------------------------------------
    // Control
    // ---------------------------------------------------------------------------

    /// Stop and free a tracked sink.
    pub fn stop(&mut self, handle: SoundHandle) {
        if let Some(slot) = self.sinks.get_mut(handle.0) {
            if let Some((sink, _)) = slot.take() {
                sink.stop();
            }
        }
    }

    /// Update the intended volume of a tracked sink.
    pub fn set_volume(&mut self, handle: SoundHandle, volume: f32) {
        let effective = self.effective_volume(volume);
        if let Some(Some((sink, stored_vol))) = self.sinks.get_mut(handle.0) {
            *stored_vol = volume;
            sink.set_volume(effective);
        }
    }

    /// Change the playback speed (and therefore pitch) of a tracked sink.
    /// Values > 1.0 raise pitch; < 1.0 lower pitch.  Clamped to ≥ 0.01.
    pub fn set_speed(&self, handle: SoundHandle, speed: f32) {
        if let Some(Some((sink, _))) = self.sinks.get(handle.0) {
            sink.set_speed(speed.max(0.01));
        }
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
        self.reapply_all_volumes();
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.reapply_all_volumes();
    }

    pub fn master_volume(&self) -> f32 { self.master_volume }
    pub fn is_muted(&self) -> bool { self.muted }

    // ---------------------------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------------------------

    fn effective_volume(&self, intended: f32) -> f32 {
        if self.muted { 0.0 } else { intended * self.master_volume }
    }

    fn alloc_slot(&mut self) -> usize {
        // Reuse slots whose sinks have finished playing.
        for (i, slot) in self.sinks.iter_mut().enumerate() {
            if slot.as_ref().map(|(s, _)| s.empty()).unwrap_or(true) {
                *slot = None;
                return i;
            }
        }
        self.sinks.push(None);
        self.sinks.len() - 1
    }

    fn reapply_all_volumes(&self) {
        for slot in &self.sinks {
            if let Some((sink, vol)) = slot {
                sink.set_volume(self.effective_volume(*vol));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sample audio asset generation
// ---------------------------------------------------------------------------

/// Write a minimal 16-bit mono PCM WAV containing a sine wave.
/// Call at startup to ensure sample sounds exist without committing binaries.
fn write_wav_sine(path: &str, frequency: f32, amplitude: f32, duration_secs: f32) -> anyhow::Result<()> {
    const SAMPLE_RATE: u32 = 22050;
    let num_samples = (SAMPLE_RATE as f32 * duration_secs) as u32;
    let data_size = num_samples * 2; // 16-bit = 2 bytes per sample

    let mut f = File::create(path)?;

    // RIFF / WAVE header
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_size).to_le_bytes())?;
    f.write_all(b"WAVE")?;

    // fmt chunk (16-byte PCM descriptor)
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;                        // PCM
    f.write_all(&1u16.to_le_bytes())?;                        // mono
    f.write_all(&SAMPLE_RATE.to_le_bytes())?;
    f.write_all(&(SAMPLE_RATE * 2).to_le_bytes())?;            // byte rate
    f.write_all(&2u16.to_le_bytes())?;                        // block align
    f.write_all(&16u16.to_le_bytes())?;                       // bits per sample

    // data chunk
    f.write_all(b"data")?;
    f.write_all(&data_size.to_le_bytes())?;
    for i in 0..num_samples {
        let t = i as f32 / SAMPLE_RATE as f32;
        // Simple exponential decay envelope for percussive sounds
        let env = (-t * 10.0).exp();
        let sample = (f32::sin(2.0 * std::f32::consts::PI * frequency * t) * amplitude * env * i16::MAX as f32) as i16;
        f.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}

/// Write a looping WAV with a flat (non-decaying) envelope.
fn write_wav_flat(path: &str, gen: impl Fn(f32) -> f32, duration_secs: f32) -> anyhow::Result<()> {
    const SAMPLE_RATE: u32 = 22050;
    let num_samples = (SAMPLE_RATE as f32 * duration_secs) as u32;
    let data_size   = num_samples * 2;

    let mut f = File::create(path)?;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_size).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&SAMPLE_RATE.to_le_bytes())?;
    f.write_all(&(SAMPLE_RATE * 2).to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_size.to_le_bytes())?;
    for i in 0..num_samples {
        let t = i as f32 / SAMPLE_RATE as f32;
        let sample = (gen(t).clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        f.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}

/// engine_loop.wav — sawtooth approximation (harmonics 1–6 at 120 Hz) for motor rumble.
/// Flat envelope; pitch shifted at runtime via `set_speed()`.
fn write_engine_loop(path: &str) -> anyhow::Result<()> {
    let pi2 = std::f32::consts::TAU;
    write_wav_flat(path, |t| {
        // Sawtooth via partial sums of sine series: sin(k*ω*t)/k
        let f0 = 120.0f32;
        let s = (1..=6).map(|k| {
            let k = k as f32;
            f32::sin(pi2 * k * f0 * t) / k
        }).sum::<f32>();
        s * 0.35 // normalise amplitude
    }, 1.0)
}

/// skid.wav — high-frequency buzz with amplitude modulation to simulate tire squeal.
fn write_skid_loop(path: &str) -> anyhow::Result<()> {
    let pi2 = std::f32::consts::TAU;
    write_wav_flat(path, |t| {
        // 850 Hz carrier, 45 Hz AM envelope → squealing tyre
        let am  = 0.5 + 0.5 * f32::sin(pi2 * 45.0 * t);
        let car = f32::sin(pi2 * 850.0 * t)
                + f32::sin(pi2 * 1020.0 * t) * 0.4;
        car * am * 0.4
    }, 0.5)
}

/// wind_loop.wav — layered near-frequency sines for wind/turbulence whoosh.
fn write_wind_loop(path: &str) -> anyhow::Result<()> {
    let pi2 = std::f32::consts::TAU;
    write_wav_flat(path, |t| {
        // Beating pattern: 350, 420, 510, 580 Hz mixed → airy whoosh
        let s = f32::sin(pi2 * 350.0 * t) * 0.25
              + f32::sin(pi2 * 420.0 * t) * 0.20
              + f32::sin(pi2 * 510.0 * t) * 0.15
              + f32::sin(pi2 * 580.0 * t) * 0.10;
        s
    }, 2.0)
}

/// Ensure `assets/sounds/` contains placeholder WAV files for the engine samples.
/// Only creates files that don't already exist.
pub fn ensure_sample_sounds() {
    let _ = std::fs::create_dir_all("assets/sounds");

    // Ambient: low 80 Hz hum (no decay — constant level for looping)
    let ambient_path = "assets/sounds/ambient.wav";
    if !Path::new(ambient_path).exists() {
        const SAMPLE_RATE: u32 = 22050;
        let duration = 2.0f32;
        let num_samples = (SAMPLE_RATE as f32 * duration) as u32;
        let data_size = num_samples * 2;

        if let Ok(mut f) = File::create(ambient_path) {
            let _ = f.write_all(b"RIFF");
            let _ = f.write_all(&(36 + data_size).to_le_bytes());
            let _ = f.write_all(b"WAVE");
            let _ = f.write_all(b"fmt ");
            let _ = f.write_all(&16u32.to_le_bytes());
            let _ = f.write_all(&1u16.to_le_bytes());
            let _ = f.write_all(&1u16.to_le_bytes());
            let _ = f.write_all(&SAMPLE_RATE.to_le_bytes());
            let _ = f.write_all(&(SAMPLE_RATE * 2).to_le_bytes());
            let _ = f.write_all(&2u16.to_le_bytes());
            let _ = f.write_all(&16u16.to_le_bytes());
            let _ = f.write_all(b"data");
            let _ = f.write_all(&data_size.to_le_bytes());
            for i in 0..num_samples {
                let t = i as f32 / SAMPLE_RATE as f32;
                // Soft hum: fundamental at 80 Hz + 2nd harmonic at 0.3 amplitude
                let s = f32::sin(2.0 * std::f32::consts::PI * 80.0 * t) * 0.25
                      + f32::sin(2.0 * std::f32::consts::PI * 160.0 * t) * 0.08;
                let sample = (s * i16::MAX as f32) as i16;
                let _ = f.write_all(&sample.to_le_bytes());
            }
            log::info!("Created {ambient_path}");
        }
    }

    // Impact: percussive thud at 180 Hz with fast decay
    if !Path::new("assets/sounds/impact.wav").exists() {
        write_wav_sine("assets/sounds/impact.wav", 180.0, 0.6, 0.25)
            .unwrap_or_else(|e| log::warn!("Could not create impact.wav: {e}"));
        log::info!("Created assets/sounds/impact.wav");
    }

    // Vehicle audio (Fase 30)
    if !Path::new("assets/sounds/engine_loop.wav").exists() {
        write_engine_loop("assets/sounds/engine_loop.wav")
            .unwrap_or_else(|e| log::warn!("Could not create engine_loop.wav: {e}"));
        log::info!("Created assets/sounds/engine_loop.wav");
    }
    if !Path::new("assets/sounds/skid.wav").exists() {
        write_skid_loop("assets/sounds/skid.wav")
            .unwrap_or_else(|e| log::warn!("Could not create skid.wav: {e}"));
        log::info!("Created assets/sounds/skid.wav");
    }
    if !Path::new("assets/sounds/wind_loop.wav").exists() {
        write_wind_loop("assets/sounds/wind_loop.wav")
            .unwrap_or_else(|e| log::warn!("Could not create wind_loop.wav: {e}"));
        log::info!("Created assets/sounds/wind_loop.wav");
    }
}
