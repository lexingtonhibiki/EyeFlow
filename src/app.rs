//! UI 会话（docs/adr/0006 修订版）。
//!
//! 只有在需要显示窗口时才进入 eframe；根视口是一个 1×1、放在屏幕外的“锚点”窗口，
//! 只为让 egui pass 每帧运行；预告浮窗、休息面板、设置窗都是按需创建的子视口。
//! 浮窗每次出现都换新的 `ViewportId`（新的原生窗口），这样 `with_active(false)` 每次
//! 都生效，再加上 `WS_EX_NOACTIVATE`，做到“看得见、点得动、但永远不抢焦点”。
//! 所有窗口都关掉后会话结束，GL 上下文随之释放（实测 NVIDIA 机器上一个 glow
//! 上下文就要 160 MB，常驻不可接受）。

use std::time::{Duration, Instant};

use egui::{Color32, RichText, ViewportBuilder, ViewportCommand, ViewportId};

use crate::core::{Core, Phase};
use crate::runtime::Runtime;
use crate::tips;
use crate::ui::{self, SettingsView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelKind {
    None,
    HeadsUp,
    Break,
    BreakFullscreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelAction {
    StartNow,
    Postpone,
    Skip,
}

pub struct UiSession<'a> {
    rt: &'a mut Runtime,
    panel_epoch: u64,
    panel_kind: PanelKind,
    panel_noactivate_applied: bool,
    /// 会话结束条件满足后再多画几帧，避免阶段切换瞬间反复拆建 GL 上下文
    idle_frames: u32,
}

impl<'a> UiSession<'a> {
    pub fn new(cc: &eframe::CreationContext<'_>, rt: &'a mut Runtime) -> Self {
        install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_theme(egui::ThemePreference::System);
        cc.egui_ctx.style_mut_of(egui::Theme::Dark, tune_style);
        cc.egui_ctx.style_mut_of(egui::Theme::Light, tune_style);
        crate::event::set_ui_context(Some(cc.egui_ctx.clone()));
        Self {
            rt,
            panel_epoch: 0,
            panel_kind: PanelKind::None,
            panel_noactivate_applied: false,
            idle_frames: 0,
        }
    }

    fn show_panel(&mut self, ctx: &egui::Context, now: Instant) {
        let core = &self.rt.core;
        let kind = match core.phase {
            Phase::Idle => PanelKind::None,
            _ if !core.cfg.visual_enabled => PanelKind::None,
            Phase::HeadsUp { .. } => PanelKind::HeadsUp,
            Phase::Break { .. } => {
                if core.cfg.strict_mode {
                    PanelKind::BreakFullscreen
                } else {
                    PanelKind::Break
                }
            }
        };
        if kind == PanelKind::None {
            self.panel_kind = PanelKind::None;
            return;
        }
        if kind != self.panel_kind {
            self.panel_epoch += 1;
            self.panel_kind = kind;
            self.panel_noactivate_applied = false;
        }

        let id = ViewportId::from_hash_of(("eyeflow-panel", self.panel_epoch));
        let title = format!("EyeFlow Reminder {}", self.panel_epoch);
        let monitor = monitor_size(ctx);
        let base = ViewportBuilder::default()
            .with_title(&title)
            .with_decorations(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_active(false)
            .with_resizable(false);
        let builder = match kind {
            PanelKind::HeadsUp => {
                let (w, h) = (400.0, 168.0);
                base.with_inner_size([w, h])
                    .with_position([monitor.x - w - 24.0, monitor.y - h - 84.0])
            }
            PanelKind::Break => {
                let (w, h) = (500.0, 360.0);
                base.with_inner_size([w, h])
                    .with_position([(monitor.x - w) / 2.0, (monitor.y - h) / 2.0])
            }
            PanelKind::BreakFullscreen => base.with_fullscreen(true),
            PanelKind::None => unreachable!(),
        };

        let rt = &mut *self.rt;
        let noactivate_applied = &mut self.panel_noactivate_applied;
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            if !*noactivate_applied {
                *noactivate_applied = apply_noactivate(&title);
            }
            let action = match kind {
                PanelKind::HeadsUp => draw_heads_up(ui, &rt.core, now),
                PanelKind::Break => draw_break(ui, &rt.core, now, false),
                PanelKind::BreakFullscreen => draw_break(ui, &rt.core, now, true),
                PanelKind::None => None,
            };
            match action {
                Some(PanelAction::StartNow) => rt.core.start_break_now(now),
                Some(PanelAction::Postpone) => {
                    rt.core.postpone(now);
                }
                Some(PanelAction::Skip) => rt.core.skip(now),
                None => {}
            }
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        });
    }

    fn show_settings(&mut self, ctx: &egui::Context, now: Instant) {
        let rt = &mut *self.rt;
        if rt.settings.is_none() {
            return;
        }
        let reminder_line = rt.reminder_line(now);
        let state_label = rt.core.state.label();
        let audio_ok = rt.audio.available();
        let hotkey_active = rt.hotkey_active();
        let focus = std::mem::take(&mut rt.settings_focus);
        let Some(state) = rt.settings.as_mut() else {
            return;
        };
        let id = ViewportId::from_hash_of("eyeflow-settings");
        let monitor = monitor_size(ctx);
        // 用户要求默认约 700×780 物理像素：除以缩放得到逻辑尺寸，再按屏幕余量夹紧
        let ppp = ctx.pixels_per_point().max(0.5);
        let (w, h) = (700.0 / ppp, (780.0 / ppp).min(monitor.y - 60.0).max(480.0));
        let builder = ViewportBuilder::default()
            .with_title("EyeFlow 设置")
            .with_inner_size([w, h])
            .with_min_inner_size([420.0, 420.0])
            .with_position([(monitor.x - w) / 2.0, (monitor.y - h) / 2.0])
            .with_resizable(true);

        let view = SettingsView {
            state_label,
            reminder_line,
            hotkey_active,
            stats: &rt.core.stats,
            audio_ok,
        };

        let (actions, close) = ctx.show_viewport_immediate(id, builder, |ui, _class| {
            if focus {
                ui.ctx().send_viewport_cmd(ViewportCommand::Focus);
            }
            let close = ui.input(|i| i.viewport().close_requested());
            let actions = egui::CentralPanel::default()
                .frame(egui::Frame::central_panel(ui.style()).inner_margin(12.0))
                .show(ui, |ui| ui::show(ui, state, &view))
                .inner;
            (actions, close)
        });

        for action in actions {
            rt.apply_settings_action(action, now);
        }
        if close {
            rt.settings = None;
        }
    }
}

