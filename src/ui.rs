//! 设置窗口内容（纯 egui 绘制，不持有窗口；窗口由 app.rs 作为子视口创建）。

use std::time::{Duration, Instant};

use egui::Widget;

use crate::config::{format_hhmm, parse_hhmm, Config, FlowSensitivity, SoundPreset, WallpaperFit};
use crate::stats::Stats;
use crate::tr::{tr, tr_fill, trn};
use crate::wallpaper::{self, WallpaperCache};

/// 设置窗的页签。
///
/// v0.7.0：原先六张卡片排在**一条**滚动流里（状态条钉在顶部，下面 5 张卡片），
/// 找一项设置要在一屏里来回扫。改成页签后，同一个时刻屏幕上只有一类设置，
/// 与「首屏第一眼看到下次休息时间」的引导目标也不再冲突（默认停在「状态」页）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Now,
    Reminder,
    Awareness,
    System,
    About,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 5] = [
        SettingsTab::Now,
        SettingsTab::Reminder,
        SettingsTab::Awareness,
        SettingsTab::System,
        SettingsTab::About,
    ];

    /// 文案在 locale 表里（`tab.*`），见 `SoundPreset::key` 的说明。
    pub fn key(self) -> &'static str {
        match self {
            SettingsTab::Now => "tab.now",
            SettingsTab::Reminder => "tab.reminder",
            SettingsTab::Awareness => "tab.awareness",
            SettingsTab::System => "tab.system",
            SettingsTab::About => "tab.about",
        }
    }

    /// 下一个页签（末尾回到开头）。
    ///
    /// 目前只有演示模式会用它：`EYEFLOW_DEMO_TABS=1` 时设置窗每 2 秒自动翻一页，
    /// 用来录 README 的 GIF——录制机上有别的窗口盖住设置窗时点击送不进去
    /// （见 `.shots/record_gif.py` 的说明），自动翻页让录制完全不依赖鼠标。
    pub fn next(self) -> SettingsTab {
        let all = Self::ALL;
        let i = all.iter().position(|t| *t == self).unwrap_or(0);
        all[(i + 1) % all.len()]
    }
}

pub struct SettingsState {
    /// 正在编辑的副本
    pub draft: Config,
    /// 上次保存的版本，用于判断是否有未保存修改
    pub saved: Config,
    /// 当前页签
    pub tab: SettingsTab,
    pub autostart: bool,
    pub autostart_error: Option<String>,
    pub saved_at: Option<Instant>,
    /// 自定义提示音：选中文件的说明 / 校验错误
    pub custom_sound_info: Option<String>,
    pub custom_sound_error: Option<String>,
    /// 严格模式背景预览用的纹理缓存（随设置窗生命周期）
    pub wallpaper: WallpaperCache,
    /// 配置文件路径的显示串，构造时算一次。
    ///
    /// 旧写法在 `system_card` 里每帧调 `Config::path().display()`，而
    /// `config_dir()` 是 `env::var("APPDATA")` + `PathBuf::from` + `.join`——
    /// 每帧 1 次环境变量查询 + 4 次堆分配，外加一次 `format!`。
    /// 60 fps 下就是 240 次/秒的纯浪费，而路径在进程生命周期内不会变。
    config_path_display: String,
}

impl SettingsState {
    pub fn new(cfg: Config, autostart: bool) -> Self {
        Self {
            draft: cfg.clone(),
            saved: cfg,
            tab: SettingsTab::default(),
            autostart,
            autostart_error: None,
            saved_at: None,
            custom_sound_info: None,
            custom_sound_error: None,
            wallpaper: WallpaperCache::new(),
            config_path_display: tr_fill("ui.config_file", "{path}", Config::path().display()),
        }
    }

    /// “修改即保存”模式下已无调用方（footer 不再依赖它），暂保留供调试/将来使用
    #[allow(dead_code)]
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
    /// 即时保存：每个控件提交时立即发出
    ///
    /// `Box<Config>`：`Config` 有 30 个字段（约 200 B），而 `Vec<SettingsAction>`
    /// 是每帧新建的。不装箱的话这个枚举会被 clippy 的 `large_enum_variant` 判为
    /// 过大（232 B vs 次大的 16 B），而装箱只是多一次已在堆上的 clone。
    Save(Box<Config>),
    RestNow,
    ResetDefaults,
    SetAutostart(bool),
    SetHotkeyEnabled(bool),
    /// 试听预设提示音，携带 draft 里当前的音量与时长
    Preview {
        preset: SoundPreset,
        volume_pct: u32,
        duration_secs: u64,
    },
    /// 试听自定义音频（按文件本身长度播放一次），仅携带音量
    PreviewCustom {
        path: String,
        volume_pct: u32,
    },
    PickCustomSound,
    ClearCustomSound,
    PickWallpaper,
    ClearWallpaper,
    // 未启用 `update-check` 时没有按钮会产出它，但 `runtime.rs` 仍然匹配它
    #[cfg_attr(not(feature = "update-check"), allow(dead_code))]
    CheckUpdateNow,
    OpenConfigDir,
    /// 「系统」卡片首行的语言选择器。语言是进程级全局状态，不能只靠保存配置
    /// 生效——托盘菜单、字体、已渲染的视口都要立刻跟着换。
    ///
    /// 载荷是 `Lang::code()`，一个 `&'static str`（只有 zh-CN / en-US 两个取值），
    /// 所以这里不该是 `String`。
    SetLanguage(&'static str),
}

pub fn show(ui: &mut egui::Ui, s: &mut SettingsState, view: &SettingsView) -> Vec<SettingsAction> {
    let mut actions = Vec::new();
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);

