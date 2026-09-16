//! 严格模式背景图片：解码、缩放、纹理缓存、自适应裁剪与预览。
//!
//! 解码在调用线程同步完成（设置里选图是一次性操作），结果按路径缓存，
//! 路径不变时不会每帧重复解码；纯几何计算集中在 [`crop_uv`] 并有单元测试。

use crate::config::WallpaperFit;

/// 严格模式蒙层样式：`alpha` 为不透明度(0.0~0.85)，`gradient` 为上深下浅的垂直渐变。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayStyle {
    pub alpha: f32,
    pub gradient: bool,
}

impl Default for OverlayStyle {
    fn default() -> Self {
        Self { alpha: 0.55, gradient: false }
    }
}

/// 支持的图片扩展名
pub const WALLPAPER_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif"];

/// 纹理最长边上限：超过则等比缩小，控制显存与内存占用。
const MAX_TEXTURE_SIDE: u32 = 2560;

/// Contain 模式留边 / 整体底色（深蓝黑）。
const BACKDROP: egui::Color32 = egui::Color32::from_rgb(14, 18, 26);

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
        let Some(path) = path else {
            self.invalidate();
            return None;
        };
        let path = std::path::PathBuf::from(path);
        if self.path.as_ref() == Some(&path) {
            // 同一路径：命中缓存直接返回；上次失败也不重复解码，换图或 invalidate() 后重试
            return self.tex.as_ref();
        }
        self.path = Some(path.clone());
        self.tex = None;
        match decode_texture(&path) {
            Ok(image) => {
                self.error = None;
                self.tex = Some(ctx.load_texture(
                    "eyeflow-wallpaper",
                    image,
                    egui::TextureOptions::LINEAR,
                ));
            }
            Err(err) => {
                log::warn!("{} ({})", err, path.display());
                self.error = Some(err);
            }
        }
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

/// 解码图片并转 RGBA8，最长边超过 [`MAX_TEXTURE_SIDE`] 时等比缩小。
fn decode_texture(path: &std::path::Path) -> Result<egui::ColorImage, String> {
    let img = image::open(path).map_err(|err| format!("无法读取图片:{err}"))?;
    let mut rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let long_side = w.max(h);
    if long_side > MAX_TEXTURE_SIDE {
        let scale = MAX_TEXTURE_SIDE as f32 / long_side as f32;
        let new_w = ((w as f32 * scale).round() as u32).clamp(1, MAX_TEXTURE_SIDE);
        let new_h = ((h as f32 * scale).round() as u32).clamp(1, MAX_TEXTURE_SIDE);
        rgba = image::imageops::resize(&rgba, new_w, new_h, image::imageops::FilterType::Triangle);
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

/// 纯数学：给定图片尺寸与目标区域尺寸，按自适应方式计算 (源 UV 矩形, 目标内绘制矩形[相对 target 左上角])。
pub fn crop_uv(
    img_size: egui::Vec2,
    target: egui::Vec2,
    fit: WallpaperFit,
) -> (egui::Rect, egui::Rect) {
    let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    // 尺寸非法（0 / 负数 / NaN）时不 panic，退化为零尺寸矩形
    let valid = [img_size.x, img_size.y, target.x, target.y]
        .into_iter()
        .all(|v| v.is_finite() && v > 0.0);
    if !valid {
        return (
            full_uv,
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::Vec2::ZERO),
        );
    }
    let full_dest = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), target);
    let img_ar = img_size.x / img_size.y;
    let target_ar = target.x / target.y;
    match fit {
        // 拉伸：全图 → 全目标
        WallpaperFit::Stretch => (full_uv, full_dest),
        // 铺满裁剪：dest 占满 target，UV 为居中子矩形
        WallpaperFit::Cover => {
            if img_ar > target_ar {
                // 图片更宽：左右居中裁掉
                let keep = target_ar / img_ar;
                let x0 = (1.0 - keep) / 2.0;
                let uv = egui::Rect::from_min_max(egui::pos2(x0, 0.0), egui::pos2(1.0 - x0, 1.0));
                (uv, full_dest)
            } else {
                // 图片更窄（或等比）：上下居中裁掉
                let keep = img_ar / target_ar;
                let y0 = (1.0 - keep) / 2.0;
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, y0), egui::pos2(1.0, 1.0 - y0));
                (uv, full_dest)
            }
        }
        // 完整显示：UV 为全图，dest 为 target 内居中子矩形
        WallpaperFit::Contain => {
            if img_ar > target_ar {
                // 图片更宽：顶满宽度，上下留边
                let h = target.x / img_ar;
                let y0 = (target.y - h) / 2.0;
                let dest = egui::Rect::from_min_size(egui::pos2(0.0, y0), egui::vec2(target.x, h));
                (full_uv, dest)
            } else {
                // 图片更窄（或等比）：顶满高度，左右留边
                let w = target.y * img_ar;
                let x0 = (target.x - w) / 2.0;
                let dest = egui::Rect::from_min_size(egui::pos2(x0, 0.0), egui::vec2(w, target.y));
                (full_uv, dest)
            }
        }
    }
}

