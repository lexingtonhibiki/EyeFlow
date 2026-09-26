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

use crate::config::WallpaperFit;
use crate::core::{Core, Phase};
use crate::runtime::Runtime;
use crate::tips;
use crate::tr::{tr, tr_fill, trn, Lang};
use crate::ui::{self, SettingsView};
use crate::wallpaper::{self, WallpaperCache};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelKind {
    None,
    HeadsUp,
    Break,
    BreakFullscreen,
    Flash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelAction {
    StartNow,
    Postpone,
    Skip,
}

pub struct UiSession<'a> {
    rt: &'a mut Runtime,
    /// 上一次装字体时的语言。切语言 → `ctx.set_fonts()` + 重画。
    /// egui 视口内容每帧现取 `tr()`，只有字体需要显式重装。
    font_lang: Lang,
    panel_epoch: u64,
    panel_kind: PanelKind,
    panel_noactivate_applied: bool,
    /// 会话结束条件满足后再多画几帧，避免阶段切换瞬间反复拆建 GL 上下文
    idle_frames: u32,
    /// 严格模式背景图纹理（随会话生命周期）
    wallpaper: WallpaperCache,
}

impl<'a> UiSession<'a> {
    pub fn new(cc: &eframe::CreationContext<'_>, rt: &'a mut Runtime) -> Self {
        set_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_theme(egui::ThemePreference::System);
        cc.egui_ctx.style_mut_of(egui::Theme::Dark, tune_style);
        cc.egui_ctx.style_mut_of(egui::Theme::Light, tune_style);
        crate::event::set_ui_context(Some(cc.egui_ctx.clone()));
        Self {
            rt,
            font_lang: crate::tr::language(),
            panel_epoch: 0,
            panel_kind: PanelKind::None,
            panel_noactivate_applied: false,
            idle_frames: 0,
            wallpaper: WallpaperCache::new(),
        }
    }