    tab_bar(ui, s);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            match s.tab {
                // 首屏（默认页签）就是「下次休息 14:32」——首次运行会自动打开这个
                // 窗口做引导，第一眼不该是一张要往下滚的设置表单。
                SettingsTab::Now => status_card(ui, view, &mut actions),
                SettingsTab::Reminder => {
                    rhythm_card(ui, s, &mut actions);
                    ui.add_space(4.0);
                    delivery_card(ui, s, view, &mut actions);
                }
                SettingsTab::Awareness => context_card(ui, s, &mut actions),
                SettingsTab::System => system_card(ui, s, view, &mut actions),
                SettingsTab::About => about_card(ui, s, view, &mut actions),
            }
            ui.add_space(8.0);
            footer(ui, s, &mut actions);
        });

    actions
}

/// 页签栏。文案只有两三个字，五个页签中英文都远窄于内容区（宽度闸门会量）。
fn tab_bar(ui: &mut egui::Ui, s: &mut SettingsState) {
    ui.horizontal(|ui| {
        for tab in SettingsTab::ALL {
            if ui.selectable_label(s.tab == tab, tr(tab.key())).clicked() {
                s.tab = tab;
            }
        }
    });
}

fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.group(|ui| {
        ui.set_min_width(ui.available_width());
        // v0.6.1：14.0 → 17.0。理由不是「1.5 px 看不出来」——`app.rs` 的休息面板标题
        // 与欢迎闪屏**早就在用 17.0**，设置窗是全项目唯一的异类。改完三个窗口说
        // 同一套层级语言。⚠️ 字号**不是**字重：egui 0.36.1 没有 `FontWeight`
        // （`epaint-0.36.1/src/text/fonts.rs:27-34`），`strong()` 走的是**颜色**
        // （纯白 255 vs 正文灰 140），两条通道都成立才是这里的层级。
        ui.label(egui::RichText::new(title).strong().size(17.0));
        ui.add_space(2.0);
        add(ui)
    })
    .inner
}

/// 把一段内容缩进一级。
///
/// 过去这里是在字符串里手写全角空格（U+3000）当缩进用。三个问题：
///
/// 1. 全角空格是**真实字符**——复制粘贴会带出去，在日志 / issue 里是隐形噪声；
/// 2. 英文界面里它照占一个字宽但什么都不显示，视觉上「多了个空格子」；
/// 3. 缩进量被写死成字符数，控件字号一改就对不齐。
///
/// `ui.indent` 走的是真实布局缩进，天然跟随 `spacing`。
fn indented<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.indent(ui.next_auto_id(), add).inner
}

/// 一级缩进的近似像素宽度（原来是一个全角空格 U+3000，约 1 em）。
const INDENT: f32 = 12.0;

/// 「标签列」宽度：量出 `label` 在**当前语言 / 当前字号 / 当前 DPI** 下的实际渲染宽度。
///
/// v0.6.1 之前这里是一个常量 `INDENT_WIDE = 5.0 * 12.0 = 60.0`（按「5 个汉字」估的）。
/// 它在**任何**语言下都对不齐，因为它把三件事全写死了：字符数（不是宽度）、字号
/// （egui 0.36.1 的默认 `TextStyle::Body` 是 **13.0 pt**，不是 12），以及漏掉了
/// `pixels_per_point`（本机主屏 125% 缩放，1 逻辑单位 = 1.25 物理 px）。
/// 顺带更正 ADR-0007 §v0.6.1 追加裁决里那组 PIL 数字：PIL 是在 1.0 倍缩放下量的，
/// 所以「5 个汉字 = 60.0 px、中文恰好零错位」这个结论**不成立**——真实排版下
/// 中文标签是 5 × 13.0 × 1.25 ≈ 81 px，错位约 21 px；英文 `Short break interval`
/// 约 138 px，错位约 78 px。**两种语言都错，只是英文更难看。**
///
/// 所以改成**问 egui 要答案**：`layout_no_wrap` 走的是与 `ui.label` 完全相同的
/// 排版路径（`ui.horizontal` 里 `wrap_mode` 是 `Extend`，`max_width = INFINITY`，
/// `halign = Align::LEFT`），量出来的 `galley.size().x` 就是那个 label 实际占掉的
/// 宽度，于是第二行的滑条与第一行的滑条**必然**对齐——与语言、字号、DPI 都无关。
fn label_column_width(ui: &egui::Ui, label: &str) -> f32 {
    let font_id = ui.style().text_styles[&egui::TextStyle::Body].clone();
    let color = ui.visuals().text_color();
    ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font_id, color))
        .size()
        .x
}

/// 状态色：全项目就这三个值，v0.6.1 之前它们以字面量形式散落在 10 处调用点上。
///
/// ⚠️ **不要改成 `visuals.error_fg_color` / `warn_fg_color`**：那两个是**字段**不是
/// 方法（`egui-0.36.1/src/style.rs:1055/1058`），值是 egui 默认的**纯红
/// 纯红 `RGB(255, 0, 0)`**（`:1516-1517` dark / `:1579-1580` light），会把本项目调过
/// 的 `(220, 80, 80)` 换成纯红——那是视觉退化；而且 **egui 根本没有
/// `success_fg_color`**，换过去会变成「错误纯红、成功自定义绿」的不一致。
const COLOR_OK: egui::Color32 = egui::Color32::from_rgb(70, 170, 100);
const COLOR_WARN: egui::Color32 = egui::Color32::from_rgb(230, 150, 40);
const COLOR_ERR: egui::Color32 = egui::Color32::from_rgb(220, 80, 80);

