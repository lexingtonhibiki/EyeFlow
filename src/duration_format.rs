//! Shared, localized duration formatting, independent of a GUI backend.

use std::time::Duration;

use crate::tr::tr_fill;

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
