//! 系统检测：键盘钩子（心流）、空闲秒数、全屏与可打扰性（传感器线程）。
//!
//! 决策依据 docs/adr/0004：`SHQueryUserNotificationState` 为主，前台窗口矩形 + 无标题栏为兜底。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_ACCEPTS_NOTIFICATIONS, QUNS_NOT_PRESENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetMessageW, GetShellWindow, GetWindowLongW,
    GetWindowRect, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, GWL_STYLE, HHOOK,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN,
    WM_SYSKEYUP, WS_CAPTION,
};

use crate::core::Sensors;
use crate::event::{wake_ui, Event};

// ------------------------------------------------------------------ 传感器

/// 采样一次传感器快照（约 3 个廉价 Win32 调用）。
pub fn sample_sensors() -> Sensors {
    Sensors {
        idle_secs: idle_millis() / 1000,
        fullscreen: foreground_is_fullscreen(),
        interruptible: system_accepts_notifications(),
    }
}

/// 启动 1 Hz 传感器线程。
pub fn start_sensor_thread(tx: mpsc::Sender<Event>) {
    std::thread::Builder::new()
        .name("eyeflow-sensors".into())
        .spawn(move || {
            let mut last: Option<Sensors> = None;
            loop {
                let s = sample_sensors();
                // 每秒都发一份（空闲秒数在变），但只有状态位变化时才额外唤醒 UI，
                // 让 UI 自身的 1 Hz 节拍处理常规刷新。
                let state_changed = last
                    .map(|l| l.fullscreen != s.fullscreen || l.interruptible != s.interruptible)
                    .unwrap_or(true);
                last = Some(s);
                if tx.send(Event::Sensors(s)).is_err() {
                    break;
                }
                if state_changed {
                    wake_ui();
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        })
        .expect("无法启动传感器线程");
}

/// 自上次键鼠输入以来的毫秒数（GetLastInputInfo，含鼠标；wrapping_sub 处理 49.7 天回绕）。
pub fn idle_millis() -> u64 {
    #[repr(C)]
    #[allow(clippy::upper_case_acronyms)]
    struct LASTINPUTINFO {
        cb_size: u32,
        dw_time: u32,
    }
    unsafe extern "system" {
        fn GetLastInputInfo(plii: *mut LASTINPUTINFO) -> i32;
        fn GetTickCount() -> u32;
    }
    unsafe {
        let mut lii = LASTINPUTINFO {
            cb_size: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dw_time: 0,
        };
        if GetLastInputInfo(&mut lii) != 0 {
            GetTickCount().wrapping_sub(lii.dw_time) as u64
        } else {
            0
        }
    }
}

/// 官方“此刻是否适合打扰用户”。锁屏/屏保（NOT_PRESENT）交给空闲逻辑处理，因此视为可打扰。
pub fn system_accepts_notifications() -> bool {
    match unsafe { SHQueryUserNotificationState() } {
        Ok(state) => state == QUNS_ACCEPTS_NOTIFICATIONS || state == QUNS_NOT_PRESENT,
        Err(_) => true,
    }
}

/// 前台窗口覆盖其所在显示器且没有标题栏 → 全屏应用（无边框窗口化游戏、F11 视频等）。
pub fn foreground_is_fullscreen() -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() || hwnd == GetShellWindow() {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return false;
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        if style & WS_CAPTION.0 == WS_CAPTION.0 {
            return false;
        }
        let Some(mon) = monitor_rect(hwnd) else {
            return false;
        };
        let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
        let (mw, mh) = (mon.right - mon.left, mon.bottom - mon.top);
        w + 2 >= mw && h + 2 >= mh
    }
}

unsafe fn monitor_rect(hwnd: HWND) -> Option<RECT> {
    let hmon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if hmon.0.is_null() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(hmon, &mut info) }.as_bool() {
        Some(info.rcMonitor)
    } else {
        None
    }
}

// ------------------------------------------------------------------ 键盘钩子

static KB_TX: OnceLock<mpsc::Sender<Event>> = OnceLock::new();
static KB_THREAD: OnceLock<u32> = OnceLock::new();
/// 每个虚拟键的按下状态，用来过滤自动重复（低级钩子没有 KF_REPEAT 标志）。
static KEY_DOWN: [AtomicBool; 256] = [const { AtomicBool::new(false) }; 256];

fn is_modifier(vk: u32) -> bool {
    matches!(vk, 0x10..=0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wparam.0 as u32;
        let vk = unsafe { (*(lparam.0 as *const KBDLLHOOKSTRUCT)).vkCode } as usize & 0xFF;
        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let was_down = KEY_DOWN[vk].swap(true, Ordering::Relaxed);
                if !was_down && !is_modifier(vk as u32) {
                    if let Some(tx) = KB_TX.get() {
                        let _ = tx.send(Event::KeyPress(Instant::now()));
                    }
                    // 心流判定对时效不敏感，交给 UI 的 1 Hz 节拍处理即可，不额外唤醒。
                }
            }
            WM_KEYUP | WM_SYSKEYUP => {
                KEY_DOWN[vk].store(false, Ordering::Relaxed);
            }
            _ => {}
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// 启动键盘钩子线程（自带消息泵）。
pub fn start_keyboard_hook(tx: mpsc::Sender<Event>) {
    if KB_TX.set(tx).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("eyeflow-keyhook".into())
        .spawn(|| unsafe {
            let _ = KB_THREAD.set(windows::Win32::System::Threading::GetCurrentThreadId());
            let hmod =
                windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default();
            let hook: HHOOK = match SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_proc),
                Some(windows::Win32::Foundation::HINSTANCE(hmod.0)),
                0,
            ) {
                Ok(h) => h,
                Err(e) => {
                    log::error!("键盘钩子安装失败: {e}（心流检测不可用）");
                    return;
                }
            };
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
            let _ = UnhookWindowsHookEx(hook);
        })
        .expect("无法启动键盘钩子线程");
}

/// 让钩子线程退出并自行卸载钩子。
pub fn stop_keyboard_hook() {
    if let Some(&tid) = KB_THREAD.get() {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}