/// 统一的“提交”语义：该帧是否应当把当前改动立即保存。
///
/// egui 0.36 的 `Response` 语义依据（本机源码
/// `egui-0.36.1/src/response.rs`、`context.rs`；`drag_released` 自 0.31 起已更名为
/// `drag_stopped`，0.36 中不存在 `drag_released`）：
/// - `changed()`：控件展示的数据发生变化的帧为 true。复选框 / `selectable_value` 等
///   点击型控件只在值翻转的那一帧为 true；而 Slider / DragValue 在拖动过程中**每一帧**
///   都为 true，直接拿它当提交条件会拖一下存几十次。
/// - `dragged()`：本帧指针正拖动该控件；`context.rs` 在 `PointerEvent::Released` 的那一帧
///   把该标志清回 false。
/// - `drag_stopped()`："The widget was being dragged, but now it has been released."
///   只在松手的那一帧为 true。
///
/// 因此提交条件 = 松手（拖动结束）或（发生变更且不在拖动中）：
/// 滑条 / DragValue 在松手（或键盘改值）时提交，点击型控件在点击帧立即提交。
fn commit(r: &egui::Response) -> bool {
    r.drag_stopped() || (r.changed() && !r.dragged())
}

/// “修改即保存”：把当前 draft 规范化后作为 [`SettingsAction::Save`] 发出。
/// runtime 保存后会回写 `s.saved` 并置 `s.saved_at`（用于 footer 的“已保存 ✓”反馈）。
fn push_save(s: &SettingsState, actions: &mut Vec<SettingsAction>) {
    actions.push(SettingsAction::Save(Box::new(s.draft.clone().sanitized())));
}

fn status_card(ui: &mut egui::Ui, view: &SettingsView, actions: &mut Vec<SettingsAction>) {
    card(ui, tr("card.now"), |ui| {
        ui.horizontal(|ui| {
            ui.label(format!(
                "{} {}{}{}",
                tr("stats.dot"),
                view.state_label,
                tr("stats.separator"),
                view.reminder_line,
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("ui.rest_now")).clicked() {
                    actions.push(SettingsAction::RestNow);
                }
            });
        });
        let st = view.stats;
        let good = COLOR_OK;
        let warn = COLOR_WARN;
        let bad = COLOR_ERR;
        let muted = ui.visuals().weak_text_color();
        // 延后次数按今日累计变色：0 灰 → 1~2 橙 → ≥3 红，让“欠账”一眼可见
        let postponed_color = match st.postponed {
            0 => muted,
            1..=2 => warn,
            _ => bad,
        };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.weak(tr("stats.completed_label"));
            ui.colored_label(
                if st.completed_today() > 0 {
                    good
                } else {
                    muted
                },
                tr_fill("stats.count", "{n}", st.completed_today()),
            );
            ui.weak(tr_fill("stats.natural_label", "{n}", st.completed_natural));
            ui.weak(tr("stats.skipped_label"));
            ui.colored_label(
                if st.skipped > 0 { bad } else { muted },
                tr_fill("stats.count", "{n}", st.skipped),
            );
            ui.weak(tr("stats.postponed_label"));
            ui.colored_label(postponed_color, tr_fill("stats.count", "{n}", st.postponed));
            ui.weak(trn("stats.streak_label", st.streak_days));
        });
    });
}

