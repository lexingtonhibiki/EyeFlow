use rodio::source::Source;
use rodio::DeviceSinkBuilder;
use std::f32::consts::PI;
use std::num::NonZero;
use std::time::Duration;

use crate::config::SoundPreset;

const SAMPLE_RATE: u32 = 48000;

pub struct AudioPlayer {
    sink: rodio::MixerDeviceSink,
}

impl AudioPlayer {
    pub fn new() -> Self {
        match DeviceSinkBuilder::open_default_sink() {
            Ok(sink) => Self { sink },
            Err(e) => {
                log::error!("音频输出初始化失败: {}", e);
                let sink = DeviceSinkBuilder::open_default_sink().expect("不可恢复");
                Self { sink }
            }
        }
    }

    pub fn play_preset(&self, preset: &crate::config::SoundPreset) {
        let samples = generate_samples(preset);
        let source = VecSource::new(samples);
        self.sink.mixer().add(source);
    }

    #[allow(dead_code)]
    pub fn preview(&self, preset: &crate::config::SoundPreset) {
        self.play_preset(preset);
    }
}

impl Default for AudioPlayer {
    fn default() -> Self { Self::new() }
}

/// 一键播放预览音效（独立的临时 sink，在后台线程播放）
pub fn preview_sound(preset: &SoundPreset) {
    let samples = generate_samples(preset);
    let preset = preset.clone();
    std::thread::spawn(move || {
        if let Ok(sink) = DeviceSinkBuilder::open_default_sink() {
            let source = VecSource::new(samples);
            sink.mixer().add(source);
            // 估算播放时长，保持线程直至播放完毕
            let dur_secs = match preset {
                SoundPreset::GentleChime => 0.500,
                SoundPreset::SoftTap => 0.080,
                SoundPreset::WaterDrop => 0.300,
                SoundPreset::DigitalDrop => 0.720,
                SoundPreset::TripleBeep => 0.340,
            };
            std::thread::sleep(Duration::from_secs_f32(dur_secs + 0.2));
        }
    });
}

fn generate_samples(preset: &SoundPreset) -> Vec<f32> {
    match preset {
        SoundPreset::GentleChime => build_gentle_chime(),
        SoundPreset::SoftTap => build_soft_tap(),
        SoundPreset::WaterDrop => build_water_drop(),
        SoundPreset::DigitalDrop => build_digital_drop(),
        SoundPreset::TripleBeep => build_triple_beep(),
    }
}

struct VecSource {
    samples: Vec<f32>,
    pos: usize,
}

impl VecSource {
    fn new(samples: Vec<f32>) -> Self { Self { samples, pos: 0 } }
}

impl Iterator for VecSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.pos < self.samples.len() {
            let s = self.samples[self.pos];
            self.pos += 1;
            Some(s)
        } else { None }
    }
}

impl Source for VecSource {
    fn current_span_len(&self) -> Option<usize> { None }
    fn channels(&self) -> rodio::ChannelCount {
        NonZero::new(1).unwrap()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        NonZero::new(SAMPLE_RATE).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        let total = self.samples.len() as u64;
        Some(Duration::from_secs_f64(total as f64 / SAMPLE_RATE as f64))
    }
}

// —— Envelope ——

fn apply_fade_out(samples: &mut [f32], fade_secs: f32) {
    let len = samples.len();
    let fade = (fade_secs * SAMPLE_RATE as f32) as usize;
    let fade = fade.min(len);
    for i in 0..fade {
        samples[len - fade + i] *= 1.0 - (i as f32 / fade as f32);
    }
}

// —— 5 种预设 ——

/// 风铃：1.5kHz→2.5kHz 上行扫频，500ms
fn build_gentle_chime() -> Vec<f32> {
    let dur = 0.500;
    let total = (SAMPLE_RATE as f64 * dur) as usize;
    let mut samples = Vec::with_capacity(total);
    for i in 0..total {
        let t = i as f64 / SAMPLE_RATE as f64;
        let p = t / dur;
        let phase = 2.0 * PI as f64 * t * (1500.0 + 500.0 * p);
        let phase_h = 2.0 * PI as f64 * t * (3000.0 + 1000.0 * p);
        samples.push((0.30 * phase.sin() + 0.12 * phase_h.sin()) as f32);
    }
    apply_fade_out(&mut samples, 0.050);
    samples
}

/// 轻敲：500Hz 短脉冲，80ms
fn build_soft_tap() -> Vec<f32> {
    let total = (SAMPLE_RATE as f64 * 0.080) as usize;
    let mut samples: Vec<f32> = (0..total).map(|i| {
        let t = i as f64 / SAMPLE_RATE as f64;
        (0.35 * (2.0 * PI as f64 * 500.0 * t).sin()) as f32
    }).collect();
    for (i, s) in samples.iter_mut().enumerate() {
        let t = i as f32 / SAMPLE_RATE as f32;
        *s *= (-t / 0.020).exp();
    }
    samples
}

/// 水滴：2kHz→800Hz 下行扫频，300ms
fn build_water_drop() -> Vec<f32> {
    let dur = 0.300;
    let total = (SAMPLE_RATE as f64 * dur) as usize;
    let mut samples = Vec::with_capacity(total);
    for i in 0..total {
        let t = i as f64 / SAMPLE_RATE as f64;
        let p = t / dur;
        let phase = 2.0 * PI as f64 * t * (2000.0 - 600.0 * p);
        samples.push((0.30 * phase.sin()) as f32);
    }
    apply_fade_out(&mut samples, 0.060);
    samples
}

/// 数字降调：1.8kHz→500Hz 三步阶梯，每步 120ms
fn build_digital_drop() -> Vec<f32> {
    let step = (SAMPLE_RATE as f64 * 0.120) as usize;
    let steps = [1800.0, 1150.0, 500.0];
    let total = step * 3;
    let mut samples = Vec::with_capacity(total);
    for (si, &freq) in steps.iter().enumerate() {
        let trans = 440;
        for i in 0..step {
            let t = i as f64 / SAMPLE_RATE as f64;
            let f = if i < trans && si > 0 { steps[si-1] + (freq - steps[si-1]) * (i as f64 / trans as f64) } else { freq };
            let phase = 2.0 * PI as f64 * f * t;
            samples.push((0.30 * phase.sin()) as f32);
        }
    }
    samples
}

/// 三连短哔：1.2kHz 三声，每声 60ms，间隔 80ms
fn build_triple_beep() -> Vec<f32> {
    let pulse = (SAMPLE_RATE as f64 * 0.060) as usize;
    let gap = (SAMPLE_RATE as f64 * 0.080) as usize;
    let total = pulse * 3 + gap * 2;
    let mut samples = Vec::with_capacity(total);
    for n in 0..3 {
        for i in 0..pulse {
            let t = i as f64 / SAMPLE_RATE as f64;
            samples.push((0.30 * (2.0 * PI as f64 * 1200.0 * t).sin()) as f32);
        }
        if n < 2 { samples.extend(std::iter::repeat(0.0f32).take(gap)); }
    }
    let fade = (SAMPLE_RATE as f64 * 0.005) as usize;
    for n in 0..3 {
        let start = (pulse + gap) * n + pulse - fade;
        for j in 0..fade.min(pulse) {
            let idx = start + j;
            if idx < samples.len() {
                samples[idx] *= j as f32 / fade as f32;
            }
        }
    }
    samples
}
