//! 系统托盘：图标、右键菜单、左键/双击事件。
//!
//! 菜单顺序遵循微软 UX 指南（docs/research/04）：默认命令在首位、常用开关带勾选、Exit 在末尾。
//! 托盘图标必须在运行 Win32 消息循环的线程上创建——即 eframe 主线程、`run_native` 之前。

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    OpenSettings,
    ToggleEnabled,
    RestNow,
    TogglePause,
    ToggleSound,
    Quit,
}

mod id {
    pub const OPEN_SETTINGS: &str = "open_settings";
    pub const TOGGLE_ENABLED: &str = "toggle_enabled";
    pub const REST_NOW: &str = "rest_now";
    pub const TOGGLE_PAUSE: &str = "toggle_pause";
    pub const TOGGLE_SOUND: &str = "toggle_sound";
    pub const QUIT: &str = "quit";
}

pub struct Tray {
    icon: TrayIcon,
    enabled_item: CheckMenuItem,
    rest_item: MenuItem,
    sound_item: CheckMenuItem,
    pause_item: MenuItem,
    tooltip: String,
}

impl Tray {
    pub fn new(enabled: bool, sound_on: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let open_item = MenuItem::with_id(MenuId(id::OPEN_SETTINGS.into()), "打开设置", true, None);
        let enabled_item = CheckMenuItem::with_id(
            MenuId(id::TOGGLE_ENABLED.into()),
            "启用提醒",
            true,
            enabled,
            None,
        );
        let rest_item = MenuItem::with_id(
            MenuId(id::REST_NOW.into()),
            "立即休息\tCtrl+Shift+E",
            true,
            None,
        );
        let pause_item =
            MenuItem::with_id(MenuId(id::TOGGLE_PAUSE.into()), "暂停 1 小时", true, None);
        let sound_item = CheckMenuItem::with_id(
            MenuId(id::TOGGLE_SOUND.into()),
            "提示音",
            true,
            sound_on,
            None,
        );
        let quit_item = MenuItem::with_id(MenuId(id::QUIT.into()), "退出", true, None);

        let menu = Menu::new();
        menu.append(&open_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&enabled_item)?;
        menu.append(&rest_item)?;
        menu.append(&pause_item)?;
        menu.append(&sound_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit_item)?;

        let rgba = crate::icon::render_rgba(32);
        let icon = Icon::from_rgba(rgba, 32, 32)?;

        let tooltip = "EyeFlow 护眼提醒".to_string();
        let tray = TrayIconBuilder::new()
            .with_id("eyeflow")
            .with_tooltip(&tooltip)
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?;

        Ok(Self {
            icon: tray,
            enabled_item,
            rest_item,
            sound_item,
            pause_item,
            tooltip,
        })
    }

    /// 每帧调用：把 tray-icon / muda 的全局事件通道排空成命令。
    pub fn poll(&self) -> Vec<TrayCommand> {
        let mut out = Vec::new();
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            let cmd = match ev.id.0.as_str() {
                id::OPEN_SETTINGS => Some(TrayCommand::OpenSettings),
                id::TOGGLE_ENABLED => Some(TrayCommand::ToggleEnabled),
                id::REST_NOW => Some(TrayCommand::RestNow),
                id::TOGGLE_PAUSE => Some(TrayCommand::TogglePause),
                id::TOGGLE_SOUND => Some(TrayCommand::ToggleSound),
                id::QUIT => Some(TrayCommand::Quit),
                _ => None,
            };
            out.extend(cmd);
        }
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            match ev {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => out.push(TrayCommand::OpenSettings),
                _ => {}
            }
        }
        out
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled_item.set_checked(enabled);
    }

    pub fn set_sound(&self, on: bool) {
        self.sound_item.set_checked(on);
    }

    /// 全局热键启用/关闭时同步菜单上的快捷键提示
    pub fn set_hotkey_hint(&self, enabled: bool) {
        self.rest_item.set_text(if enabled {
            "立即休息\tCtrl+Shift+E"
        } else {
            "立即休息"
        });
    }

    pub fn set_paused(&self, paused: bool) {
        self.pause_item.set_text(if paused {
            "恢复提醒"
        } else {
            "暂停 1 小时"
        });
    }

    /// tooltip ≤ 128 字符（NOTIFYICONDATA.szTip 上限），内容变化时才写。
    pub fn set_tooltip(&mut self, text: &str) {
        if self.tooltip != text {
            let clipped: String = text.chars().take(120).collect();
            if self.icon.set_tooltip(Some(&clipped)).is_ok() {
                self.tooltip = text.to_string();
            }
        }
    }
}