/// 「连续用屏 N h」滑条的数值显示形态。
///
/// 根因：`egui::Slider` 内部的 `DragValue` 对 `f32` 默认用**两位小数**格式，
/// 于是 2.0 h 渲染成 `2.00 h`——同一张「提醒节奏」卡片里 `15 min` / `30 s` 都是
/// 干净形态，只有这个带尾零。
///
/// 只改**显示**，不动滑条语义：范围仍是 `0.5..=4.0` h、步进仍是 `0.25` h、
/// `Config::long_break_after_secs` 仍是 `u64` 秒（老配置零迁移）。
///
/// 为什么不用「改成分钟为单位的整数滑条」：`0.5 h` 会被迫显示成 `30 min`，
/// 同一个字段在同一个卡片里换单位；而 0.25 h 的粒度（15 min）在分钟滑条上要靠
/// `step_by(15)` 表达，界面上并不能因此变得更好读。**若将来要换整数滑条，应当连同
/// `Config` 字段类型与老配置兼容一起评估，记为 v0.7 候选。**
fn hours_label(v: f64) -> String {
    // 先按两位小数取整（顺带吃掉 f32 累加误差），再去掉尾零与尾随的小数点：
    //   2.00 → "2"    1.00 → "1"    0.50 → "0.5"    0.75 → "0.75"    1.25 → "1.25"
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn rhythm_card(ui: &mut egui::Ui, s: &mut SettingsState, actions: &mut Vec<SettingsAction>) {
    card(ui, tr("card.rhythm"), |ui| {
        if commit(&ui.checkbox(&mut s.draft.enabled, tr("ui.enable_reminders"))) {
            push_save(s, actions);
        }

        let mut min_m = (s.draft.short_break_min_secs / 60) as u32;
        let mut max_m = (s.draft.short_break_max_secs / 60) as u32;
        let mut committed = false;
        ui.horizontal(|ui| {
            ui.label(tr("ui.short_interval"));
            committed |= commit(
                &ui.add(
                    egui::Slider::new(&mut min_m, 5..=90)
                        .suffix(tr("unit.minute"))
                        .text(tr("ui.slider_shortest")),
                ),
            );
        });
        ui.horizontal(|ui| {
            // 对齐上面那行「短休息间隔」标签的实际渲染宽度（v0.6.1：原先是常量
            // `INDENT_WIDE = 60.0`，按 5 个汉字估的，在中英文下都错位）
            //
            // ⚠️ 必须再补一个 `item_spacing.x`：`ui.horizontal` 里普通控件（上面那行
            // 的 label）与 `Space` 的「占位 + 后续控件」之间的间距规则并不对称
            // （egui 的 `Region::next_space` 对两者走的是不同的 `item_spacing`
            // 折半逻辑）。只填实测宽度的话第二行会**正好差一个 `item_spacing.x`**
            // ——截图实测中文差 10 物理 px（卡片里 `show()` 把 spacing 覆盖成
            // (8, 6)，× 1.25 DPI），英文同样差 10 px。
            ui.add_space(
                label_column_width(ui, tr("ui.short_interval")) + ui.spacing().item_spacing.x,
            );
            committed |= commit(
                &ui.add(
                    egui::Slider::new(&mut max_m, 5..=120)
                        .suffix(tr("unit.minute"))
                        .text(tr("ui.slider_longest")),
                ),
            );
        });
        if max_m < min_m {
            max_m = min_m;
        }
        s.draft.short_break_min_secs = min_m as u64 * 60;
        s.draft.short_break_max_secs = max_m as u64 * 60;
        if committed {
            push_save(s, actions);
        }
        ui.weak(tr("ui.weak_interval_random"));

        let mut rest = s.draft.short_break_secs as u32;
        let mut committed = false;
        ui.horizontal(|ui| {
            ui.label(tr("ui.short_duration"));
            committed |=
                commit(&ui.add(egui::Slider::new(&mut rest, 10..=90).suffix(tr("unit.second"))));
        });
        s.draft.short_break_secs = rest as u64;
        if committed {
            push_save(s, actions);
        }

        ui.add_space(4.0);
        if commit(&ui.checkbox(
            &mut s.draft.long_break_enabled,
            tr("ui.long_break_checkbox"),
        )) {
            push_save(s, actions);
        }
        ui.add_enabled_ui(s.draft.long_break_enabled, |ui| {
            let mut after_h = s.draft.long_break_after_secs as f32 / 3600.0;
            let mut long_m = (s.draft.long_break_secs / 60) as u32;
            let mut committed = false;
            ui.horizontal(|ui| {
                ui.label(tr("ui.after_screen"));
                // v0.6.1：长说明从「行末灰字」搬到控件的 hover。英文 213 字符，
                // 在英文布局里折成约 279~294 px 宽、占一屏 15~22% 的灰字，是整窗
                // 最不显眼的一块内容。仓库里已有同款先例：`context_card` 的
                // `FlowSensitivity` 说明（`on_hover_text(tr(f.desc_key()))`）。
                let after_screen = ui.add(
                    egui::Slider::new(&mut after_h, 0.5..=4.0)
                        .step_by(0.25)
                        .suffix(tr("unit.hour"))
                        .custom_formatter(|v, _range| hours_label(v)),
                );
                committed |= commit(&after_screen);
                after_screen.on_hover_text(tr("ui.weak_aoa"));
            });
            // v0.6.1 回归修复：`ui.then_break` + 第二个滑条原本与上面同一行，
            // 整行实测右缘 **753.1 pt**，超出内容区 546.0 pt 达 **+207 pt**。
            // 触发条件是本版自己：`tune_style` 把 `spacing.slider_width` 从 egui
            // 默认的 100.0 抬到 220.0，而**这一行没有任何对齐处理**（不像上面的
            // `Short break interval` 两行那样有 `label_column_width` 兜底）。
            // 拆成两行后每行只剩「标签 + 一个 220 滑条」。
            ui.horizontal(|ui| {
                ui.label(tr("ui.then_break"));
                committed |= commit(
                    &ui.add(egui::Slider::new(&mut long_m, 3..=30).suffix(tr("unit.minute"))),
                );
            });
            s.draft.long_break_after_secs = (after_h * 3600.0).round() as u64;
            s.draft.long_break_secs = long_m as u64 * 60;
            if committed {
                push_save(s, actions);
            }
        });

        ui.add_space(4.0);
        let mut heads = s.draft.heads_up_secs as u32;
        let mut post = (s.draft.postpone_secs / 60) as u32;
        let mut committed = false;
        ui.horizontal(|ui| {
            ui.label(tr("ui.heads_up"));
            committed |=
                commit(&ui.add(egui::Slider::new(&mut heads, 5..=60).suffix(tr("unit.second"))));
        });
        // v0.6.1 回归修复：这一行原本是「预告提前 + 滑条 + 延后一次 + 滑条」**两个
        // 滑条挤一行**。`slider_width = 100` 时它是 510.1 pt（放得下），本版把它抬到
        // 220.0 之后实测右缘 **750.1 pt**，超出 546.0 pt 达 **+204 pt**——截图里
        // `Postpone once` 的滑条直接跑出窗口右边界。拆成两行，每行「标签 + 一个滑条」。
        ui.horizontal(|ui| {
            ui.label(tr("ui.postpone_once"));
            committed |=
                commit(&ui.add(egui::Slider::new(&mut post, 1..=30).suffix(tr("unit.minute"))));
        });
        s.draft.heads_up_secs = heads as u64;
        s.draft.postpone_secs = post as u64 * 60;
        if committed {
            push_save(s, actions);
        }
    });
}

fn delivery_card(
    ui: &mut egui::Ui,
    s: &mut SettingsState,
    view: &SettingsView,
    actions: &mut Vec<SettingsAction>,
) {
    card(ui, tr("card.delivery"), |ui| {
        // v0.6.1：这两条复选框的标签原本把「标题 + 一整句解释」写进同一个 key，
        // 英文分别是 93 / 96 字符，在英文布局里整行冲出内容区右缘被截断
        // （截图实测右缘 750.1 / 763.4 pt > 546.0 pt）。按本版已定的方向——长说明
        // 一律搬进控件的 hover——拆成「短标签 + `ui.weak_*` 说明」，这是第 5、6 条
        // （前 4 条是 AOA 时长、提示音时长、离开判定、热键）。
        let visual = ui.checkbox(&mut s.draft.visual_enabled, tr("ui.visual_checkbox"));
        let visual_committed = commit(&visual);
        visual.on_hover_text(tr("ui.weak_visual"));
        if visual_committed {
            push_save(s, actions);
        }
        ui.add_enabled_ui(s.draft.visual_enabled, |ui| {
            let strict = ui.checkbox(&mut s.draft.strict_mode, tr("ui.strict_checkbox"));
            let strict_committed = commit(&strict);
            strict.on_hover_text(tr("ui.weak_strict"));
            if strict_committed {
                push_save(s, actions);
            }
            ui.add_enabled_ui(s.draft.strict_mode, |ui| wallpaper_section(ui, s, actions));
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if commit(&ui.checkbox(&mut s.draft.sound_enabled, tr("ui.sound"))) {
                push_save(s, actions);
            }
            ui.add_enabled_ui(s.draft.sound_enabled, |ui| {
                // 悬停提示不能省：`Custom` 排到第一位只在**展开下拉框之后**才看得见，
                // 而收起时框里显示的还是当前选中项（默认「风铃」），用户根本不会去点开它。
                egui::ComboBox::from_id_salt("sound_preset")
                    .selected_text(tr(s.draft.sound_preset.key()))
                    .show_ui(ui, |ui| {
                        for p in SoundPreset::ALL {
                            if commit(&ui.selectable_value(
                                &mut s.draft.sound_preset,
                                p,
                                tr(p.key()),
                            )) {
                                push_save(s, actions);
                            }
                        }
                    })
                    .response
                    .on_hover_text(tr("ui.weak_sound_custom"));
                let custom = s.draft.sound_preset == SoundPreset::Custom;
                let can_preview = view.audio_ok && (!custom || s.draft.custom_sound_path.is_some());
                if ui
                    .add_enabled(can_preview, egui::Button::new(tr("ui.preview")))
                    .clicked()
                {
                    // 修复：试听必须携带 draft 里当前的音量与时长（此前用的是已保存配置，
                    // 拖动音量滑条后不点保存、试听音量不会变化）。
                    if custom {
                        if let Some(p) = &s.draft.custom_sound_path {
                            actions.push(SettingsAction::PreviewCustom {
                                path: p.clone(),
                                volume_pct: s.draft.cue_volume_pct,
                            });
                        }
                    } else {
                        actions.push(SettingsAction::Preview {
                            preset: s.draft.sound_preset,
                            volume_pct: s.draft.cue_volume_pct,
                            duration_secs: s.draft.cue_duration_secs,
                        });
                    }
                }
            });
            if !view.audio_ok {
                ui.weak(tr("ui.no_audio_device"));
            }
        });
        ui.add_enabled_ui(s.draft.sound_enabled, |ui| {
            if commit(&ui.checkbox(&mut s.draft.start_cue_enabled, tr("ui.start_cue_checkbox"))) {
                push_save(s, actions);
            }
            // v0.7.0：预告浮窗此前是**静默**弹出的（整条链路只有结束音会响），
            // 不盯着屏幕右下角就不知道提醒来了。默认开，可在本页关掉。
            let heads_up_cue = ui
                .checkbox(
                    &mut s.draft.heads_up_cue_enabled,
                    tr("ui.heads_up_cue_checkbox"),
                )
                .on_hover_text(tr("ui.weak_heads_up_cue"));
            if commit(&heads_up_cue) {
                push_save(s, actions);
            }
        });
        if s.draft.sound_preset == SoundPreset::Custom {
            ui.add_enabled_ui(s.draft.sound_enabled, |ui| custom_sound_row(ui, s, actions));
        }
        ui.add_enabled_ui(s.draft.sound_enabled, |ui| {
            let mut vol = s.draft.cue_volume_pct as f32;
            let mut dur = s.draft.cue_duration_secs as u32;
            let mut committed = false;
            ui.horizontal(|ui| {
                ui.label(tr("ui.cue_volume"));
                committed |= commit(
                    &ui.add(egui::Slider::new(&mut vol, 50.0..=200.0).suffix(tr("unit.percent"))),
                );
            });
            ui.horizontal(|ui| {
                ui.label(tr("ui.cue_duration"));
                // v0.6.1：`weak_sound_advice`（英文 190 字符）与
                // `weak_custom_length`（英文 52 字符）都从行末灰字搬进「提示音时长」
                // 滑条的 hover——两条说明讲的都是这一个控件。原先它们在
                // `ui.horizontal`（Extend）里当行内条目，中文实测右缘 601.5 pt、
                // 英文 639.7 pt，都越过 546.0 pt 的内容区右缘。
                let cue_duration =
                    ui.add(egui::Slider::new(&mut dur, 1..=5).suffix(tr("unit.second")));
                committed |= commit(&cue_duration);
                let custom = s.draft.sound_preset == SoundPreset::Custom;
                if custom {
                    cue_duration.on_hover_text(tr("ui.weak_custom_length"));
                } else {
                    cue_duration.on_hover_text(tr("ui.weak_sound_advice"));
                }
            });
            s.draft.cue_volume_pct = vol as u32;
            s.draft.cue_duration_secs = dur as u64;
            if committed {
                push_save(s, actions);
            }
        });
    });
}

fn custom_sound_row(ui: &mut egui::Ui, s: &mut SettingsState, actions: &mut Vec<SettingsAction>) {
    ui.horizontal(|ui| {
        ui.add_space(INDENT);
        if ui.button(tr("ui.pick_sound")).clicked() {
            actions.push(SettingsAction::PickCustomSound);
        }
        if s.draft.custom_sound_path.is_some() && ui.small_button(tr("ui.clear")).clicked() {
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
            indented(ui, |ui| {
                ui.colored_label(COLOR_ERR, err);
            });
        }
        (None, Some(info), _) => {
            indented(ui, |ui| {
                ui.weak(tr_fill("ui.selected", "{info}", info));
            });
        }
        (None, None, Some(name)) => {
            indented(ui, |ui| {
                ui.weak(tr_fill("ui.current", "{name}", name));
            });
        }
        _ => {
            indented(ui, |ui| {
                ui.weak(tr("ui.weak_sound_formats"));
            });
        }
    }
}

/// 按钮的自然高度 = 文本行高 + 上下 `button_padding`（egui 0.36.1 的按钮就是这么
/// 算的）。现场量而不是写死：中英文两套字体链（`ZH_FONTS` / `EN_FONTS`）的行高不同。
/// 用途见 `wallpaper_section` 里对行高的注释。
fn button_height(ui: &egui::Ui) -> f32 {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let color = ui.visuals().text_color();
    ui.fonts_mut(|f| f.layout_no_wrap("Ag".to_owned(), font_id, color).size().y)
        + 2.0 * ui.spacing().button_padding.y
}

fn wallpaper_section(ui: &mut egui::Ui, s: &mut SettingsState, actions: &mut Vec<SettingsAction>) {
    // ⚠️ 这一行混排了 label / 按钮 / 下拉框，必须**先**把行高抬到按钮高度再开
    // `ui.horizontal`。不这么做的话（v0.7.1 之前）：「背景图片」「自适应」两个
    // label 按行的**初始**高度（`spacing.interact_size.y`，默认 18 pt）居中，
    // 比按钮中心高 6.4 pt；而 ComboBox 的可用区域从被按钮撑高后的 cursor 起算，
    // 又比按钮中心低 6.4 pt——同一行里三层错位，「铺满裁剪」看起来就是偏下。
    let row_h = button_height(ui);
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = row_h;
        ui.horizontal(|ui| {
            ui.add_space(INDENT);
            ui.label(tr("ui.background_image"));
            if ui.button(tr("ui.pick_image")).clicked() {
                actions.push(SettingsAction::PickWallpaper);
            }
            if s.draft.strict_wallpaper_path.is_some() && ui.small_button(tr("ui.clear")).clicked()
            {
                actions.push(SettingsAction::ClearWallpaper);
            }
            ui.label(tr("ui.fit"));
            egui::ComboBox::from_id_salt("wallpaper_fit")
                .selected_text(tr(s.draft.strict_wallpaper_fit.key()))
                .show_ui(ui, |ui| {
                    for f in WallpaperFit::ALL {
                        if commit(&ui.selectable_value(
                            &mut s.draft.strict_wallpaper_fit,
                            f,
                            tr(f.key()),
                        )) {
                            push_save(s, actions);
                        }
                    }
                });
        });
    });
    let Some(path) = s.draft.strict_wallpaper_path.clone() else {
        indented(ui, |ui| {
            ui.weak(tr("ui.weak_no_image"));
        });
        return;
    };
    // 蒙层：浓度 + 渐变（严格模式“渐变半透明”）
    let overlay = wallpaper::OverlayStyle {
        alpha: s.draft.strict_overlay_pct as f32 / 100.0,
        gradient: s.draft.strict_overlay_gradient,
    };
    ui.horizontal(|ui| {
        ui.add_space(INDENT);
        ui.label(tr("ui.overlay_pct"));
        let mut pct = s.draft.strict_overlay_pct as f32;
        let r_vol = ui.add(egui::Slider::new(&mut pct, 0.0..=85.0).suffix(tr("unit.percent")));
        ui.add_space(INDENT);
        let r_grad = ui.checkbox(&mut s.draft.strict_overlay_gradient, tr("ui.gradient"));
        if commit(&r_vol) {
            s.draft.strict_overlay_pct = pct as u32;
            push_save(s, actions);
        }
        if commit(&r_grad) {
            push_save(s, actions);
        }
    });
    let fit = s.draft.strict_wallpaper_fit;
    let ctx = ui.ctx().clone();
    let loaded = s.wallpaper.get(&ctx, Some(&path)).cloned();
    match loaded {
        Some(tex) => {
            ui.horizontal(|ui| {
                ui.add_space(INDENT);
                let width = (ui.available_width() - 8.0).clamp(240.0, 480.0);
                wallpaper::preview(ui, &tex, fit, width, overlay);
            });
            indented(ui, |ui| {
                ui.weak(tr("ui.weak_preview"));
            });
        }
        None => {
            let err = s
                .wallpaper
                .last_error()
                .unwrap_or_else(|| tr("ui.image_loading"))
                .to_string();
            indented(ui, |ui| {
                ui.colored_label(COLOR_ERR, err);
            });
        }
    }
}