impl eframe::App for UiSession<'_> {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        self.rt.step(now);

        if self.rt.quit {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            return;
        }
        if self.rt.needs_ui() {
            self.idle_frames = 0;
        } else {
            self.idle_frames += 1;
            if self.idle_frames >= 3 {
                ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
                return;
            }
        }
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        self.show_panel(&ctx, now);
        self.show_settings(&ctx, now);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        crate::event::set_ui_context(None);
    }
}

// ---------------------------------------------------------------------- 绘制

fn draw_heads_up(ui: &mut egui::Ui, core: &Core, now: Instant) -> Option<PanelAction> {
    let Phase::HeadsUp {
        started,
        ends,
        is_long,
    } = core.phase
    else {
        return None;
    };
    let remaining = ends.saturating_duration_since(now);
    let total = ends
        .saturating_duration_since(started)
        .max(Duration::from_secs(1));
    let frac = 1.0 - remaining.as_secs_f32() / total.as_secs_f32();
    let mut action = None;

    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(ui.visuals().panel_fill)
                .stroke(egui::Stroke::new(1.0, accent(ui).gamma_multiply(0.6)))
                .inner_margin(16.0),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("👀").size(26.0));
                ui.vertical(|ui| {
                    let title = if is_long {
                        "连续用屏 2 小时了，该离开屏幕一会儿".to_string()
                    } else {
                        format!("{} 秒后休息一下", remaining.as_secs() + 1)
                    };
                    ui.label(RichText::new(title).size(17.0).strong());
                    ui.weak(if is_long {
                        format!("{} 分钟长休息即将开始", core.cfg.long_break_secs / 60)
                    } else {
                        "看向 6 米外，多眨几次眼".to_string()
                    });
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("现在开始").strong()).clicked() {
                    action = Some(PanelAction::StartNow);
                }
                let postpone_label = format!("延后 {} 分钟", core.cfg.postpone_secs / 60);
                if ui
                    .add_enabled(core.can_postpone(), egui::Button::new(postpone_label))
                    .on_disabled_hover_text(if is_long {
                        "长休息不可延后（连续用屏 2 小时是底线）"
                    } else {
                        "每次提醒只能延后一次"
                    })
                    .clicked()
                {
                    action = Some(PanelAction::Postpone);
                }
                if ui.button("跳过").clicked() {
                    action = Some(PanelAction::Skip);
                }
            });
            ui.add_space(8.0);
            ui.add(
                egui::ProgressBar::new(frac)
                    .desired_height(4.0)
                    .fill(accent(ui)),
            );
        });
    action
}

