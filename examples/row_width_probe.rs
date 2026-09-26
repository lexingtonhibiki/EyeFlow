//! 无头布局探针：量出设置窗里几行控件的**实测右缘**（逻辑 pt）。
//!
//! 闸门 B（`right_edge_gate`）扫的是**截图**，回答「像素有没有漏出去」；这个探针
//! 回答「为什么会漏出去」——把同一行交给**真正的 egui 布局器**，读回
//! `Ui::min_rect().max.x`。它不重画、不截图，因此能在提交前就给出精确数字，
//! 而不是靠目视。
//!
//! 复刻的是 `src/ui.rs` 的真实环境：
//! - 视口内容宽 560 pt → 卡片内可用宽 **546.0 pt**（探针把外框设成 546.0，
//!   于是「右缘 ≤ 546.0」是一个直接的判据，不需要再做偏移换算）；
//! - `settings::show` 开头把 `item_spacing` 覆盖成 (8, 6)（`ui.rs:121`）；
//! - `card` 用 `ui.group` + `set_min_width(available_width)`（`ui.rs:147-148`）；
//! - `tune_style` 的 `slider_width = 220.0`（`src/app.rs:502`）；
//! - 字体链与 `build_font_defs` 完全一致（中文 msyh+seguisym，英文 segoeui+seguisym）。
//!
//! # 用法
//!
//! ```text
//! cargo run --release --example row_width_probe -- <zh-CN|en-US>
//! ```
//!
//! 故意放在 `examples/` 而不是 `tests/`：它要读系统字体文件、且结论随
//! `slider_width` 这样的设计常量变，属于设计验收工具而不是回归测试。

use std::sync::Arc;

use egui::FontFamily;

/// 内容区右缘（逻辑 pt）。超过就是越界。
const CONTENT_W: f32 = 546.0;
const SLIDER_W: f32 = 220.0;

