//! 提示音：运行时合成，无音频文件依赖；无输出设备时静默降级，绝不 panic。
//!
//! 默认“风铃”按 spec N4 设计在 1.5~2.5 kHz 上行扫频区（高于多数游戏音效的主能量区）。
//! `Mixer::add` 会把音源统一重采样到设备采样率，这里固定按 48 kHz 合成即可。

use std::f64::consts::PI;
use std::num::NonZero;
use std::time::Duration;

use rodio::source::Source;
use rodio::{DeviceSinkBuilder, MixerDeviceSink};

use crate::config::SoundPreset;

const SAMPLE_RATE: u32 = 48_000;

pub struct AudioPlayer {
    sink: Option<MixerDeviceSink>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        match DeviceSinkBuilder::open_default_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                Self { sink: Some(sink) }
            }
            Err(e) => {
                log::warn!("音频输出不可用（{e}），提示音将静默");
                Self { sink: None }
            }
        }
    }

    pub fn available(&self) -> bool {
        self.sink.is_some()
    }

    /// 播放提示音：按音量百分比(50~200)放大，并把短图案循环铺满设定时长。
    pub fn play_cue(&self, preset: SoundPreset, volume_pct: u32, duration_secs: u64) {
        if let Some(sink) = &self.sink {
            sink.mixer()
                .add(VecSource::new(render(preset, volume_pct, duration_secs)));
        }
    }
}

struct VecSource {
    samples: Vec<f32>,
    pos: usize,
}

impl VecSource {
    fn new(samples: Vec<f32>) -> Self {
        Self { samples, pos: 0 }
    }
}

impl Iterator for VecSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let s = *self.samples.get(self.pos)?;
        self.pos += 1;
        Some(s)
    }
}

impl Source for VecSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        NonZero::new(1).unwrap()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        NonZero::new(SAMPLE_RATE).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            self.samples.len() as f64 / SAMPLE_RATE as f64,
        ))
    }
}

pub fn generate(preset: SoundPreset) -> Vec<f32> {
    match preset {
        SoundPreset::GentleChime => gentle_chime(),
        SoundPreset::SoftTap => soft_tap(),
        SoundPreset::WaterDrop => water_drop(),
        SoundPreset::DigitalDrop => digital_drop(),
        SoundPreset::TripleBeep => triple_beep(),
    }
}

/// 把提示音渲染到目标时长与音量。
///
/// 背景音乐 / 视频场景下，零点几秒的单次短音几乎必然被掩蔽：以 0.15 s 间隔
/// 循环短图案直到铺满 `duration_secs`(1~5 s)，对应“重复呈现”的可察觉性原则；
/// `volume_pct` 50~200，>100 为主动放大(基准峰值 0.4，2× 仍留有余量，末端限幅)。
pub fn render(preset: SoundPreset, volume_pct: u32, duration_secs: u64) -> Vec<f32> {
    let base = generate(preset);
    let target = (duration_secs.clamp(1, 10) as f64 * SAMPLE_RATE as f64) as usize;
    let gap = (0.15 * SAMPLE_RATE as f64) as usize;
    let mut out = Vec::with_capacity(target);
    while out.len() < target {
        if !out.is_empty() {
            let pad = gap.min(target - out.len());
            out.extend(std::iter::repeat_n(0.0f32, pad));
        }
        let take = base.len().min(target - out.len());
        out.extend_from_slice(&base[..take]);
    }
    let volume = (volume_pct.clamp(10, 400) as f32 / 100.0).min(2.0);
    for s in out.iter_mut() {
        *s = (*s * volume).clamp(-0.95, 0.95);
    }
    out
}

// ------------------------------------------------------------ 合成工具

/// 按瞬时频率函数积分相位生成正弦扫频，避免 `sin(2π·f(t)·t)` 的频率失真。
fn sweep(dur: f64, amp: f64, freq_at: impl Fn(f64) -> f64) -> Vec<f32> {
    let total = (SAMPLE_RATE as f64 * dur) as usize;
    let dt = 1.0 / SAMPLE_RATE as f64;
    let mut phase = 0.0f64;
    let mut out = Vec::with_capacity(total);
    for i in 0..total {
        let t = i as f64 * dt;
        phase += 2.0 * PI * freq_at(t / dur) * dt;
        out.push((amp * phase.sin()) as f32);
    }
    out
}

fn fade_in_out(samples: &mut [f32], fade_in: f64, fade_out: f64) {
    let n = samples.len();
    let fi = ((fade_in * SAMPLE_RATE as f64) as usize).min(n);
    let fo = ((fade_out * SAMPLE_RATE as f64) as usize).min(n);
    for (i, s) in samples.iter_mut().take(fi).enumerate() {
        *s *= i as f32 / fi.max(1) as f32;
    }
    for i in 0..fo {
        samples[n - 1 - i] *= i as f32 / fo.max(1) as f32;
    }
}

