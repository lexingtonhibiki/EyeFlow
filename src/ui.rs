//! 设置窗口内容（纯 egui 绘制，不持有窗口；窗口由 app.rs 作为子视口创建）。

use std::time::{Duration, Instant};

use crate::config::{format_hhmm, parse_hhmm, Config, FlowSensitivity, SoundPreset, WallpaperFit};
use crate::stats::Stats;
use crate::wallpaper::{self, WallpaperCache};

pub struct SettingsState {
    /// 正在编辑的副本
    pub draft: Config,
    /// 上次保存的版本，用于判断是否有未保存修改
    pub saved: Config,
    pub autostart: bool,
    pub autostart_error: Option<String>,
    pub saved_at: Option<Instant>,
    /// 自定义提示音：选中文件的说明 / 校验错误
    pub custom_sound_info: Option<String>,
    pub custom_sound_error: Option<String>,
    /// 严格模式背景预览用的纹理缓存（随设置窗生命周期）
    pub wallpaper: WallpaperCache,
}

impl SettingsState {
    pub fn new(cfg: Config, autostart: bool) -> Self {
        Self {
            draft: cfg.clone(),
            saved: cfg,
            autostart,
            autostart_error: None,
            saved_at: None,
            custom_sound_info: None,
            custom_sound_error: None,
            wallpaper: WallpaperCache::new(),
        }
    }

    pub fn has_unsaved(&self) -> bool {
        self.draft != self.saved
    }
}

/// 更新检查状态（设置窗展示）
#[derive(Debug, Clone, PartialEq, Default)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available {
        latest: String,
        url: String,
    },
    Failed(String),
}

/// 设置窗口需要展示的只读运行状态
pub struct SettingsView<'a> {
    pub state_label: &'a str,
    /// “下次休息约 12 分钟后 / 即将休息 / 休息中 / 已暂停 / 提醒已关闭”
    pub reminder_line: String,
    /// 全局热键当前是否已注册（显示用）
    pub hotkey_active: bool,
    pub stats: &'a Stats,
    pub audio_ok: bool,
    pub update_status: &'a UpdateStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsAction {
    Save(Config),
    RestNow,
    ResetDefaults,
    SetAutostart(bool),
    SetHotkeyEnabled(bool),
    Preview(SoundPreset),
    PreviewCustom(String),
    PickCustomSound,
    ClearCustomSound,
    PickWallpaper,
    ClearWallpaper,
    CheckUpdateNow,
    OpenConfigDir,
}

pub fn show(ui: &mut egui::Ui, s: &mut SettingsState, view: &SettingsView) -> Vec<SettingsAction> {
    let mut actions = Vec::new();
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            status_card(ui, view, &mut actions);
            ui.add_space(4.0);
            rhythm_card(ui, s);
            ui.add_space(4.0);
            delivery_card(ui, s, view, &mut actions);
            ui.add_space(4.0);
            context_card(ui, s);
            ui.add_space(4.0);
            system_card(ui, s, view, &mut actions);
            ui.add_space(8.0);
            footer(ui, s, &mut actions);
        });

    actions
}

fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.group(|ui| {
        ui.set_min_width(ui.available_width());
        ui.label(egui::RichText::new(title).strong().size(14.0));
        ui.add_space(2.0);
        add(ui)
    })
    .inner
}