    fn show_panel(&mut self, ctx: &egui::Context, now: Instant) {
        let core = &self.rt.core;
        // 休息完成的“欢迎回来”闪屏（独立于阶段机）
        let flash = core.cfg.visual_enabled && core.flash_active(now);
        let kind = if flash {
            PanelKind::Flash
        } else {
            match core.phase {
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
        // ⚠️ 故意**不翻译**：这串标题是 `apply_noactivate` 的 `FindWindowW` 查找键，
        // 一旦随语言变化，首帧的 `WS_EX_NOACTIVATE` 就会失效，浮窗开始抢焦点。
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
            PanelKind::Flash => {
                let (w, h) = (420.0, 132.0);
                base.with_inner_size([w, h])
                    .with_position([(monitor.x - w) / 2.0, (monitor.y - h) / 2.0])
            }
            PanelKind::None => unreachable!(),
        };

        // 严格模式背景图 + 蒙层样式：在借用 rt 之前取好（克隆都是 Arc / Copy 级别）
        let wall = if kind == PanelKind::BreakFullscreen {
            let path = self.rt.core.cfg.strict_wallpaper_path.clone();
            let fit = self.rt.core.cfg.strict_wallpaper_fit;
            let overlay = wallpaper::OverlayStyle {
                alpha: self.rt.core.cfg.strict_overlay_pct as f32 / 100.0,
                gradient: self.rt.core.cfg.strict_overlay_gradient,
            };
            self.wallpaper
                .get(ctx, path.as_deref())
                .cloned()
                .map(|tex| (tex, fit, overlay))
        } else {
            None
        };

        let rt = &mut *self.rt;
        let noactivate_applied = &mut self.panel_noactivate_applied;
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            if !*noactivate_applied {
                *noactivate_applied = apply_noactivate(&title);
            }
            let action = match kind {
                PanelKind::HeadsUp => draw_heads_up(ui, &rt.core, now),
                PanelKind::Break => draw_break(ui, &rt.core, now, false, None),
                PanelKind::BreakFullscreen => draw_break(
                    ui,
                    &rt.core,
                    now,
                    true,
                    wall.as_ref().map(|(t, f, o)| (t, *f, *o)),
                ),
                PanelKind::Flash => draw_flash(ui, &rt.core),
                PanelKind::None => None,
            };
            match action {
                Some(PanelAction::StartNow) => rt.on_manual_break_start(now),
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
        let state_label = tr(rt.core.state.key());
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
            // ⚠️ **未验证**：切语言后这个原生窗口标题
            // 能否真的跟着变，取决于 egui 在「其他属性
            // 没变、只有 title 变了」时是否仍然下发
            // `ViewportCommand::Title`。ADR-0008 把这一项列为需人工验证，
            // 本轮**没有**验证过。若切语言后标题停在旧语言，
            // 症状只是标题栏文字不跟随，窗口内容与托盘
            // 都不受影响——修法是显式发
            // `ctx.send_viewport_cmd_to(id, ViewportCommand::Title(title))`。
            .with_title(tr("app.window_title_settings"))
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
            update_status: &rt.update_status,
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

        // 语言切换：托盘由 `Runtime::apply_language` 立即重写，egui 侧只需重装字体
        if crate::tr::language() != self.font_lang {
            self.font_lang = crate::tr::language();
            set_fonts(ctx);
        }

        if self.rt.quit {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
            return;
        }
        let now = Instant::now();
        if self.rt.needs_ui(now) {
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
                // 👀 不进 locale：实测它在 msyh / segoeui / seguisym 里都没有 glyph，
                // 靠的是 egui `default_fonts` 自带的 emoji 字体（T11c）。留在代码里
                // 才能让 T11a 只查真正的界面文案。
                ui.label(RichText::new("\u{1F440}").size(26.0));
                ui.vertical(|ui| {
                    let title = if is_long {
                        tr("app.panel_long_title").to_string()
                    } else {
                        tr_fill("app.panel_short_title", "{n}", remaining.as_secs() + 1)
                    };
                    ui.label(RichText::new(title).size(17.0).strong());
                    ui.weak(if is_long {
                        tr_fill("app.panel_long_sub", "{n}", core.cfg.long_break_secs / 60)
                    } else {
                        tr("app.panel_short_sub").to_string()
                    });
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new(tr("app.start_now")).strong())
                    .clicked()
                {
                    action = Some(PanelAction::StartNow);
                }
                let postpone_label = tr_fill("app.postpone", "{n}", core.cfg.postpone_secs / 60);
                if ui
                    .add_enabled(core.can_postpone(), egui::Button::new(postpone_label))
                    .on_hover_text(if is_long {
                        tr("app.hover_postpone_long")
                    } else {
                        tr("app.hover_postpone")
                    })
                    .on_disabled_hover_text(tr("app.hover_postpone"))
                    .clicked()
                {
                    action = Some(PanelAction::Postpone);
                }
                if ui.button(tr("app.skip")).clicked() {
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

fn draw_flash(ui: &mut egui::Ui, core: &Core) -> Option<PanelAction> {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(ui.visuals().panel_fill)
                .stroke(egui::Stroke::new(1.0, accent(ui).gamma_multiply(0.6)))
                .inner_margin(18.0),
        )
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                // 👏 同上：留在代码里（T11a 不查 emoji，T11c 查它有系统字体兜底）
                ui.label(
                    RichText::new(format!(
                        "\u{1F44F} {}",
                        trn("app.flash_title", core.stats.completed_today())
                    ))
                    .size(17.0)
                    .strong(),
                );
                ui.weak(tr("app.flash_sub"));
            });
        });
    None
}

fn draw_break(
    ui: &mut egui::Ui,
    core: &Core,
    now: Instant,
    fullscreen: bool,
    wallpaper_tex: Option<(&egui::TextureHandle, WallpaperFit, wallpaper::OverlayStyle)>,
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
        // 有壁纸时 paint_fullscreen 会自己铺底色，这里只是兜底
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
            if let Some((tex, fit, overlay)) = wallpaper_tex {
                // 自选背景 + 可调蒙层先于文字绘制（同一图层按调用顺序叠放）
                wallpaper::paint_fullscreen(ui, tex, fit, ui.clip_rect(), overlay);
            }
            ui.vertical_centered(|ui| {
                if fullscreen {
                    ui.add_space((ui.available_height() * 0.26).max(0.0));
                }
                ui.set_max_width(if fullscreen { 560.0 } else { 440.0 });
                ui.label(
                    RichText::new(if is_long {
                        tr("app.break_title_long")
                    } else {
                        tr("app.break_title_short")
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
                if ui.button(tr("app.continue_work")).clicked() {
                    action = Some(PanelAction::Skip);
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new(tr("app.auto_complete"))
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
    // v0.6.1：滑条默认只有 100.0（`egui-0.36.1/src/style.rs:1460`），而本项目
    // `tune_style` 从未设过它 —— 设置窗每一行右侧因此留着约 244 px 的死区
    // （一行两滑条：标签 + 两个 100 px 滑条，右边全空）。220.0 把那条死区
    // 压到 30 px 以内，滑条本身仍然明显短于行宽，动起来仍然好按。
    style.spacing.slider_width = 220.0;
    style.visuals.widgets.noninteractive.corner_radius = 6.0.into();
    style.visuals.widgets.inactive.corner_radius = 6.0.into();
    style.visuals.widgets.hovered.corner_radius = 6.0.into();
    style.visuals.widgets.active.corner_radius = 6.0.into();
}

/// 一个待加载的字体文件：egui `FontDefinitions` 里的名字 + 磁盘路径。
struct FontFile {
    key: &'static str,
    path: &'static str,
}

const fn font(key: &'static str, path: &'static str) -> FontFile {
    FontFile { key, path }
}

/// 中文模式的字体链。
///
/// - `msyh.ttc` 实测 19,704,352 B = 18.79 MiB，是**会话内峰值**的最大单项。
/// - `seguisym.ttf` 是**尾部回退**（v0.6 引入）：实测 `✓`(U+2713) 与 `▶`(U+25B6)
///   在 `msyh.ttc` 里**没有 glyph**，而它们出现在「已保存 ✓」和「▶ 试听」两处界面上。
///   也就是说 v0.5.2 的中文界面今天就在显示豆腐块。egui 没有系统级字体回退，
///   缺字就是方块。**这一条现在比任何时候都硬**——v0.6 逐个字体查过 cmap，
///   `msyhl` / `simhei` 都没能兜住这两个字符。
///
/// 顺序即优先级：egui 对每个字符从 family 列表头开始找第一个有 glyph 的字体，
/// 所以 `msyh` 拿绝大多数字符，`seguisym` 只在前面都没有时兜底。
///
/// **v0.6.1 删掉了 `msyhl.ttc`（原 `cjk_light`）与 `simhei.ttf`（原 `cjk_alt`）**，
/// 实测省 21,935,404 B = 20.92 MiB，链体积 **−49.68%**。三条依据都是 fontTools
/// 直读系统字体 cmap 的实测（[ADR-0007](docs/adr/0007-v0.6-scope.md) §v0.6.1 追加裁决
/// 的 Sources 逐条列了数字）：
///
/// 1. 本项目 `locales/*.toml` 全部去重字符里，「**需要 `msyhl` 或 `simhei` 才画得
///    出来**」的字符数是 **0**；
/// 2. `msyh` 对 CJK 统一表意文字区 U+4E00–U+9FFF 的覆盖是 **20992/20992 = 100%**，
///    而 `simhei` 是 **20902/20992**——**比 `msyh` 还少 90 个码位，它从来不是超集**；
/// 3. `(msyhl ∪ simhei) − (msyh ∪ seguisym)` 的 BMP 码位恰为 **29 个**（25 个私有区
///    PUA U+E78D–U+E864，加 `ﬁ`(U+FB01) / `ﬂ`(U+FB02) / `﴾`(U+FD3E) / `﴿`(U+FD3F)），
///    **非 BMP 的额外码位 0 个**。
///
/// 也就是说，**保留这两个字体的唯一理由是「glyph 覆盖兜底」，从来不是字重**
/// （egui 0.36.1 没有字重概念，见 `EN_FONTS` 上方注释）——而那条理由已被实测证伪，
/// 两个条目随之删除。将来若要引入 BMP 之外的 CJK 扩展区（Ext-B 起）字符，需要的是
/// **新字体**，不是这两个。
///
/// ⚠️ 这条改动省的是**会话内峰值**与**每会话分配 churn**，**不是常驻内存**：
/// 每份字体字节在 egui 内部存两份，峰值杠杆约 43.9 MB；每会话一次 44 MB 的
/// `fs::read`（`build_font_defs`）+ 约 44 MB 的 `memcpy`，砍完各减半。
/// 关闭设置窗之后那部分残渣里它们只占约 1.3 MB——**用户在任务管理器里看不出变化，
/// 发布说明里也不许写「降低常驻内存」**。
const ZH_FONTS: &[FontFile] = &[
    font("cjk", "C:\\Windows\\Fonts\\msyh.ttc"),
    font("sym", "C:\\Windows\\Fonts\\seguisym.ttf"),
];

/// 英文模式的字体链（ADR-0008 判决二）。
///
/// 跳过 18.8 MB 的 msyh，改用 Segoe UI：Vista 起每台 Windows 都有，
/// 带 ClearType hinting，是系统自己用的字族，总计约 1.9 MB。
///
/// **epaint 0.36.1 不认识字重**（这是 v0.6 撤销 ADR-0008 一处裁决的原因，
/// 务必读完再改字体链）：`FontId` 只有 `{ size, family }` 两个字段
/// （`epaint-0.36.1/src/text/fonts.rs:27-34`，源码里留着
/// `// TODO(emilk): weight (bold), italics, …`），全文件 `FontWeight` 出现 0 次。
/// `FontFamily` 的列表被当作**逐字符的 glyph 回退链**，不是「常规体 + 粗体」这一对。
/// 两个直接后果：
///
/// 1. **不要为了「要粗体」往链里加粗体字体文件**：加进去 `segoeuib.ttf` 只会在
///    `segoeui.ttf` 缺某个 glyph 时被当作第二道回退，947 KB 换不到任何字重。
/// 2. 层级只能靠**字号 / 颜色 / 间距**建立。
///
/// ⚠️ **「`strong()` 什么也不做」这句话是错的，v0.6.1 已更正（本注释此前写的正是
/// 那句错话）**。
/// 「字重那条」结论成立，但**「颜色那条」同样成立，而它一直在生效**——`strong()`
/// 改的是**颜色**，不是字重：
/// `egui-0.36.1/src/widget_text.rs:252` 置 `strong = true` → `:483-485`
/// `if self.strong { Some(visuals.strong_text_color()) }` → `style.rs:1147`
/// `strong_text_color()` = `widgets.active.text_color()` → `style.rs:1710`
/// `active.fg_stroke = Color32::WHITE`，而 `style.rs:1686`
/// `noninteractive.fg_stroke = from_gray(140)`。**标题是纯白 255、正文灰 140，
/// 亮度差 82%——标题的颜色层级早就成立了。** 被浪费的是**字号**那条通道
/// （`ui.rs` 的卡片标题曾是全项目唯一的 14.0，v0.6.1 归到 17.0）。
///
/// 真正的字重阶梯需要 epaint 上游支持 `FontWeight`，属 v0.7 议题。
const EN_FONTS: &[FontFile] = &[
    font("latin", "C:\\Windows\\Fonts\\segoeui.ttf"),
    font("sym", "C:\\Windows\\Fonts\\seguisym.ttf"),
];

/// 按语言构建 `FontDefinitions`。
///
/// 从 egui 默认字体（`default_fonts`）起步而不是清空：`app.rs:288` 的 👀 与
/// `app.rs:347` 的 👏 靠默认字体里的 emoji 字形渲染，**不能关掉 default_fonts**。
/// 实测这两个 emoji 在 `msyh` / `segoeui` / `seguisym` 里**都没有**。
fn build_font_defs(lang: Lang) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let chain = match lang {
        Lang::ZhCn => ZH_FONTS,
        Lang::EnUs => EN_FONTS,
    };
    let mut loaded = 0usize;
    for f in chain {
        match std::fs::read(f.path) {
            Ok(bytes) => {
                fonts.font_data.insert(
                    f.key.to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(bytes)),
                );
                loaded += 1;
            }
            Err(e) => log::warn!("字体 {} 加载失败: {e}", f.path),
        }
    }
    if loaded == 0 {
        log::warn!("未找到任何界面字体，退回 egui 内置字体（中文/符号会显示为方块）");
        return fonts;
    }
    // 每个可用字体都插到 family 首位：egui 从头找第一个有该字符 glyph 的字体，
    // 所以整条链都能兜底，而链首（msyh / segoeui）承担绝大多数字符。
    let mut order: Vec<String> = chain
        .iter()
        .filter(|f| fonts.font_data.contains_key(f.key))
        .map(|f| f.key.to_owned())
        .collect();
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let mut list = std::mem::take(&mut order);
        let entry = fonts.families.entry(family).or_default();
        list.append(entry);
        *entry = list;
    }
    fonts
}

/// 把当前语言的字体装进 egui；缺字体时退回 egui 默认字体并记录告警。
fn set_fonts(ctx: &egui::Context) {
    let lang = crate::tr::language();
    ctx.set_fonts(build_font_defs(lang));
    // 字体换了必须重画一遍，否则当帧仍用旧字体的缓存纹理
    ctx.request_repaint();
    if lang == Lang::ZhCn {
        log::debug!("界面字体：微软雅黑 + Segoe UI Symbol 回退");
    } else {
        log::debug!("界面字体：Segoe UI + Segoe UI Symbol 回退（跳过 18.8 MB 中文字体）");
    }
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