fn main() {
    let lang = std::env::args().nth(1).unwrap_or_else(|| "en-US".into());
    let zh = lang.starts_with("zh");
    println!(
        "\n=== row_width_probe [{lang}] 内容区 {CONTENT_W} pt / slider_width {SLIDER_W} pt ==="
    );

    let chain: &[(&str, &str)] = if zh {
        &[
            ("cjk", r"C:\Windows\Fonts\msyh.ttc"),
            ("sym", r"C:\Windows\Fonts\seguisym.ttf"),
        ]
    } else {
        &[
            ("latin", r"C:\Windows\Fonts\segoeui.ttf"),
            ("sym", r"C:\Windows\Fonts\seguisym.ttf"),
        ]
    };

    let mut fonts = egui::FontDefinitions::default();
    let mut order = Vec::new();
    for (key, path) in chain {
        let bytes = std::fs::read(path).expect("读不到界面字体");
        fonts.font_data.insert(
            (*key).to_owned(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        order.push((*key).to_owned());
    }
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let mut list = std::mem::take(&mut order);
        let entry = fonts.families.entry(family).or_default();
        list.append(entry);
        *entry = list;
    }

    let ctx = egui::Context::default();
    ctx.set_fonts(fonts);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.slider_width = SLIDER_W;
    });

    // locale 表**只读一次**：早先的写法每次查键都重读一遍磁盘。
    let locale = tr(zh);
    let t = |k: &str| -> String {
        for line in locale.lines() {
            if let Some(rest) = line.strip_prefix(&format!("\"{k}\" = \"")) {
                return rest.trim_end().trim_end_matches('"').to_owned();
            }
        }
        panic!("locale 缺键 {k}");
    };

    // 改前的长标签**故意硬编码**，不进 locale：它们是回归基线，
    // 不该再是产品文案（v0.6.1 已把解释搬进 `ui.weak_visual` / `ui.weak_strict`）。
    let (old_visual, old_strict) = if zh {
        (
            "视觉提醒：右下角预告浮窗 + 居中休息面板（不抢焦点）",
            "严格模式：休息面板改为全屏遮罩（可自选背景图片）",
        )
    } else {
        (
                       "Visual reminder: heads-up window at the bottom right + centered break panel (never steals focus)",
            "Strict mode: the break panel becomes a full-screen overlay (with your own background image)",
        )
    };

    // 每项：名称 + 是否计入闸门 + 一个闭包（在卡片内容区里摆出那一行，返回该行右缘）
    let cases: Vec<Case<'_>> = vec![
        (
            "短休息间隔（两滑条，有 label_column_width 对齐）",
            true,
            Box::new(|ui: &mut egui::Ui| {
                let (mut mn, mut mx) = (15.0f32, 25.0f32);
                let r1 = ui
                    .horizontal(|ui| {
                        ui.label(t("ui.short_interval"));
                        ui.add(egui::Slider::new(&mut mn, 5.0..=90.0).suffix(t("unit.minute")));
                        ui.min_rect().max.x
                    })
                    .inner;
                let w =
                    label_column_width(ui, &t("ui.short_interval")) + ui.spacing().item_spacing.x;
                let r2 = ui
                    .horizontal(|ui| {
                        ui.add_space(w);
                        ui.add(egui::Slider::new(&mut mx, 5.0..=120.0).suffix(t("unit.minute")));
                        ui.min_rect().max.x
                    })
                    .inner;
                r1.max(r2)
            }),
        ),
        (
            "长休息：After + 滑条 + of screen time, rest for + 滑条（v0.6.1 改前）",
            false,
            Box::new(|ui: &mut egui::Ui| {
                let (mut a_h, mut long_m) = (2.0f32, 15.0f32);
                ui.horizontal(|ui| {
                    ui.label(t("ui.after_screen"));
                    ui.add(
                        egui::Slider::new(&mut a_h, 0.5..=4.0)
                            .step_by(0.25)
                            .suffix(t("unit.hour"))
                            .custom_formatter(|v, _| fmt_h(v)),
                    );
                    ui.label(t("ui.then_break"));
                    ui.add(egui::Slider::new(&mut long_m, 3.0..=30.0).suffix(t("unit.minute")));
                    ui.min_rect().max.x
                })
                .inner
            }),
        ),
        (
            "长休息：After + 滑条（v0.6.1 改后，第 1 行）",
            true,
            Box::new(|ui: &mut egui::Ui| {
                let mut a_h = 2.0f32;
                ui.horizontal(|ui| {
                    ui.label(t("ui.after_screen"));
                    ui.add(
                        egui::Slider::new(&mut a_h, 0.5..=4.0)
                            .step_by(0.25)
                            .suffix(t("unit.hour"))
                            .custom_formatter(|v, _| fmt_h(v)),
                    );
                    ui.min_rect().max.x
                })
                .inner
            }),
        ),
        (
            "长休息：of screen time, rest for + 滑条（v0.6.1 改后，第 2 行）",
            true,
            Box::new(|ui: &mut egui::Ui| {
                let mut long_m = 15.0f32;
                ui.horizontal(|ui| {
                    ui.label(t("ui.then_break"));
                    ui.add(egui::Slider::new(&mut long_m, 3.0..=30.0).suffix(t("unit.minute")));
                    ui.min_rect().max.x
                })
                .inner
            }),
        ),
        (
            "预告提前 + 延后一次 两个滑条一行（v0.6.1 改前）",
            false,
            Box::new(|ui: &mut egui::Ui| {
                let (mut heads, mut post) = (45.0f32, 5.0f32);
                ui.horizontal(|ui| {
                    ui.label(t("ui.heads_up"));
                    ui.add(egui::Slider::new(&mut heads, 5.0..=60.0).suffix(t("unit.second")));
                    ui.add_space(12.0);
                    ui.label(t("ui.postpone_once"));
                    ui.add(egui::Slider::new(&mut post, 1.0..=30.0).suffix(t("unit.minute")));
                    ui.min_rect().max.x
                })
                .inner
            }),
        ),
        (
            "预告提前 + 滑条（v0.6.1 改后，第 1 行）",
            true,
            Box::new(|ui: &mut egui::Ui| {
                let mut heads = 45.0f32;
                ui.horizontal(|ui| {
                    ui.label(t("ui.heads_up"));
                    ui.add(egui::Slider::new(&mut heads, 5.0..=60.0).suffix(t("unit.second")));
                    ui.min_rect().max.x
                })
                .inner
            }),
        ),
        (
            "延后一次 + 滑条（v0.6.1 改后，第 2 行）",
            true,
            Box::new(|ui: &mut egui::Ui| {
                let mut post = 5.0f32;
                ui.horizontal(|ui| {
                    ui.label(t("ui.postpone_once"));
                    ui.add(egui::Slider::new(&mut post, 1.0..=30.0).suffix(t("unit.minute")));
                    ui.min_rect().max.x
                })
                .inner
            }),
        ),
        (
            "视觉提醒（v0.6.1 改前，整句塞进标签）",
            false,
            Box::new(move |ui: &mut egui::Ui| {
                let mut visual = true;
                ui.horizontal(|ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut visual, old_visual);
                        ui.min_rect().max.x
                    })
                    .inner
                })
                .inner
            }),
        ),
        (
            "严格模式（v0.6.1 改前，整句塞进标签）",
            false,
            Box::new(move |ui: &mut egui::Ui| {
                let mut strict = false;
                ui.horizontal(|ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut strict, old_strict);
                        ui.min_rect().max.x
                    })
                    .inner
                })
                .inner
            }),
        ),
        (
            "视觉提醒（v0.6.1 改后，短标签）",
            true,
            Box::new(|ui: &mut egui::Ui| {
                let mut visual = true;
                ui.horizontal(|ui| {
                    ui.checkbox(&mut visual, t("ui.visual_checkbox"));
                    ui.min_rect().max.x
                })
                .inner
            }),
        ),
        (
            "严格模式（v0.6.1 改后，短标签）",
            true,
            Box::new(|ui: &mut egui::Ui| {
                let mut strict = false;
                ui.horizontal(|ui| {
                    ui.checkbox(&mut strict, t("ui.strict_checkbox"));
                    ui.min_rect().max.x
                })
                .inner
            }),
        ),
    ];

    // 屏幕矩形直接设成内容区本身，于是「右缘 ≤ 546.0」是一个直接判据，
    // 不需要再做任何偏移换算（`run_ui` 给出的 `Ui` 铺满整个 screen rect）。
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(CONTENT_W, 4000.0),
        )),
        ..Default::default()
    };
    let out: std::cell::RefCell<Vec<(String, f32)>> = Default::default();
    let _ = ctx.run_ui(raw, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
        out.replace_with(|_| {
            let mut v = Vec::new();
            // `card` 的等价物：group + set_min_width(available_width)
            ui.group(|ui| {
                ui.set_min_width(ui.available_width());
                for (name, _current, f) in &cases {
                    v.push(((*name).to_owned(), f(ui)));
                }
            });
            v
        });
    });
    let results = out.into_inner();

    let mut over = 0;
    for ((name, current, _), (_, right)) in cases.iter().zip(results.iter()) {
        let ok = *right <= CONTENT_W;
        // 「改前」是**故意保留的回归基线**，只作对照，不参与退出码——
        // 否则这个探针永远红，就没人再拿它当闸门了。
        if *current {
            over += usize::from(!ok);
        }
        println!(
            "  {}  右缘 {right:>7.1} pt  (内容区 {CONTENT_W}, 余量 {:>6.1})  {name}",
            match (*current, ok) {
                (true, true) => "OK  ",
                (true, false) => "OVER",
                (false, true) => "base ",
                (false, false) => "BASE!",
            },
            CONTENT_W - right
        );
    }
    if over == 0 {
        println!("  => 全部 {CONTENT_W} pt 以内。\n");
    } else {
        println!("  => {over} 项越界。\n");
        std::process::exit(1);
    }
}

/// 被量的一行：`false` = 改前的回归基线（只作对照，不进闸门）。
/// 带生命周期参数：闭包要借用 `main` 里的 `t`（`t` 又持有 locale 表的内容）。
type Case<'a> = (&'static str, bool, Box<dyn Fn(&mut egui::Ui) -> f32 + 'a>);

/// `src/ui.rs` 的 `label_column_width` 原样复制。
fn label_column_width(ui: &egui::Ui, label: &str) -> f32 {
    let font_id = ui.style().text_styles[&egui::TextStyle::Body].clone();
    let color = ui.visuals().text_color();
    ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font_id, color))
        .size()
        .x
}

fn fmt_h(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// 直接读 locale 文件：探针不该依赖 `build.rs` 生成的表（那是 binary-only 的）。
fn tr(zh: bool) -> String {
    let p = if zh {
        "locales/zh-CN.toml"
    } else {
        "locales/en-US.toml"
    };
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("读 {p} 失败：{e}"))
}