fn status_card(ui: &mut egui::Ui, view: &SettingsView, actions: &mut Vec<SettingsAction>) {
    card(ui, "现在", |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("● {} · {}", view.state_label, view.reminder_line));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("立即休息").clicked() {
                    actions.push(SettingsAction::RestNow);
                }
            });
        });
        let st = view.stats;
        let good = egui::Color32::from_rgb(70, 170, 100);
        let warn = egui::Color32::from_rgb(230, 150, 40);
        let bad = egui::Color32::from_rgb(220, 80, 80);
        let muted = ui.visuals().weak_text_color();
        // 延后次数按今日累计变色：0 灰 → 1~2 橙 → ≥3 红，让“欠账”一眼可见
        let postponed_color = match st.postponed {
            0 => muted,
            1..=2 => warn,
            _ => bad,
        };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.weak("今日完成");
            ui.colored_label(
                if st.completed_today() > 0 {
                    good
                } else {
                    muted
                },
                format!("{} 次", st.completed_today()),
            );
            ui.weak(format!("（其中自然休息 {}）· 跳过", st.completed_natural));
            ui.colored_label(
                if st.skipped > 0 { bad } else { muted },
                format!("{} 次", st.skipped),
            );
            ui.weak("· 延后");
            ui.colored_label(postponed_color, format!("{} 次", st.postponed));
            ui.weak(format!("· 连续坚持 {} 天", st.streak_days));
        });
    });
}

fn rhythm_card(ui: &mut egui::Ui, s: &mut SettingsState) {
    card(ui, "提醒节奏", |ui| {
        ui.checkbox(&mut s.draft.enabled, "启用提醒");

        let mut min_m = (s.draft.short_break_min_secs / 60) as u32;
        let mut max_m = (s.draft.short_break_max_secs / 60) as u32;
        ui.horizontal(|ui| {
            ui.label("短休息间隔");
            ui.add(
                egui::Slider::new(&mut min_m, 5..=90)
                    .suffix(" 分钟")
                    .text("最短"),
            );
        });
        ui.horizontal(|ui| {
            ui.label("　　　　　");
            ui.add(
                egui::Slider::new(&mut max_m, 5..=120)
                    .suffix(" 分钟")
                    .text("最长"),
            );
        });
        if max_m < min_m {
            max_m = min_m;
        }
        s.draft.short_break_min_secs = min_m as u64 * 60;
        s.draft.short_break_max_secs = max_m as u64 * 60;
        ui.weak(
            "实际间隔在区间内随机，峰值落在中点（默认 15~25 分钟，峰值 20 分钟 — AOA 20-20-20）。",
        );

        let mut rest = s.draft.short_break_secs as u32;
        ui.horizontal(|ui| {
            ui.label("短休息时长");
            ui.add(egui::Slider::new(&mut rest, 10..=90).suffix(" 秒"));
        });
        s.draft.short_break_secs = rest as u64;

        ui.add_space(4.0);
        ui.checkbox(
            &mut s.draft.long_break_enabled,
            "长休息：连续用屏一段时间后建议离开屏幕",
        );
        ui.add_enabled_ui(s.draft.long_break_enabled, |ui| {
            let mut after_h = s.draft.long_break_after_secs as f32 / 3600.0;
            let mut long_m = (s.draft.long_break_secs / 60) as u32;
            ui.horizontal(|ui| {
                ui.label("连续用屏");
                ui.add(
                    egui::Slider::new(&mut after_h, 0.5..=4.0)
                        .step_by(0.25)
                        .suffix(" 小时"),
                );
                ui.label("后休息");
                ui.add(egui::Slider::new(&mut long_m, 3..=30).suffix(" 分钟"));
            });
            s.draft.long_break_after_secs = (after_h * 3600.0).round() as u64;
            s.draft.long_break_secs = long_m as u64 * 60;
            ui.weak(
                "AOA：连续用屏 2 小时后休息 15 分钟。长休息同样可延后一次；跳过后 10 分钟再提示。",
            );
        });

        ui.add_space(4.0);
        let mut heads = s.draft.heads_up_secs as u32;
        let mut post = (s.draft.postpone_secs / 60) as u32;
        ui.horizontal(|ui| {
            ui.label("预告提前");
            ui.add(egui::Slider::new(&mut heads, 5..=60).suffix(" 秒"));
            ui.label("　延后一次");
            ui.add(egui::Slider::new(&mut post, 1..=30).suffix(" 分钟"));
        });
        s.draft.heads_up_secs = heads as u64;
        s.draft.postpone_secs = post as u64 * 60;
    });
}

