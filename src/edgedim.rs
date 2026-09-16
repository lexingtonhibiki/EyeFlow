//! 预告期的“边缘渐暗”覆盖层：全屏、透明、点击穿透、不抢焦点，
//! 屏幕四边向内渐隐的暗色渐变，让预告余光可感。
//!
//! 【接口桩：由子任务实现】签名保持不变。

use egui::Color32;

/// 预告浮窗之外的全屏覆盖视口 Builder（透明 + 点击穿透 + 置顶 + 不进任务栏）。
pub fn builder(base: egui::ViewportBuilder) -> egui::ViewportBuilder {
    let _ = base;
    base
}

/// 纯数学：把 `rect` 的四条边切成 `width` 宽的渐变条。
/// 返回 (条矩形, 条内侧的颜色)；外侧颜色由调用方按 alpha 渐变绘制。
pub fn edge_strips(rect: egui::Rect, width: f32) -> Vec<(egui::Rect, egui::Rect)> {
    let _ = (rect, width);
    Vec::new()
}

/// 在当前 ui 上绘制四边渐暗（`color` 为最外缘颜色，向内渐隐到透明）。
pub fn paint(ui: &mut egui::Ui, width: f32, color: Color32) {
    let _ = (ui, width, color);
}
