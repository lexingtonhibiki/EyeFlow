use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use std::sync::mpsc;

use crate::config::FlowSensitivity;
use crate::Event;

/// 检测前台窗口是否为全屏状态。
pub fn is_fullscreen() -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowRect, GetSystemMetrics,
            SYSTEM_METRICS_INDEX,
        };

        unsafe {
            let hwnd = GetForegroundWindow();
            #[allow(clippy::erasing_op, clippy::zero_ptr)]
            if hwnd.0 == std::ptr::null_mut() {
                return false;
            }

            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return false;
            }

            let screen_w = GetSystemMetrics(SYSTEM_METRICS_INDEX(0));
            let screen_h = GetSystemMetrics(SYSTEM_METRICS_INDEX(1));

            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;

            w + 2 >= screen_w && h + 2 >= screen_h
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// ---- 心流检测器 ----
/// 30 秒滑动窗口统计按键频率。

pub struct FlowDetector {
    key_times: VecDeque<Instant>,
    threshold: u32,
    in_flow: bool,
}

impl FlowDetector {
    pub fn new(sensitivity: FlowSensitivity) -> Self {
        Self {
            key_times: VecDeque::with_capacity(128),
            threshold: sensitivity.threshold(),
            in_flow: false,
        }
    }

    pub fn on_key_down(&mut self) {
        let now = Instant::now();
        self.key_times.push_back(now);
        while let Some(&t) = self.key_times.front() {
            if now - t > Duration::from_secs(30) {
                self.key_times.pop_front();
            } else {
                break;
            }
        }
        self.in_flow = self.key_times.len() >= self.threshold as usize;
    }

    #[allow(dead_code)]
    pub fn is_in_flow(&self) -> bool {
        if !self.in_flow {
            return false;
        }
        if let Some(&last) = self.key_times.back() {
            if last.elapsed() > Duration::from_secs(8) {
                return false;
            }
        }
        true
    }

    pub fn idle_time(&self) -> Option<Duration> {
        self.key_times.back().map(|t| t.elapsed())
    }

    pub fn set_sensitivity(&mut self, s: FlowSensitivity) {
        self.threshold = s.threshold();
        self.in_flow = self.key_times.len() >= self.threshold as usize;
    }
}

/// ---- 空闲检测 ----

/// 返回用户空闲分钟数（基于 GetLastInputInfo）。
pub fn idle_minutes() -> u64 {
    #[cfg(target_os = "windows")]
    {
        #[repr(C)]
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
                let idle_ms = GetTickCount().wrapping_sub(lii.dw_time);
                (idle_ms as u64) / 60_000
            } else {
                0
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

/// ---- 低级键盘钩子 ----
/// WH_KEYBOARD_LL 不需要 UAC 提权即可全系统键盘监听。

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, HHOOK, MSG, WM_QUIT,
    WM_KEYDOWN, WM_SYSKEYDOWN,
};

/// 存储 (钩子句柄, 线程ID)，供退出时清理和唤醒。
/// HHOOK 是 `*mut c_void` 的 newtype，不实现 Send/Sync，转 isize 存储。
static KB_STATE: OnceLock<(isize, u32)> = OnceLock::new();
static KB_TX: OnceLock<mpsc::Sender<Event>> = OnceLock::new();

unsafe extern "system" fn keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 {
        match wparam.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if let Some(tx) = KB_TX.get() {
                    let _ = tx.send(Event::KeyboardActivity);
                }
            }
            _ => {}
        }
    }
    unsafe { CallNextHookEx(Some(HHOOK::default()), code, wparam, lparam) }
}

/// 启动键盘钩子线程。
///
/// 阻塞直到收到 WM_QUIT。main loop 退出时须调用 `stop_keyboard_hook()`。
pub fn start_keyboard_hook(
    tx: mpsc::Sender<Event>,
) {
    KB_TX.set(tx).ok().expect("keyboard hook already started");

    let thread_id: u32;
    let hook: HHOOK;

    unsafe {
        thread_id = windows::Win32::System::Threading::GetCurrentThreadId();
        let hmod = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
            .unwrap_or_default();
        // HMODULE 与 HINSTANCE 在 Win32 中相同，构造方式一致
        let hinst = windows::Win32::Foundation::HINSTANCE(hmod.0);
        hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(hinst), 0)
            .expect("SetWindowsHookExW failed");
    }

    KB_STATE.set((hook.0 as isize, thread_id)).ok();

    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0).as_bool() } {
        // WH_KEYBOARD_LL 回调由 GetMessageW 内部调用
    }

    // GetMessageW 返回 false（收到 WM_QUIT），清理钩子
    unsafe { let _ = UnhookWindowsHookEx(hook); }
}

/// 停止键盘钩子并唤醒消息泵线程。
pub fn stop_keyboard_hook() {
    if let Some(&(raw_hook, thread_id)) = KB_STATE.get() {
        unsafe {
            let hook = HHOOK(raw_hook as *mut std::ffi::c_void);
            let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            let _ = UnhookWindowsHookEx(hook);
        }
    }
}