fn context_card(ui: &mut egui::Ui, s: &mut SettingsState, actions: &mut Vec<SettingsAction>) {
    card(ui, tr("card.context"), |ui| {
        ui.horizontal(|ui| {
            ui.label(tr("ui.flow_sensitivity"));
            // 短名进选择框、规则进 hover（ADR-0008 §b：英文直译 45 字符会撑爆）
            egui::ComboBox::from_id_salt("flow_sensitivity")
                .selected_text(tr(s.draft.flow_sensitivity.key()))
                .show_ui(ui, |ui| {
                    for f in FlowSensitivity::ALL {
                        if commit(
                            &ui.selectable_value(&mut s.draft.flow_sensitivity, f, tr(f.key()))
                                .on_hover_text(tr(f.desc_key())),
                        ) {
                            push_save(s, actions);
                        }
                    }
                });
        });
        ui.weak(tr("ui.weak_flow"));

        let mut away_m = (s.draft.away_secs / 60) as u32;
        let mut committed = false;
        ui.horizontal(|ui| {
            ui.label(tr("ui.away"));
            // v0.6.1：`weak_away`（英文 171 字符）从行末灰字挂到它描述的那个滑条上
            let away =
                ui.add(egui::Slider::new(&mut away_m, 1..=30).suffix(tr("ui.unit_min_idle")));
            committed |= commit(&away);
            away.on_hover_text(tr("ui.weak_away"));
        });
        s.draft.away_secs = away_m as u64 * 60;
        if committed {
            push_save(s, actions);
        }

        ui.add_space(4.0);
        let mut qs = parse_hhmm(&s.draft.quiet_start);
        let mut qe = parse_hhmm(&s.draft.quiet_end);
        let mut committed = false;
        ui.horizontal(|ui| {
            ui.label(tr("ui.quiet_hours"));
            committed |= time_editor(ui, "qs", &mut qs);
            ui.label(tr("ui.quiet_until"));
            committed |= time_editor(ui, "qe", &mut qe);
            if qs == qe {
                ui.weak(tr("ui.weak_quiet_same"));
            }
        });
        s.draft.quiet_start = format_hhmm(qs);
        s.draft.quiet_end = format_hhmm(qe);
        if committed {
            push_save(s, actions);
        }

        ui.add_space(4.0);
        if commit(&ui.checkbox(&mut s.draft.esc_skip_enabled, tr("ui.esc_skip"))) {
            push_save(s, actions);
        }
    });
}