/// 在 `rect`（通常是全屏休息面板）上绘制背景图。
pub fn paint_fullscreen(
    ui: &mut egui::Ui,
    tex: &egui::TextureHandle,
    fit: WallpaperFit,
    rect: egui::Rect,
    overlay: OverlayStyle,
) {
    let painter = ui.painter();
    // 深色底：Contain 的留边处可见，也兜住纹理未覆盖的区域
    painter.rect_filled(rect, 0.0, BACKDROP);
    let (uv, dest_rel) = crop_uv(tex.size_vec2(), rect.size(), fit);
    let dest = dest_rel.translate(rect.min.to_vec2());
    painter.image(tex.id(), dest, uv, egui::Color32::WHITE);
    // 半透明暗色叠层：保证上层倒计时文字可读
    paint_overlay(painter, rect, overlay);
}

/// 叠层：`gradient=false` 均匀；`true` 为上深下浅的垂直渐变（顶 alpha → 35% alpha）。
/// 【子任务实现】当前为均匀近似占位。
fn paint_overlay(painter: &egui::Painter, rect: egui::Rect, overlay: OverlayStyle) {
    let _ = overlay.gradient;
    painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(to_alpha8(overlay.alpha)));
}

pub fn to_alpha8(alpha01: f32) -> u8 {
    (alpha01.clamp(0.0, 0.85) * 255.0).round() as u8
}

