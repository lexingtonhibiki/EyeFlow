//! 严格模式背景图片：解码、缩放、纹理缓存、自适应裁剪与预览。
//!
//! 接口桩（由壁纸子任务实现，签名保持不变）。

use crate::config::WallpaperFit;

/// 支持的图片扩展名
pub const WALLPAPER_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif"];

/// 纹理缓存：路径变化时重新解码上传；解码失败记录错误。
#[derive(Default)]
pub struct WallpaperCache {
    path: Option<std::path::PathBuf>,
    tex: Option<egui::TextureHandle>,
    error: Option<String>,
}

impl WallpaperCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 按需加载；`path` 为 None 或加载失败时返回 None（错误见 `last_error`）。
    pub fn get(&mut self, ctx: &egui::Context, path: Option<&str>) -> Option<&egui::TextureHandle> {
        let _ = (ctx, path);
        self.error = Some("壁纸加载尚未实现".to_string());
        self.tex.as_ref()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn invalidate(&mut self) {
        self.path = None;
        self.tex = None;
        self.error = None;
    }
}

/// 纯数学：给定图片尺寸与目标区域尺寸，按自适应方式计算 (源 UV 矩形, 目标内绘制矩形[相对 target 左上角])。
pub fn crop_uv(img_size: egui::Vec2, target: egui::Vec2, fit: WallpaperFit) -> (egui::Rect, egui::Rect) {
    let _ = (img_size, fit);
    (
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), target),
    )
}

/// 在 `rect`（通常是全屏休息面板）上绘制背景图。
pub fn paint_fullscreen(ui: &mut egui::Ui, tex: &egui::TextureHandle, fit: WallpaperFit, rect: egui::Rect) {
    let _ = (ui, tex, fit, rect);
}

/// 设置窗中的 16:9 裁剪预览框（宽度 `width`），实时反映 `fit` 的裁剪结果。
pub fn preview(ui: &mut egui::Ui, tex: &egui::TextureHandle, fit: WallpaperFit, width: f32) -> egui::Response {
    let _ = (tex, fit);
    ui.label(format!("（预览尚未实现，宽 {width}）"))
}