/// 小时 : 分钟两个 DragValue；返回是否发生了“提交”（见 `commit`）。
fn time_editor(ui: &mut egui::Ui, salt: &str, minutes: &mut u32) -> bool {
    let mut h = *minutes / 60;
    let mut m = *minutes % 60;
    let response = ui.push_id(salt, |ui| {
        let rh = ui.add(
            egui::DragValue::new(&mut h)
                .range(0..=23)
                .speed(0.05)
                .custom_formatter(|v, _| format!("{:02}", v as u32)),
        );
        ui.label(":");
        let rm = ui.add(
            egui::DragValue::new(&mut m)
                .range(0..=59)
                .speed(0.2)
                .custom_formatter(|v, _| format!("{:02}", v as u32)),
        );
        commit(&rh) || commit(&rm)
    });
    *minutes = h * 60 + m;
    response.inner
}

fn system_card(
    ui: &mut egui::Ui,
    s: &mut SettingsState,
    view: &SettingsView,
    actions: &mut Vec<SettingsAction>,
) {
    card(ui, tr("card.system"), |ui| {
        // 语言选择器放在卡片**首行**：英文母语用户装上第一眼看到的是中文的托盘
        // 右键菜单（7 项），而设置窗是首次运行时唯一自动打开的窗口。
        ui.horizontal(|ui| {
            ui.label(tr("ui.language"));
            let mut lang = crate::tr::Lang::from_config(&s.draft.language);
            egui::ComboBox::from_id_salt("language")
                .selected_text(tr(lang.key()))
                .show_ui(ui, |ui| {
                    for l in crate::tr::Lang::ALL {
                        if ui.selectable_value(&mut lang, l, tr(l.key())).clicked()
                            && lang != crate::tr::Lang::from_config(&s.draft.language)
                        {
                            s.draft.language = lang.code().to_string();
                            actions.push(SettingsAction::SetLanguage(lang.code()));
                        }
                    }
                });
        });

        // 开机自启不是 Config 字段（注册表状态），单独即时生效
        let autostart_resp = ui.checkbox(&mut s.autostart, tr("ui.autostart"));
        if commit(&autostart_resp) {
            actions.push(SettingsAction::SetAutostart(s.autostart));
        }
        if let Some(err) = &s.autostart_error {
            ui.colored_label(COLOR_ERR, err);
        }
        let before_hotkey = s.draft.hotkey_enabled;
        ui.horizontal(|ui| {
            // v0.6.1：`weak_hotkey`（英文 158 字符）挂到热键复选框上
            let hotkey_resp = ui
                .checkbox(&mut s.draft.hotkey_enabled, tr("ui.hotkey"))
                .on_hover_text(tr("ui.weak_hotkey"));
            if commit(&hotkey_resp) {
                // 热键要立即注册/注销；draft 的改动同时也即时保存
                actions.push(SettingsAction::SetHotkeyEnabled(s.draft.hotkey_enabled));
                push_save(s, actions);
            }
        });
        // v0.6.1：注册失败那条红字原先与复选框**同一行**（`ui.horizontal` 是
        // Extend，单行不换行），英文实测右缘 653.7 pt > 546.0 pt。挪到行外——
        // 与上面 `autostart_error` 的写法一致：错误信息是竖排的一行，不需要
        // 和它描述的控件并排。
        if s.draft.hotkey_enabled && !view.hotkey_active && before_hotkey {
            ui.colored_label(COLOR_ERR, tr("ui.hotkey_failed"));
        }

        ui.add_space(2.0);
        // 更新检查 + 版本号已搬到「关于」页（v0.7.0）：那里还有作者 / 仓库 / 许可证
        // 链接与求 Star 的一句，见 `about_card`。

        // v0.6.1：配置路径是**全项目唯一无上界的运行期字符串**，而这一行是
        // `ui.horizontal`（Extend，单行不换行）——路径一变长就横向越界。
        // `Label::truncate`（`egui-0.36.1/src/widgets/label.rs:71`）把它压成
        // 一行并省略；`show_tooltip_when_elided` 默认 true（`label.rs:42`），
        // 于是完整路径在悬停时可见。
        ui.horizontal(|ui| {
            egui::Label::new(egui::RichText::from(&s.config_path_display).weak())
                .truncate()
                .ui(ui);
            if ui.small_button(tr("ui.open_dir")).clicked() {
                actions.push(SettingsAction::OpenConfigDir);
            }
        });
    });
}