fn delivery_card(
    ui: &mut egui::Ui,
    s: &mut SettingsState,
    view: &SettingsView,
    actions: &mut Vec<SettingsAction>,
) {
    card(ui, "提醒方式", |ui| {
        ui.checkbox(
            &mut s.draft.visual_enabled,
            "视觉提醒：右下角预告浮窗 + 居中休息面板（不抢焦点）",
        );
        ui.add_enabled_ui(s.draft.visual_enabled, |ui| {
            ui.checkbox(
                &mut s.draft.strict_mode,
                "严格模式：休息面板改为全屏遮罩（可自选背景图片）",
            );
            ui.add_enabled_ui(s.draft.strict_mode, |ui| wallpaper_section(ui, s, actions));
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut s.draft.sound_enabled, "提示音");
            ui.add_enabled_ui(s.draft.sound_enabled, |ui| {
                egui::ComboBox::from_id_salt("sound_preset")
                    .selected_text(s.draft.sound_preset.label())
                    .show_ui(ui, |ui| {
                        for p in SoundPreset::ALL {
                            ui.selectable_value(&mut s.draft.sound_preset, p, p.label());
                        }
                    });
                let custom = s.draft.sound_preset == SoundPreset::Custom;
                let can_preview = view.audio_ok && (!custom || s.draft.custom_sound_path.is_some());
                if ui
                    .add_enabled(can_preview, egui::Button::new("▶ 试听"))
                    .clicked()
                {
                    if custom {
                        if let Some(p) = &s.draft.custom_sound_path {
                            actions.push(SettingsAction::PreviewCustom(p.clone()));
                        }
                    } else {
                        actions.push(SettingsAction::Preview(s.draft.sound_preset));
                    }
                }
            });
            if !view.audio_ok {
                ui.weak("（未检测到音频输出设备）");
            }
        });
        if s.draft.sound_preset == SoundPreset::Custom {
            ui.add_enabled_ui(s.draft.sound_enabled, |ui| custom_sound_row(ui, s, actions));
        }
        ui.add_enabled_ui(s.draft.sound_enabled, |ui| {
            let mut vol = s.draft.cue_volume_pct as f32;
            let mut dur = s.draft.cue_duration_secs as u32;
            ui.horizontal(|ui| {
                ui.label("提示音音量");
                ui.add(egui::Slider::new(&mut vol, 50.0..=200.0).suffix(" %"));
            });
            ui.horizontal(|ui| {
                ui.label("提示音时长");
                ui.add(egui::Slider::new(&mut dur, 1..=5).suffix(" 秒"));
                if s.draft.sound_preset == SoundPreset::Custom {
                    ui.weak("（自定义音频按文件本身长度播放一次）");
                }
            });
            s.draft.cue_volume_pct = vol as u32;
            s.draft.cue_duration_secs = dur as u64;
        });
        ui.weak(
            "听歌 / 看视频时短促的声音容易被背景声掩蔽：建议把时长调到 2~3 秒（重复呈现更易察觉），\
             音量最高可放大到 200%。全屏游戏 / 视频中声音是唯一提醒通道，会自动至少响 2 秒。",
        );
    });
}

fn custom_sound_row(ui: &mut egui::Ui, s: &mut SettingsState, actions: &mut Vec<SettingsAction>) {
    ui.horizontal(|ui| {
        ui.label("　");
        if ui.button("选择音频文件…").clicked() {
            actions.push(SettingsAction::PickCustomSound);
        }
        if s.draft.custom_sound_path.is_some() && ui.small_button("清除").clicked() {
            actions.push(SettingsAction::ClearCustomSound);
        }
    });
    let fallback_name = s
        .draft
        .custom_sound_path
        .as_deref()
        .and_then(|p| std::path::Path::new(p).file_name())
        .map(|n| n.to_string_lossy().into_owned());
    match (&s.custom_sound_error, &s.custom_sound_info, fallback_name) {
        (Some(err), _, _) => {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), format!("　{err}"));
        }
        (None, Some(info), _) => {
            ui.weak(format!("　已选择：{info}"));
        }
        (None, None, Some(name)) => {
            ui.weak(format!("　当前：{name}"));
        }
        _ => {
            ui.weak("　支持 wav / mp3 / ogg / flac / m4a / aac，时长不超过 5 分钟。");
        }
    }
}

