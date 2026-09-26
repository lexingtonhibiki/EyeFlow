/// 休息面板上轮换展示的护眼贴士。
///
/// 全部改写自 AOA / AAO 官方页面的表述（docs/research/02-science-evidence.md），
/// 刻意避开“保护视力 / 防近视 / 防止眼损伤”等与 AAO 定位冲突的说法。
///
/// 文案本体在 `locales/*.toml` 的 `tip.1`~`tip.7`（v0.6 起）。本模块只保留
/// **key**，让贴士也走上同一条 `tr()` 通路——`pick()` 每次返回 `&'static str`，
/// 调用点（`app.rs:437`）直接交给 `RichText::new`，每帧零分配。
pub const TIP_KEYS: &[&str] = &[
    "tip.1", "tip.2", "tip.3", "tip.4", "tip.5", "tip.6", "tip.7",
];

pub fn pick(index: usize) -> &'static str {
    crate::tr::tr(TIP_KEYS[index % TIP_KEYS.len()])
}