/// 设置窗中的 16:9 裁剪预览框（宽度 `width`），实时反映 `fit` 的裁剪结果。
pub fn preview(
    ui: &mut egui::Ui,
    tex: &egui::TextureHandle,
    fit: WallpaperFit,
    width: f32,
    overlay: OverlayStyle,
) -> egui::Response {
    let width = width.max(0.0);
    let size = egui::vec2(width, width * 9.0 / 16.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, BACKDROP);
    let (uv, dest_rel) = crop_uv(tex.size_vec2(), size, fit);
    let dest = dest_rel.translate(rect.min.to_vec2());
    painter.image(tex.id(), dest, uv, egui::Color32::WHITE);
    let outline = ui.visuals().widgets.noninteractive.bg_stroke;
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, outline.color),
        egui::StrokeKind::Inside,
    );
    let [w, h] = tex.size();
    response.on_hover_text(format!("原图 {w}×{h} px · 当前模式：{}", fit.label()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn assert_rect_eq(a: egui::Rect, b: egui::Rect) {
        assert!(
            approx(a.min.x, b.min.x)
                && approx(a.min.y, b.min.y)
                && approx(a.max.x, b.max.x)
                && approx(a.max.y, b.max.y),
            "{a:?} != {b:?}"
        );
    }

    fn full_uv() -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
    }

    /// 16:9 图 → 4:3 目标 Cover：裁左右，dest 占满 target
    #[test]
    fn cover_wide_image_into_narrow_target_crops_sides() {
        let img = vec2(1920.0, 1080.0);
        let target = vec2(1024.0, 768.0);
        let (uv, dest) = crop_uv(img, target, WallpaperFit::Cover);
        assert_rect_eq(
            uv,
            egui::Rect::from_min_max(egui::pos2(0.125, 0.0), egui::pos2(0.875, 1.0)),
        );
        assert_rect_eq(
            dest,
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), target),
        );
    }

    /// 4:3 图 → 16:9 目标 Cover：裁上下，dest 占满 target
    #[test]
    fn cover_narrow_image_into_wide_target_crops_top_bottom() {
        let img = vec2(1024.0, 768.0);
        let target = vec2(1920.0, 1080.0);
        let (uv, dest) = crop_uv(img, target, WallpaperFit::Cover);
        assert_rect_eq(
            uv,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.125), egui::pos2(1.0, 0.875)),
        );
        assert_rect_eq(
            dest,
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), target),
        );
    }

    /// 16:9 图 → 4:3 目标 Contain：全图显示，上下留边
    #[test]
    fn contain_wide_image_letterboxes_top_bottom() {
        let img = vec2(1920.0, 1080.0);
        let target = vec2(1024.0, 768.0);
        let (uv, dest) = crop_uv(img, target, WallpaperFit::Contain);
        assert_rect_eq(uv, full_uv());
        // 顶满宽度 1024 → 高 576，垂直居中留 96 px 边
        assert_rect_eq(
            dest,
            egui::Rect::from_min_size(egui::pos2(0.0, 96.0), egui::vec2(1024.0, 576.0)),
        );
    }

    /// 4:3 图 → 16:9 目标 Contain：全图显示，左右留边
    #[test]
    fn contain_narrow_image_pillarboxes_sides() {
        let img = vec2(1024.0, 768.0);
        let target = vec2(1920.0, 1080.0);
        let (uv, dest) = crop_uv(img, target, WallpaperFit::Contain);
        assert_rect_eq(uv, full_uv());
        // 顶满高度 1080 → 宽 1440，水平居中留 240 px 边
        assert_rect_eq(
            dest,
            egui::Rect::from_min_size(egui::pos2(240.0, 0.0), egui::vec2(1440.0, 1080.0)),
        );
    }

    /// Stretch 恒等：全图 → 全目标
    #[test]
    fn stretch_is_identity() {
        let img = vec2(800.0, 600.0);
        let target = vec2(1366.0, 768.0);
        for (i, t) in [(img, target), (target, img)] {
            let (uv, dest) = crop_uv(i, t, WallpaperFit::Stretch);
            assert_rect_eq(uv, full_uv());
            assert_rect_eq(dest, egui::Rect::from_min_size(egui::pos2(0.0, 0.0), t));
        }
    }

    /// 方形图 → 方形目标：三种模式结果一致
    #[test]
    fn square_into_square_all_fits_agree() {
        let img = vec2(100.0, 100.0);
        let target = vec2(200.0, 200.0);
        let mut expected: Option<(egui::Rect, egui::Rect)> = None;
        for fit in WallpaperFit::ALL {
            let result = crop_uv(img, target, fit);
            assert_rect_eq(result.0, full_uv());
            assert_rect_eq(
                result.1,
                egui::Rect::from_min_size(egui::pos2(0.0, 0.0), target),
            );
            if let Some(prev) = expected {
                assert_rect_eq(prev.0, result.0);
                assert_rect_eq(prev.1, result.1);
            }
            expected = Some(result);
        }
    }

    /// 极端 0 尺寸不 panic
    #[test]
    fn zero_sizes_do_not_panic() {
        let normal = vec2(640.0, 480.0);
        let zero = egui::Vec2::ZERO;
        for fit in WallpaperFit::ALL {
            let _ = crop_uv(zero, normal, fit);
            let _ = crop_uv(normal, zero, fit);
            let (uv, dest) = crop_uv(zero, zero, fit);
            assert_eq!(dest.size(), egui::Vec2::ZERO);
            assert_rect_eq(uv, full_uv());
        }
    }
}