fn wallpaper_section(ui: &mut egui::Ui, s: &mut SettingsState, actions: &mut Vec<SettingsAction>) {
    ui.horizontal(|ui| {
        ui.label("　背景图片");
        if ui.button("选择图片…").clicked() {
            actions.push(SettingsAction::PickWallpaper);
        }
        if s.draft.strict_wallpaper_path.is_some() && ui.small_button("清除").clicked() {
            actions.push(SettingsAction::ClearWallpaper);
        }
        ui.label("自适应");
        egui::ComboBox::from_id_salt("wallpaper_fit")
            .selected_text(s.draft.strict_wallpaper_fit.label())
            .show_ui(ui, |ui| {
                for f in WallpaperFit::ALL {
                    ui.selectable_value(&mut s.draft.strict_wallpaper_fit, f, f.label());
                }
            });
    });
    let Some(path) = s.draft.strict_wallpaper_path.clone() else {
        ui.weak("　未选择图片时使用纯暗色遮罩。支持 png / jpg / webp / bmp / gif。");
        return;
    };
    let fit = s.draft.strict_wallpaper_fit;
    let ctx = ui.ctx().clone();
    let loaded = s.wallpaper.get(&ctx, Some(&path)).cloned();
    match loaded {
        Some(tex) => {
            ui.horizontal(|ui| {
                ui.label("　");
                let width = (ui.available_width() - 8.0).clamp(240.0, 480.0);
                wallpaper::preview(ui, &tex, fit, width, wallpaper::OverlayStyle::default());
            });
            ui.weak("　预览即为全屏遮罩的实际裁剪效果（16:9）；切换“自适应”即时更新。");
        }
        None => {
            let err = s
                .wallpaper
                .last_error()
                .unwrap_or("图片加载中…")
                .to_string();
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), format!("　{err}"));
        }
    }
}

fn context_card(ui: &mut egui::Ui, s: &mut SettingsState) {
    card(ui, "上下文感知", |ui| {
        ui.horizontal(|ui| {
            ui.label("心流灵敏度");
            egui::ComboBox::from_id_salt("flow_sensitivity")
                .selected_text(s.draft.flow_sensitivity.label())
                .show_ui(ui, |ui| {
                    for f in FlowSensitivity::ALL {
                        ui.selectable_value(&mut s.draft.flow_sensitivity, f, f.label());
                    }
                });
        });
        ui.weak("持续快速输入时判定为心流：到点的提醒会等键盘停歇 8 秒后再出现，不会被取消。");

        let mut away_m = (s.draft.away_secs / 60) as u32;
        ui.horizontal(|ui| {
            ui.label("离开判定");
            ui.add(egui::Slider::new(&mut away_m, 1..=30).suffix(" 分钟无操作"));
        });
        s.draft.away_secs = away_m as u64 * 60;
        ui.weak("离开时计时暂停；进入离开状态即视为一次自然休息。全屏游戏 / 演示 / 独占应用中只响提示音，退出后补发预告。");

        ui.add_space(4.0);
        let mut qs = parse_hhmm(&s.draft.quiet_start);
        let mut qe = parse_hhmm(&s.draft.quiet_end);
        ui.horizontal(|ui| {
            ui.label("免打扰时段");
            time_editor(ui, "qs", &mut qs);
            ui.label("到");
            time_editor(ui, "qe", &mut qe);
            if qs == qe {
                ui.weak("（起止相同 = 不启用）");
            }
        });
        s.draft.quiet_start = format_hhmm(qs);
        s.draft.quiet_end = format_hhmm(qe);
    });
}

