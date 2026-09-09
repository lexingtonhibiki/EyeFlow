use std::sync::Mutex;
use std::time::Instant;

use crate::core::Sensors;

/// 后台线程 → 主线程的事件。
///
/// 按键时间戳（心流判定需要精确到按键）、1 Hz 传感器快照、可选的更新检查结果。
/// 托盘菜单、热键都由各自 crate 的全局 channel 在主线程轮询，不经此总线。
#[derive(Debug, Clone)]
pub enum Event {
    KeyPress(Instant),
    Sensors(Sensors),
    /// 后台线程完成的更新检查结果：Ok(最新版本) 或 Err(失败原因)
    UpdateChecked(Result<String, String>),
}

static UI_CTX: Mutex<Option<egui::Context>> = Mutex::new(None);

/// UI 会话开始时登记、结束时清除；期间任何线程都可以用 `wake_ui()` 唤醒 egui 事件循环。
/// 没有 UI 会话时（轻量循环）这是空操作——轻量循环自己以固定节拍轮询。
pub fn set_ui_context(ctx: Option<egui::Context>) {
    if let Ok(mut slot) = UI_CTX.lock() {
        *slot = ctx;
    }
}

pub fn wake_ui() {
    if let Ok(slot) = UI_CTX.lock() {
        if let Some(ctx) = slot.as_ref() {
            ctx.request_repaint();
        }
    }
}