/// 项目与作者链接（「关于」页）。写成常量而不是散在调用点，方便一处改。
const REPO_URL: &str = "https://github.com/lexingtonhibiki/EyeFlow";
const AUTHOR_URL: &str = "https://github.com/lexingtonhibiki";
const LICENSE_URL: &str = "https://github.com/lexingtonhibiki/EyeFlow/blob/main/LICENSE";

/// 「关于」页：版本、更新检查、作者与仓库链接、求 Star。
///
/// v0.7.0 新增。此前「版本号 + 更新检查」挤在「系统」卡片末尾，既没有作者与
/// 仓库入口，也没有任何「这是什么项目 / 去哪儿反馈」的信息——按高星开源项目的
/// 惯例补齐：关于页是用户主动寻找「这东西是谁做的、去哪儿说话」时唯一会点开的地方。
///
/// **更新只做「检查」**：查到新版本时展示版本号 + 一个通往 GitHub Releases 的
/// 链接，**不自动下载、不自动安装**——下载与否由用户自己决定（README 的隐私一节
/// 也是这么写的）。
fn about_card(
    ui: &mut egui::Ui,
    s: &mut SettingsState,
    view: &SettingsView,
    actions: &mut Vec<SettingsAction>,
) {
    card(ui, tr("card.about"), |ui| {
        ui.label(
            egui::RichText::new(tr_fill(
                "about.version",
                "{ver}",
                crate::update::current_version(),
            ))
            .strong(),
        );
        ui.weak(tr("about.tagline"));
        ui.add_space(6.0);
        ui.weak(tr("about.links"));
        // `horizontal_wrapped`：四条链接在 560 pt 里排不下时可以折行——
        // 这一行没有「横向裁切」的风险，因此不在 layout_width 闸门的管辖内。
        ui.horizontal_wrapped(|ui| {
            ui.hyperlink_to(tr("about.repo"), REPO_URL);
            ui.hyperlink_to(tr("about.author"), AUTHOR_URL);
            ui.hyperlink_to(tr("about.releases"), crate::update::RELEASES_URL);
            ui.hyperlink_to(tr("about.license"), LICENSE_URL);
        });

        ui.add_space(8.0);
        // 更新检查的 TLS 栈随 `update-check` feature 编译（**v0.7.0 起默认开启**，
        // 见 Cargo.toml；`--no-default-features` 可省下约 1.1 MB）。
        // 没编进来的时候**不显示那个永远勾不上的复选框**——给用户一个按了没反应
        // 的开关比不给更糟。
        #[cfg(feature = "update-check")]
        if commit(
            &ui.checkbox(&mut s.draft.update_check_enabled, tr("ui.update_check"))
                .on_hover_text(tr("ui.update_check_detail")),
        ) {
            push_save(s, actions);
        }
        #[cfg(not(feature = "update-check"))]
        ui.weak(tr("ui.update_check_absent"));

        ui.horizontal(|ui| {
            ui.weak(tr_fill(
                "ui.current_version",
                "{ver}",
                crate::update::current_version(),
            ));
            #[cfg(feature = "update-check")]
            if ui.small_button(tr("ui.check_now")).clicked() {
                actions.push(SettingsAction::CheckUpdateNow);
            }
            match view.update_status {
                UpdateStatus::Idle => {}
                UpdateStatus::Checking => {
                    ui.weak(tr("ui.checking"));
                }
                UpdateStatus::UpToDate => {
                    ui.colored_label(COLOR_OK, tr("ui.up_to_date"));
                }
                UpdateStatus::Available { latest, url } => {
                    ui.colored_label(COLOR_WARN, tr_fill("ui.new_version", "{ver}", latest));
                    // 「前往下载」= 打开 Releases 页面，下哪个包、下不下，用户自己定
                    ui.hyperlink_to(tr("ui.open_release"), url);
                }
                UpdateStatus::Failed(e) => {
                    ui.weak(tr_fill("ui.check_failed", "{err}", e));
                }
            }
        });

        ui.add_space(8.0);
        ui.weak(tr("about.star"));
    });
}

