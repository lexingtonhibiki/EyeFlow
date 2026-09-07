use crate::Event;
use std::sync::mpsc;
use tray_icon::menu::{Menu, MenuItem, MenuItemBuilder, MenuEvent, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// 托盘菜单操作枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// 打开设置窗口
    OpenSettings,
    /// 全局提醒开关
    ToggleEnabled,
    /// 静音开关（提示音 + 通知）
    ToggleMute,
    /// 退出程序
    Quit,
}

impl TrayAction {
    /// 从菜单字符串 ID 映射到枚举
    pub fn from_menu_id(id: &str) -> Option<Self> {
        match id {
            menu_id::OPEN_SETTINGS => Some(TrayAction::OpenSettings),
            menu_id::TOGGLE_ENABLED => Some(TrayAction::ToggleEnabled),
            menu_id::TOGGLE_MUTE => Some(TrayAction::ToggleMute),
            menu_id::QUIT => Some(TrayAction::Quit),
            _ => None,
        }
    }
}

/// 菜单项字符串 ID 常量
mod menu_id {
    pub const TOGGLE_ENABLED: &str = "toggle_enabled";
    pub const TOGGLE_MUTE: &str = "toggle_mute";
    pub const OPEN_SETTINGS: &str = "open_settings";
    pub const QUIT: &str = "quit";
}

/// 系统托盘
pub struct Tray {
    _tray: TrayIcon,
    toggle_enabled_item: MenuItem,
    toggle_mute_item: MenuItem,
}

impl Tray {
    /// 创建系统托盘
    ///
    /// 会启动一个独立线程监听 `MenuEvent` 并将其转发到主循环的 `event_tx`。
    pub fn new(event_tx: mpsc::Sender<Event>, enabled: bool, muted: bool) -> Self {
        let icon = build_icon();

        // 先创建菜单项（需在 TrayIcon 之前，以便存储句柄）
        let toggle_enabled_item = build_enabled_item(enabled);
        let toggle_mute_item = build_mute_item(muted);
        let open_settings_item = MenuItemBuilder::new()
            .enabled(true)
            .id(menu_id::OPEN_SETTINGS.into())
            .text("设置")
            .build();
        let separator_item = PredefinedMenuItem::separator();
        let quit_item = MenuItemBuilder::new()
            .enabled(true)
            .id(menu_id::QUIT.into())
            .text("退出")
            .build();

        let menu = Menu::new();
        menu.append(&toggle_enabled_item).expect("添加菜单项失败");
        menu.append(&toggle_mute_item).expect("添加菜单项失败");
        menu.append(&open_settings_item).expect("添加菜单项失败");
        menu.append(&separator_item).expect("添加分隔符失败");
        menu.append(&quit_item).expect("添加菜单项失败");

        let tray = TrayIconBuilder::new()
            .with_tooltip("EyeFlow - 护眼提醒")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
            .expect("托盘图标创建失败");

        // 监听菜单事件（tray-icon 使用全局事件通道）
        let tx = event_tx.clone();
        std::thread::Builder::new()
            .name("tray-listener".into())
            .spawn(move || {
                let receiver = MenuEvent::receiver();
                while let Ok(event) = receiver.recv() {
                    if let Some(action) = TrayAction::from_menu_id(event.id.0.as_ref()) {
                        if tx.send(Event::TrayAction(action)).is_err() {
                            break;
                        }
                    }
                }
            })
            .expect("无法启动托盘监听线程");

        Self { _tray: tray, toggle_enabled_item, toggle_mute_item }
    }

    /// 更新提醒开关状态（菜单文字 + 已用属性）
    pub fn set_enabled(&self, enabled: bool) {
        self.toggle_enabled_item.set_text(if enabled { "✅ 提醒已开启" } else { "⏸ 提醒已关闭" });
    }

    /// 更新静音开关状态（菜单文字 + 已用属性）
    pub fn set_muted(&self, muted: bool) {
        self.toggle_mute_item.set_text(if muted { "🔇 已静音" } else { "🔊 声音已开启" });
    }

    /// 显示系统通知（日志替代，待 notify-rust 实现原生通知）
    pub fn show_notification(&self, title: &str, body: &str) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("通知: [{}] {}", title, body);
        Ok(())
    }
}

fn build_enabled_item(enabled: bool) -> MenuItem {
    MenuItemBuilder::new()
        .enabled(true)
        .id(menu_id::TOGGLE_ENABLED.into())
        .text(if enabled { "✅ 提醒已开启" } else { "⏸ 提醒已关闭" })
        .build()
}

fn build_mute_item(muted: bool) -> MenuItem {
    MenuItemBuilder::new()
        .enabled(true)
        .id(menu_id::TOGGLE_MUTE.into())
        .text(if muted { "🔇 已静音" } else { "🔊 声音已开启" })
        .build()
}

/// 生成 32x32 绿色眼睛图标（程序化，无外部文件依赖）
fn build_icon() -> Icon {
    let size = 32u32;
    let cx = size as f32 / 2.0;
    let cy = size as f32 / 2.0;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx + 0.5;
            let dy = y as f32 - cy + 0.5;
            let dist = (dx * dx + dy * dy).sqrt();

            let (r, g, b, a) = if dist < 10.0 {
                let bright = 1.0 - (dist / 10.0) * 0.3;
                (0, (150.0 * bright) as u8, (200.0 * bright) as u8, 220)
            } else if dist < 11.0 {
                (50, 120, 160, 180)
            } else if dist < 4.0 {
                (0, 0, 0, 0)
            } else {
                (0, 0, 0, 0)
            };

            let (r, g, b, a) = if dist < 3.5 {
                (20, 20, 30, 230)
            } else if dist < 4.0 {
                (40, 40, 50, 200)
            } else {
                (r, g, b, a)
            };

            let (r, g, b, a) = {
                let hx = x as f32 - 13.0;
                let hy = y as f32 - 12.5;
                let hd = (hx * hx + hy * hy).sqrt();
                if hd < 2.0 {
                    let alpha = ((2.0 - hd) / 2.0 * 200.0) as u8;
                    (255, 255, 255, alpha)
                } else {
                    (r, g, b, a)
                }
            };

            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }
    }

    Icon::from_rgba(rgba, size, size).expect("图标创建失败")
}
