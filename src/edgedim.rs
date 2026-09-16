//! 预告期的“边缘渐暗”：屏幕四条边各一条半透明暗色细条（整窗 LWA_ALPHA 半透明，
//! 无需逐像素透明——glow 的清屏色不透明，全屏透明视口会整片变黑）。
//!
//! 四条边都是本进程的普通置顶窗口：不抢焦点、不进任务栏、点击穿透由
//! `WS_EX_TRANSPARENT | WS_EX_LAYERED` 保证；随预告阶段出现/消失。

use egui::{ViewportBuilder, ViewportId};

/// 边条宽度（逻辑像素）
pub const EDGE_PX: f32 = 60.0;
/// 整窗不透明度（0~255，约 43% 暗度）
pub const EDGE_ALPHA: u8 = 110;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

pub const ALL: [Edge; 4] = [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right];

pub fn title(edge: Edge, epoch: u64) -> String {
    let name = match edge {
        Edge::Top => "Top",
        Edge::Bottom => "Bottom",
        Edge::Left => "Left",
        Edge::Right => "Right",
    };
    format!("EyeFlow Dim {name} {epoch}")
}

/// 某条边的视口 ID 与 Builder（位置 / 尺寸已按 `monitor` 逻辑尺寸定好）。
pub fn viewport(edge: Edge, epoch: u64, monitor: egui::Vec2) -> (ViewportId, ViewportBuilder) {
    let w = EDGE_PX.min(monitor.x / 4.0);
    let h = EDGE_PX.min(monitor.y / 4.0);
    let (pos, size) = match edge {
        Edge::Top => ([0.0, 0.0], [monitor.x, h]),
        Edge::Bottom => ([0.0, monitor.y - h], [monitor.x, h]),
        Edge::Left => ([0.0, 0.0], [w, monitor.y]),
        Edge::Right => ([monitor.x - w, 0.0], [w, monitor.y]),
    };
    let id = ViewportId::from_hash_of(("eyeflow-edgedim", edge, epoch));
    let builder = ViewportBuilder::default()
        .with_title(title(edge, epoch))
        .with_position(pos)
        .with_inner_size(size)
        .with_decorations(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_active(false)
        .with_resizable(false);
    (id, builder)
}

/// 面板内容：整面纯黑（窗口本身的 LWA_ALPHA 负责半透明）。
pub fn paint(ui: &mut egui::Ui) {
    ui.painter()
        .rect_filled(ui.clip_rect(), 0.0, egui::Color32::BLACK);
}

/// 对标题以 `title_prefix` 开头的窗口施加：不激活 + 点击穿透 + 整窗半透明。
/// 每个新 epoch 调用一次即可。
pub fn apply_window_styles(epoch: u64, alpha: u8) -> bool {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetLayeredWindowAttributes, SetWindowLongPtrW, GWL_EXSTYLE, LWA_ALPHA,
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TRANSPARENT,
    };
    let mut all_ok = true;
    for edge in ALL {
        let t = title(edge, epoch);
        let wide = HSTRING::from(t.as_str());
        unsafe {
            let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(wide.as_ptr())) else {
                all_ok = false;
                continue;
            };
            if hwnd.0.is_null() {
                all_ok = false;
                continue;
            }
            let ex = SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                GetWindowLongPtrW(hwnd, GWL_EXSTYLE)
                    | (WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0 | WS_EX_NOACTIVATE.0) as isize,
            );
            let _ = ex;
            if SetLayeredWindowAttributes(
                hwnd,
                windows::Win32::Foundation::COLORREF(0),
                alpha,
                LWA_ALPHA,
            )
            .is_err()
            {
                all_ok = false;
            }
        }
    }
    all_ok
}

#[allow(non_snake_case)]
unsafe fn GetWindowLongPtrW(
    hwnd: windows::Win32::Foundation::HWND,
    n: windows::Win32::UI::WindowsAndMessaging::WINDOW_LONG_PTR_INDEX,
) -> isize {
    windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_unique_and_stable() {
        let a: Vec<String> = ALL.iter().map(|e| title(*e, 7)).collect();
        let mut b = a.clone();
        b.sort();
        b.dedup();
        assert_eq!(a.len(), b.len());
        assert_eq!(title(Edge::Top, 7), title(Edge::Top, 7));
    }

    #[test]
    fn viewports_sit_on_their_edges() {
        let m = egui::vec2(1000.0, 800.0);
        let (id, b) = viewport(Edge::Top, 1, m);
        assert_ne!(id, ViewportId::default());
        let pos = b.position.unwrap();
        let size = b.inner_size.unwrap();
        assert_eq!(pos, egui::pos2(0.0, 0.0));
        assert_eq!(size, [1000.0, 60.0]);
        let (_, b2) = viewport(Edge::Right, 1, m);
        assert_eq!(b2.position.unwrap(), egui::pos2(1000.0 - 60.0, 0.0));
        assert_eq!(b2.inner_size.unwrap(), [60.0, 800.0]);
        let (_, b3) = viewport(Edge::Bottom, 1, m);
        assert_eq!(b3.position.unwrap(), egui::pos2(0.0, 800.0 - 60.0));
        let (_, b4) = viewport(Edge::Left, 1, m);
        assert_eq!(b4.inner_size.unwrap(), [60.0, 800.0]);
    }

    #[test]
    fn edge_width_is_capped_on_tiny_screens() {
        let m = egui::vec2(100.0, 100.0);
        let (_, b) = viewport(Edge::Left, 1, m);
        assert_eq!(b.inner_size.unwrap(), [25.0, 100.0]);
    }
}