fn footer(ui: &mut egui::Ui, s: &mut SettingsState, actions: &mut Vec<SettingsAction>) {
    ui.horizontal(|ui| {
        if ui.button(tr("ui.reset_defaults")).clicked() {
            actions.push(SettingsAction::ResetDefaults);
            // “修改即保存”：draft 换成默认值后也立即保存默认配置
            s.draft = Config::default();
            push_save(s, actions);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(tr("ui.weak_saved_hint"));
            if s.saved_at
                .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
            {
                ui.colored_label(COLOR_OK, tr("ui.saved"));
            }
        });
    });
}

/// 人读时长。小时与分钟是**两条独立 key**，不是在一条文案里塞两个占位符——
/// 英文必须能只输出 `{n} h`（m == 0 时），而 `"{n} h {m} min"` 会显示成
/// 「2 h 0 min」（ADR-0008 §风险清单第 3 条 / T12）。
pub fn human_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        tr_fill("dur.seconds", "{n}", secs)
    } else if secs < 3600 {
        tr_fill("dur.minutes_rounded", "{n}", (secs + 30) / 60)
    } else {
        let hours = tr_fill("dur.hours", "{n}", secs / 3600);
        let minutes = (secs % 3600) / 60;
        if minutes > 0 {
            format!("{hours} {}", tr_fill("dur.minutes", "{n}", minutes))
        } else {
            hours
        }
    }
}

#[cfg(test)]
mod tests {
    use super::hours_label;

    /// 滑条数值的显示形态：整小时不写尾零（`2` 而不是 `2.00`）。
    ///
    /// 根因是 `egui::Slider` 内部的 `DragValue` 对 `f32` 用两位小数格式。
    /// 滑条语义（0.5~4.0 h、步进 0.25 h）不变，只断言显示。
    #[test]
    fn slider_hours_label_drops_trailing_zeros() {
        for (value, want) in [
            (0.5, "0.5"),
            (0.75, "0.75"),
            (1.0, "1"),
            (1.25, "1.25"),
            (1.5, "1.5"),
            (1.75, "1.75"),
            (2.0, "2"),
            (2.25, "2.25"),
            (3.0, "3"),
            (4.0, "4"),
        ] {
            assert_eq!(
                hours_label(value),
                want,
                "「{value} h」显示成 {:?}，应为 {want:?} —— 尾零让同一张卡片里 \
                 其它时长（15 min / 30 s）显得不整齐",
                hours_label(value)
            );
        }
        // 负数与十位整小时不该被 trim 吃掉（trim_end_matches 遇 '.' 会停，
        // 但值得钉住这个性质，将来改实现时不会悄悄回归）。
        assert_eq!(hours_label(10.0), "10");
        assert_eq!(hours_label(0.0), "0");
    }
}