fn exp_decay(samples: &mut [f32], tau: f64) {
    for (i, s) in samples.iter_mut().enumerate() {
        *s *= (-(i as f64 / SAMPLE_RATE as f64) / tau).exp() as f32;
    }
}

fn silence(dur: f64) -> Vec<f32> {
    vec![0.0; (SAMPLE_RATE as f64 * dur) as usize]
}

// ------------------------------------------------------------ 预设

/// 风铃：1.5 → 2.5 kHz 上行扫频 + 二次谐波，500 ms。
fn gentle_chime() -> Vec<f32> {
    let base = sweep(0.5, 0.28, |p| 1500.0 + 1000.0 * p);
    let harm = sweep(0.5, 0.10, |p| 3000.0 + 2000.0 * p);
    let mut out: Vec<f32> = base.iter().zip(&harm).map(|(a, b)| a + b).collect();
    fade_in_out(&mut out, 0.01, 0.12);
    out
}

/// 轻敲：1.6 kHz 极短敲击，指数衰减，90 ms。
fn soft_tap() -> Vec<f32> {
    let mut out = sweep(0.09, 0.40, |_| 1600.0);
    exp_decay(&mut out, 0.02);
    out
}

/// 水滴：2.4 → 1.5 kHz 下行扫频，300 ms。
fn water_drop() -> Vec<f32> {
    let mut out = sweep(0.3, 0.30, |p| 2400.0 - 900.0 * p);
    fade_in_out(&mut out, 0.005, 0.08);
    out
}

/// 数字降调：2.2 → 1.6 → 1.2 kHz 三级阶梯，每级 120 ms，级间平滑。
fn digital_drop() -> Vec<f32> {
    let steps = [2200.0, 1600.0, 1200.0];
    let mut out = Vec::new();
    for (i, &f) in steps.iter().enumerate() {
        let prev = if i == 0 { f } else { steps[i - 1] };
        let mut seg = sweep(0.12, 0.30, move |p| {
            if p < 0.15 {
                prev + (f - prev) * (p / 0.15)
            } else {
                f
            }
        });
        fade_in_out(&mut seg, 0.0, if i == 2 { 0.05 } else { 0.0 });
        out.extend(seg);
    }
    out
}

/// 三连短哔：1.8 kHz × 3，每声 60 ms，间隔 80 ms。
fn triple_beep() -> Vec<f32> {
    let mut out = Vec::new();
    for i in 0..3 {
        let mut pulse = sweep(0.06, 0.30, |_| 1800.0);
        fade_in_out(&mut pulse, 0.004, 0.008);
        out.extend(pulse);
        if i < 2 {
            out.extend(silence(0.08));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_generate_bounded_audio() {
        for p in SoundPreset::ALL {
            let s = generate(p);
            assert!(!s.is_empty());
            assert!(s.iter().all(|v| v.abs() <= 1.0), "{p:?} clipped");
            let dur = s.len() as f64 / SAMPLE_RATE as f64;
            assert!(dur > 0.05 && dur < 1.0, "{p:?} dur {dur}");
        }
    }

    #[test]
    fn chime_starts_and_ends_silent() {
        let s = gentle_chime();
        assert!(s[0].abs() < 1e-3);
        assert!(s[s.len() - 1].abs() < 1e-3);
    }

    #[test]
    fn render_reaches_target_duration_by_repeating() {
        for p in SoundPreset::ALL {
            for secs in [1u64, 3, 5] {
                let s = render(p, 100, secs);
                let expect = secs as usize * SAMPLE_RATE as usize;
                assert_eq!(s.len(), expect, "{p:?} @{secs}s");
                assert!(s.iter().all(|v| v.abs() <= 1.0));
            }
        }
    }

    #[test]
    fn render_scales_volume_and_limits() {
        let quiet = render(SoundPreset::GentleChime, 50, 1);
        let loud = render(SoundPreset::GentleChime, 200, 1);
        let peak_quiet = quiet.iter().fold(0.0f32, |a, v| a.max(v.abs()));
        let peak_loud = loud.iter().fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(
            peak_loud > peak_quiet * 3.0,
            "loud {peak_loud} vs quiet {peak_quiet}"
        );
        assert!(peak_loud <= 0.95, "limiter");
        // 极端值被夹紧而不是 panic
        assert_eq!(
            render(SoundPreset::SoftTap, 0, 0).len(),
            SAMPLE_RATE as usize
        );
        assert_eq!(
            render(SoundPreset::SoftTap, 9999, 99).len(),
            10 * SAMPLE_RATE as usize
        );
    }
}