fn draw_break(
    ui: &mut egui::Ui,
    core: &Core,
    now: Instant,
    fullscreen: bool,
) -> Option<PanelAction> {
    let Phase::Break {
        started,
        ends,
        is_long,
    } = core.phase
    else {
        return None;
    };
    let remaining = ends.saturating_duration_since(now);
    let total = ends
        .saturating_duration_since(started)
        .max(Duration::from_secs(1));
    let frac = 1.0 - remaining.as_secs_f32() / total.as_secs_f32();
    let mut action = None;

    let fill = if fullscreen {
        Color32::from_rgb(14, 18, 26)
    } else {
        ui.visuals().panel_fill
    };
    let fg = if fullscreen {
        Color32::from_gray(230)
    } else {
        ui.visuals().text_color()
    };
    let accent_color = if fullscreen {
        Color32::from_rgb(90, 170, 220)
    } else {
        accent(ui)
    };

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(fill).inner_margin(24.0))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                if fullscreen {
                    ui.add_space((ui.available_height() * 0.26).max(0.0));
                }
                ui.set_max_width(if fullscreen { 560.0 } else { 440.0 });
                ui.label(
                    RichText::new(if is_long {
                        "长休息 · 离开屏幕一会儿"
                    } else {
                        "看向远处"
                    })
                    .size(if fullscreen { 26.0 } else { 20.0 })
                    .strong()
                    .color(fg),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(mmss(remaining))
                        .size(if fullscreen { 104.0 } else { 68.0 })
                        .strong()
                        .color(accent_color),
                );
                ui.add_space(6.0);
                ui.add(
                    egui::ProgressBar::new(frac)
                        .desired_width(if fullscreen { 460.0 } else { 380.0 })
                        .desired_height(6.0)
                        .fill(accent_color),
                );
                ui.add_space(14.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(tips::pick(core.tip_index))
                            .size(if fullscreen { 18.0 } else { 15.0 })
                            .color(fg),
                    )
                    .wrap(),
                );
                ui.add_space(18.0);
                if ui.button("继续工作").clicked() {
                    action = Some(PanelAction::Skip);
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new("倒计时结束会自动完成并记入今日统计")
                        .size(12.0)
                        .color(fg.gamma_multiply(0.6)),
                );
            });
        });
    action
}

fn mmss(d: Duration) -> String {
    let s = d.as_secs() + u64::from(d.subsec_millis() > 0);
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn accent(ui: &egui::Ui) -> Color32 {
    ui.visuals().selection.bg_fill
}

fn tune_style(style: &mut egui::Style) {
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.visuals.widgets.noninteractive.corner_radius = 6.0.into();
    style.visuals.widgets.inactive.corner_radius = 6.0.into();
    style.visuals.widgets.hovered.corner_radius = 6.0.into();
    style.visuals.widgets.active.corner_radius = 6.0.into();
}

/// 加载系统中文字体（微软雅黑），缺失时退回 egui 默认字体并记录告警。
fn install_fonts(ctx: &egui::Context) {
    let candidates = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\msyhl.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "cjk".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, "cjk".to_owned());
            }
            ctx.set_fonts(fonts);
            return;
        }
    }
    log::warn!("未找到中文字体，界面中文可能显示为方块");
}

/// 根视口所在显示器的逻辑尺寸；拿不到时退回主显示器物理尺寸 / 缩放。
fn monitor_size(ctx: &egui::Context) -> egui::Vec2 {
    if let Some(size) = ctx.input(|i| i.viewport().monitor_size) {
        if size.x > 100.0 && size.y > 100.0 {
            return size;
        }
    }
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let ppp = ctx.pixels_per_point().max(0.5);
    egui::vec2(w as f32 / ppp, h as f32 / ppp)
}

/// 给浮窗加上 WS_EX_NOACTIVATE：点击按钮也不会把焦点从用户的应用抢走。
fn apply_noactivate(title: &str) -> bool {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };
    let wide = HSTRING::from(title);
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(wide.as_ptr())) else {
            return false;
        };
        if hwnd.0.is_null() {
            return false;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let wanted = ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
        if wanted != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
        }
        true
    }
}