fn time_editor(ui: &mut egui::Ui, salt: &str, minutes: &mut u32) {
    let mut h = *minutes / 60;
    let mut m = *minutes % 60;
    ui.push_id(salt, |ui| {
        ui.add(
            egui::DragValue::new(&mut h)
                .range(0..=23)
                .speed(0.05)
                .custom_formatter(|v, _| format!("{:02}", v as u32)),
        );
        ui.label(":");
        ui.add(
            egui::DragValue::new(&mut m)
                .range(0..=59)
                .speed(0.2)
                .custom_formatter(|v, _| format!("{:02}", v as u32)),
        );
    });
    *minutes = h * 60 + m;
}

fn system_card(
    ui: &mut egui::Ui,
    s: &mut SettingsState,
    view: &SettingsView,
    actions: &mut Vec<SettingsAction>,
) {
    card(ui, "系统", |ui| {
        let before = s.autostart;
        ui.checkbox(
            &mut s.autostart,
            "开机自启（写入 HKCU\\...\\Run，可在任务管理器“启动应用”中管理）",
        );
        if s.autostart != before {
            actions.push(SettingsAction::SetAutostart(s.autostart));
        }
        if let Some(err) = &s.autostart_error {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
        }
        let before_hotkey = s.draft.hotkey_enabled;
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut s.draft.hotkey_enabled,
                "全局热键 Ctrl+Shift+E = 立即休息",
            );
            if s.draft.hotkey_enabled && !view.hotkey_active && before_hotkey {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 80, 80),
                    "注册失败：组合键可能已被其他程序占用",
                );
            }
        });
        if s.draft.hotkey_enabled != before_hotkey {
            actions.push(SettingsAction::SetHotkeyEnabled(s.draft.hotkey_enabled));
        }
        ui.weak("全局热键由操作系统全局独占，可能与其他软件冲突，可随时关闭；关闭后仍可用托盘菜单“立即休息”。");

        ui.add_space(2.0);
        ui.checkbox(
            &mut s.draft.update_check_enabled,
            "启动时检查更新（每 24 小时至多一次，仅访问 GitHub Releases API，不下载任何文件）",
        );
        ui.horizontal(|ui| {
            ui.weak(format!("当前版本 v{}", crate::update::current_version()));
            if ui.small_button("立即检查").clicked() {
                actions.push(SettingsAction::CheckUpdateNow);
            }
            match view.update_status {
                UpdateStatus::Idle => {}
                UpdateStatus::Checking => {
                    ui.weak("正在检查…");
                }
                UpdateStatus::UpToDate => {
                    ui.colored_label(egui::Color32::from_rgb(70, 170, 100), "已是最新版本 ✓");
                }
                UpdateStatus::Available { latest, url } => {
                    ui.colored_label(
                        egui::Color32::from_rgb(230, 150, 40),
                        format!("有新版本 v{latest}"),
                    );
                    ui.hyperlink_to("打开发布页", url);
                }
                UpdateStatus::Failed(e) => {
                    ui.weak(format!("检查失败：{e}"));
                }
            }
        });

        ui.horizontal(|ui| {
            ui.weak(format!("配置文件：{}", Config::path().display()));
            if ui.small_button("打开目录").clicked() {
                actions.push(SettingsAction::OpenConfigDir);
            }
        });
    });
}

fn footer(ui: &mut egui::Ui, s: &mut SettingsState, actions: &mut Vec<SettingsAction>) {
    ui.horizontal(|ui| {
        if ui.button("恢复默认").clicked() {
            actions.push(SettingsAction::ResetDefaults);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let unsaved = s.has_unsaved();
            if ui
                .add_enabled(
                    unsaved,
                    egui::Button::new(egui::RichText::new("保存").strong()),
                )
                .clicked()
            {
                actions.push(SettingsAction::Save(s.draft.clone().sanitized()));
            }
            if unsaved {
                ui.weak("有未保存的修改");
            } else if s
                .saved_at
                .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
            {
                ui.colored_label(egui::Color32::from_rgb(70, 170, 100), "已保存 ✓");
            }
        });
    });
}

pub fn human_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs} 秒")
    } else if secs < 3600 {
        format!("{} 分钟", (secs + 30) / 60)
    } else {
        format!("{} 小时 {} 分", secs / 3600, (secs % 3600) / 60)
    }
}
