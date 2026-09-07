use crate::config::Config;
use crate::tray::TrayAction;

/// EyeFlow 全局事件枚举
///
/// 所有模块通过 mpsc::Sender<Event> 向主循环推事件。
/// 不做模块间直接耦合，所有消息走事件总线。
#[derive(Debug, Clone)]
pub enum Event {
    // ---- 检测器事件 ----
    /// 键盘活动（低级别钩子回调时不区分按键）
    KeyboardActivity,
    /// 全屏状态变化
    FullscreenChanged(bool),
    /// 连续空闲 3 分钟
    IdleTimeout,

    // ---- 托盘事件 ----
    /// 托盘菜单操作
    TrayAction(TrayAction),

    // ---- 全局快捷键 (预留) ----
    /// Ctrl+Shift+E 静音开关
    #[allow(dead_code)]
    GlobalHotkey,

    // ---- 提醒器事件 ----
    /// 定时器节拍（~1s间隔，驱动状态机 tick）
    TimerTick,
    /// 提醒触发
    ReminderTriggered,

    // ---- 设置窗口 ----
    /// 打开设置窗口 (由 TrayAction::OpenSettings 覆盖)
    #[allow(dead_code)]
    OpenSettings,
    /// 设置已变更
    SettingsChanged(Config),

    // ---- 生命周期 ----
    /// 退出程序
    #[allow(dead_code)]
    Quit,
}
