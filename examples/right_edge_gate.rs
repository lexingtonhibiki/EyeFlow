//! 闸门 B：设置窗截图的**右缘扫描**——一个不靠肉眼、可重复的验收。
//!
//! # 为什么要有它
//!
//! v0.6.1 把 `tune_style` 的 `spacing.slider_width` 从 egui 默认的 100.0 抬到
//! 220.0，两行「一行两个滑条」的设置项因此冲出内容区右缘。**这类缺陷的本质是
//! 「要目视才能发现」**：unit test 测不到，clippy 测不到，`cargo test` 全绿，
//! 只有把窗口截成图、放大看右边缘才知道少了东西。
//!
//! 所以这里给一个客观判据：**内容区右边那条竖带里必须空无一物**。
//!
//! # 判据
//!
//! 设置窗截图的宽度 W = 710 物理 px（700 逻辑 px @ 125% DPI + 边框）。带子取
//! `x ∈ [W-22, W-12]`，`y ∈ [30, H-8]`：它落在卡片右边框（约 677）与窗口边框
//! （710）之间的空隙里——**本该一个像素都没有**。任何非背景像素都说明有控件
//! 越过了卡片右缘，也就是「英文设置窗右侧内容被裁掉」的可测量形式。
//!
//! 背景色不写死：取该竖带自身的**众数色**（抗锯齿与圆角会产生少量过渡像素），
//! 容差 6/255。众数落在哪个值就是哪个值，两种语言各判一次，互不干扰。
//!
//! # 用法
//!
//! ```text
//! cargo run --release --example right_edge_gate -- <png> [<png> …]
//! ```
//!
//! 全绿退出码 0，任一条带出现越界像素则退出码 1，并逐行报出 y 坐标。
//!
//! 故意放在 `examples/` 而不是 `tests/`：它验收的是**产物（截图）**，不是源码；
//! 而且 `tests/` 目录由另一条线并行工作，两者不该互相踩。

use std::collections::HashMap;
use std::path::PathBuf;

use image::Rgb;

/// 竖带相对窗口右缘的偏移（物理 px）。
const BAND_RIGHT: u32 = 22;
const BAND_LEFT: u32 = 12;
/// 竖带相对窗口上下边缘的留白：躲开标题栏与底边。
///
/// 36 / 12 不是随手取的：本机实测（718×827 的设置窗截图）**窗口自身的黑边框是
/// 最后 9 行**（y = 818..826）**与最后 9 列**（x = 709..717），标题栏在 y < 30。
/// 留白小于这个数就会把窗口边框当成越界内容报出来。
const BAND_TOP: u32 = 36;
const BAND_BOTTOM: u32 = 12;
/// 允许的非背景像素与背景色的每通道最大偏差。
const TOLERANCE: i32 = 6;

fn main() {
    let mut failed = false;
    for arg in std::env::args().skip(1) {
        let path = PathBuf::from(arg);
        match scan(&path) {
            Ok(true) => {}
            Ok(false) => failed = true,
            Err(e) => {
                eprintln!("FAIL {} — 读不进截图：{e}", path.display());
                failed = true;
            }
        }
    }
    if std::env::args().skip(1).count() == 0 {
        eprintln!("用法：cargo run --release --example right_edge_gate -- <png> [<png> …]");
        std::process::exit(2);
    }
    if failed {
        eprintln!("\n闸门 B：FAIL —— 右缘竖带里有内容被裁掉。");
        std::process::exit(1);
    }
    println!("\n闸门 B：PASS —— 所有截图的右缘竖带里空无一物。");
}

/// 扫一张截图。`Ok(true)` = 干净，`Ok(false)` = 有越界像素。
fn scan(path: &PathBuf) -> Result<bool, String> {
    let img = image::open(path).map_err(|e| e.to_string())?.to_rgb8();
    let (w, h) = img.dimensions();
    let x0 = w.saturating_sub(BAND_RIGHT);
    let x1 = w.saturating_sub(BAND_LEFT);
    let y0 = BAND_TOP.min(h);
    let y1 = h.saturating_sub(BAND_BOTTOM);

    if x1 <= x0 || y1 <= y0 {
        return Err(format!("截图 {w}×{h} 太小，取不出竖带"));
    }

    // 背景色 = 该竖带的众数色。抗锯齿与圆角只会产生少数偏离像素，不会抢走众数。
    let mut tally: HashMap<Rgb<u8>, u32> = HashMap::new();
    for y in y0..y1 {
        for x in x0..x1 {
            *tally.entry(*img.get_pixel(x, y)).or_default() += 1;
        }
    }
    let (bg, bg_n) = tally
        .iter()
        .max_by_key(|(_, n)| **n)
        .map(|(c, n)| (*c, *n))
        .ok_or_else(|| "竖带一个像素都没有".to_string())?;
    let total = (x1 - x0) * (y1 - y0);

    // 逐行找越界像素：同一个 y 可能有多个连续像素（一条被截断的文字或滑条轨道）。
    let mut bad_rows: Vec<(u32, u32, Rgb<u8>)> = Vec::new();
    for y in y0..y1 {
        let mut n = 0;
        let mut first = Rgb([0u8; 3]);
        for x in x0..x1 {
            let p = *img.get_pixel(x, y);
            if far(p, bg) {
                if n == 0 {
                    first = p;
                }
                n += 1;
            }
        }
        if n > 0 {
            bad_rows.push((y, n, first));
        }
    }

    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let band = format!("x∈[{x0}, {x1}] y∈[{y0}, {y1}]");

    if bad_rows.is_empty() {
        println!(
            "PASS {name:<22} {w}×{h}  背景色 rgb({},{},{}) 占 {bg_n}/{total}  带 {band}  0 个越界像素",
            bg.0[0], bg.0[1], bg.0[2]
        );
        return Ok(true);
    }

    // 报出全部 y，但同一段连续区域折叠成一条，免得刷屏（滑条轨道常有连续几十行）。
    let mut groups: Vec<(u32, u32, u32, Rgb<u8>)> = Vec::new(); // (y_first, y_last, px, color)
    let n_bad = bad_rows.len();
    for (y, n, c) in bad_rows {
        match groups.last_mut() {
            Some(g) if g.1 + 1 == y => {
                g.1 = y;
                g.2 = g.2.max(n);
            }
            _ => groups.push((y, y, n, c)),
        }
    }

    println!(
        "FAIL {name:<22} {w}×{h}  背景色 rgb({},{},{})  带 {band}  {} 行有越界像素（{} 段）",
        bg.0[0],
        bg.0[1],
        bg.0[2],
        n_bad,
        groups.len()
    );
    for (a, b, n, c) in &groups {
        let ys = if a == b {
            format!("y={a}")
        } else {
            format!("y={a}..{b}")
        };
        println!(
            "     {ys:<14} 最多 {n:>2} px/行  首个越界色 rgb({},{},{})",
            c.0[0], c.0[1], c.0[2]
        );
    }
    Ok(false)
}

fn far(a: Rgb<u8>, b: Rgb<u8>) -> bool {
    (0..3).any(|i| (a.0[i] as i32 - b.0[i] as i32).abs() > TOLERANCE)
}
