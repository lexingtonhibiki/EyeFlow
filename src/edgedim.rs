//! 预告期的“边缘渐暗”：屏幕四条边各一条半透明暗色细条。
//!
//! v0.5.1 修复闪烁：不再用 egui/glow 视口窗口（分层 GL 窗口每次交换缓冲都会
//! 以 alpha 重绘，叠加周期性 repaint 表现为四边闪烁），改为 **4 个原生 Win32
//! 静态窗口**——`WS_POPUP` + 系统黑画刷，`WS_EX_LAYERED|TRANSPARENT|NOACTIVATE|
//! TOPMOST`，`SetLayeredWindowAttributes(LWA_ALPHA)` 半透明。静态窗口没有
//! 渲染循环，从根上消除闪烁；用几何纯函数保留单元测试。
//!
//! 窗口几何用**物理像素**（GetSystemMetrics 主屏），不依赖 egui 的逻辑坐标，
//! 避免根锚点视口（屏幕外 1×1）pixels-per-point 漂移导致的尺寸抖动。
//! 窗口进程生命周期内复用：预告开始 `show()`、结束 `hide()`（隐藏而非销毁，
//! 下次出现零成本且无闪烁）。

use std::sync::Mutex;

/// 边条宽度（物理像素）
pub const EDGE_PX: i32 = 60;
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

/// 某条边的 (x, y, w, h)，物理像素。`screen` 为主屏物理尺寸。
pub fn edge_rect(edge: Edge, screen: (i32, i32), width: i32) -> (i32, i32, i32, i32) {
    let w = width.min(screen.0 / 4).max(1);
    let h = width.min(screen.1 / 4).max(1);
    match edge {
        Edge::Top => (0, 0, screen.0, h),
        Edge::Bottom => (0, screen.1 - h, screen.0, h),
        Edge::Left => (0, 0, w, screen.1),
        Edge::Right => (screen.0 - w, 0, w, screen.1),
    }
}

// ---------------------------------------------------------------- 原生窗口

/// 已创建的 4 个边窗句柄（isize 存储；只会在主线程创建/使用）。
static HANDLES: Mutex<Option<[isize; 4]>> = Mutex::new(None);

/// 确保边窗存在并按预告状态显示。幂等；窗口进程生命周期内复用。
/// 返回 false 表示创建失败（边缘渐暗不可用，不影响其他功能）。
pub fn show() -> bool {
    let mut guard = match HANDLES.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    match *guard {
        Some(handles) => set_visible(handles, true),
        None => match create_all() {
            Some(handles) => {
                *guard = Some(handles);
                set_visible(handles, true)
            }
            None => false,
        },
    }
}

/// 隐藏边窗（预告结束）。幂等。
pub fn hide() {
    if let Ok(guard) = HANDLES.lock() {
        if let Some(handles) = *guard {
            set_visible(handles, false);
        }
    }
}

fn set_visible(handles: [isize; 4], visible: bool) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOWNOACTIVATE};
    let mut all_ok = true;
    for raw in handles {
        let hwnd = windows::Win32::Foundation::HWND(raw as *mut _);
        if hwnd.0.is_null() {
            all_ok = false;
            continue;
        }
        unsafe {
            let _ = ShowWindow(hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    }
    all_ok
}

fn create_all() -> Option<[isize; 4]> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::CreateSolidBrush;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, RegisterClassW, SetLayeredWindowAttributes, LWA_ALPHA,
        WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_EX_TRANSPARENT, WS_POPUP,
    };

    // windows 0.62 的 DefWindowProcW 是安全包装，不能直接当 WNDPROC；包一层 extern。
    unsafe extern "system" fn edge_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    unsafe {
        // 类只注册一次（重复注册报错可忽略）
        static REGISTERED: std::sync::Once = std::sync::Once::new();
        REGISTERED.call_once(|| {
            let hinstance = GetModuleHandleW(None).unwrap_or_default();
            let class_name = w!("EyeFlowEdgeDim");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(edge_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                hbrBackground: CreateSolidBrush(COLORREF(0)),
                style: Default::default(),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hIcon: Default::default(),
                hCursor: Default::default(),
                lpszMenuName: PCWSTR::null(),
            };
            let _ = RegisterClassW(&wc);
        });

        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        let screen = primary_screen();
        let ex =
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
        let mut handles = [0isize; 4];
        for (i, edge) in ALL.iter().enumerate() {
            let (x, y, w, h) = edge_rect(*edge, screen, EDGE_PX);
            let hwnd = CreateWindowExW(
                ex,
                w!("EyeFlowEdgeDim"),
                w!("EyeFlow edge dim"),
                WS_POPUP,
                x,
                y,
                w,
                h,
                None,
                None,
                Some(HINSTANCE(hinstance.0)),
                None,
            );
            if hwnd.is_err() {
                return None;
            }
            let hwnd = hwnd.unwrap();
            if hwnd.0.is_null() {
                return None;
            }
            if SetLayeredWindowAttributes(hwnd, COLORREF(0), EDGE_ALPHA, LWA_ALPHA).is_err() {
                return None;
            }
            handles[i] = hwnd.0 as isize;
        }
        Some(handles)
    }
}

/// 主屏物理尺寸。
pub fn primary_screen() -> (i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects_hug_their_edges() {
        let screen = (1000, 800);
        assert_eq!(edge_rect(Edge::Top, screen, 60), (0, 0, 1000, 60));
        assert_eq!(edge_rect(Edge::Bottom, screen, 60), (0, 740, 1000, 60));
        assert_eq!(edge_rect(Edge::Left, screen, 60), (0, 0, 60, 800));
        assert_eq!(edge_rect(Edge::Right, screen, 60), (940, 0, 60, 800));
    }

    #[test]
    fn width_is_capped_on_tiny_screens() {
        assert_eq!(edge_rect(Edge::Left, (100, 100), 60), (0, 0, 25, 100));
        assert_eq!(edge_rect(Edge::Bottom, (100, 100), 60), (0, 75, 100, 25));
    }

    #[test]
    fn zero_screens_do_not_panic() {
        assert_eq!(edge_rect(Edge::Top, (0, 0), 60), (0, 0, 0, 1));
        assert_eq!(edge_rect(Edge::Right, (0, 0), 60), (-1, 0, 1, 0));
    }
}
