//! Portable foreground session host. The Windows tray host remains unchanged.
use eframe::egui;
use eyeflow_core::{
    config::Config,
    core::{Core, CoreSnapshot, Phase},
    stats::Stats,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(serde::Serialize, serde::Deserialize)]
struct Saved {
    config: Config,
    stats: Stats,
    timer: CoreSnapshot,
}

struct EyeFlow {
    core: Core,
    origin: Instant,
    active: Duration,
    previous: Instant,
    running: bool,
    focus: bool,
    path: PathBuf,
    notice: String,
    last_saved: Instant,
}
impl EyeFlow {
    fn new() -> Self {
        let now = Instant::now();
        let dir = directories::ProjectDirs::from("org", "EyeFlow", "EyeFlowPortable")
            .map(|p| p.data_local_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".eyeflow-portable"));
        let path = dir.join("session.json");
        let core = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Saved>(&b).ok())
            .and_then(|s| Core::restore(s.config.sanitized(), s.stats, s.timer, now))
            .unwrap_or_else(|| Core::new(Config::default(), Stats::default(), now));
        Self {
            core,
            origin: now,
            active: Duration::ZERO,
            previous: now,
            running: false,
            focus: false,
            path,
            notice: String::new(),
            last_saved: now,
        }
    }
    fn save_session(&mut self) {
        let saved = Saved {
            config: self.core.cfg.clone(),
            stats: self.core.stats.clone(),
            timer: self.core.snapshot(self.origin + self.active),
        };
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let bytes = serde_json::to_vec(&saved)?;
            atomicwrites::AtomicFile::new(
                &self.path,
                atomicwrites::OverwriteBehavior::AllowOverwrite,
            )
            .write(|file| std::io::Write::write_all(file, &bytes))?;
            Ok(())
        })();
        if let Err(e) = result {
            self.notice = format!("Cannot save session: {e}");
        }
    }
}
impl Drop for EyeFlow {
    fn drop(&mut self) {
        self.save_session();
    }
}
impl eframe::App for EyeFlow {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let wall_now = Instant::now();
        let delta = wall_now.duration_since(self.previous);
        self.previous = wall_now;
        // Conservative portable baseline: only count foreground, visible app sessions.
        let foreground = ctx.input(|i| {
            i.viewport().focused.unwrap_or(false) && !i.viewport().minimized.unwrap_or(false)
        });
        if self.running && foreground && delta < Duration::from_secs(5) {
            self.active += delta;
        }
        let now = self.origin + self.active;
        if self.running && foreground {
            self.core.tick(now, self.focus);
        }
        if wall_now.duration_since(self.last_saved) >= Duration::from_secs(5) {
            self.save_session();
            self.last_saved = wall_now;
        }
        ctx.set_visuals(egui::Visuals::dark());
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(20.0);
            ui.heading(egui::RichText::new("EyeFlow").size(34.0).color(egui::Color32::from_rgb(121, 222, 187)));
            ui.label("A gentler rhythm for your eyes");
            ui.add_space(24.0);
            // This preview has no native lock/fullscreen evidence. Do not show
            // a frozen automatic schedule as if it were an active countdown.
            if let Phase::Break { ends, is_long, .. } = self.core.phase {
                ui.label(if is_long { "Long break" } else { "Look into the distance" });
                let seconds = ends.saturating_duration_since(now).as_secs();
                ui.label(egui::RichText::new(format!("{:02}:{:02}", seconds / 60, seconds % 60)).size(54.0));
            } else {
                ui.label("Automatic reminders paused");
                ui.label(self.core.automatic_pause_reason(now).unwrap_or(
                    "This preview does not provide native monitoring evidence.",
                ));
                ui.label("Use Rest now to start a manual break.");
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(if self.running { "Pause session" } else { "Start session" }).clicked() { self.running = !self.running; }
                if ui.button("Rest now").clicked() { self.running = true; self.core.start_break_now(now); }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(self.core.can_postpone() && !matches!(self.core.phase, Phase::Idle), egui::Button::new("Postpone 5 min")).clicked() { self.core.postpone(now); }
                if ui.add_enabled(!matches!(self.core.phase, Phase::Idle), egui::Button::new("Skip")).clicked() { self.core.skip(now); }
            });
            ui.checkbox(&mut self.focus, "Do not disturb · cancel an active break");
            ui.add_space(20.0);
            ui.label(format!("Completed today: {}     Postponed: {}", self.core.stats.completed_short + self.core.stats.completed_long, self.core.stats.postponed));
            ui.separator();
            ui.label("Manual-only portable preview: the break timer advances only while this window is reported focused and not minimized. This is not native lock detection. Native idle, fullscreen and session evidence is unavailable, so automatic reminders stay paused.");
            ui.label("Manual break state and local statistics are saved. Unobserved time is not credited as screen use or natural rest. No account or telemetry.");
            if !self.notice.is_empty() { ui.label(&self.notice); }
        });
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}
fn main() -> eframe::Result {
    eframe::run_native(
        "EyeFlow · Portable",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([460.0, 540.0])
                .with_min_inner_size([360.0, 500.0]),
            ..Default::default()
        },
        Box::new(|_| Ok(Box::new(EyeFlow::new()))),
    )
}
