//! 系统托盘：图标、右键菜单、左键/双击事件。
//!
//! 菜单顺序遵循微软 UX 指南（docs/research/04）：默认命令在首位、常用开关带勾选、Exit 在末尾。
//! 托盘图标必须在运行 Win32 消息循环的线程上创建——即 eframe 主线程、`run_native` 之前。

use std::cell::Cell;

use crate::tr::tr;
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
    // 7 个菜单项**全部**是字段。`open_item` / `quit_item` 在 v0.5.2 里还是
    // `Tray::new` 的局部变量，`apply_language` 改不到它们——ADR-0008 §落地前置
    // 点名的就是这一条：切语言后「打开设置」和「退出」两项会保持旧语言。
    open_item: MenuItem,
    enabled_item: CheckMenuItem,
    rest_item: MenuItem,
    sound_item: CheckMenuItem,
    pause_item: MenuItem,
    /// 只读的今日统计行（禁用态菜单项）
    stats_item: MenuItem,
    quit_item: MenuItem,
    tooltip: String,
    /// `pause_item` 当前显示的状态，用于 `set_paused` 的变更守卫
    paused: Cell<bool>,
    /// `rest_item` 的热键提示当前是否带加速键（`set_hotkey_hint` 的变更守卫）
    hotkey_shown: Cell<bool>,
}

impl Tray {
    pub fn new(enabled: bool, sound_on: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let open_item = MenuItem::with_id(
            MenuId(id::OPEN_SETTINGS.into()),
            tr("tray.open_settings"),
            true,
            None,
        );
        let enabled_item = CheckMenuItem::with_id(
            MenuId(id::TOGGLE_ENABLED.into()),
            tr("tray.toggle_enabled"),
            true,
            enabled,
            None,
        );
        let rest_item = MenuItem::with_id(
            MenuId(id::REST_NOW.into()),
            tr("tray.rest_now_hotkey"),
            true,
            None,
        );
        let pause_item = MenuItem::with_id(
            MenuId(id::TOGGLE_PAUSE.into()),
            tr("tray.toggle_pause"),
            true,
            None,
        );
        let sound_item = CheckMenuItem::with_id(
            MenuId(id::TOGGLE_SOUND.into()),
            tr("tray.toggle_sound"),
            true,
            sound_on,
            None,
        );
        let quit_item = MenuItem::with_id(MenuId(id::QUIT.into()), tr("tray.quit"), true, None);
        let stats_item = MenuItem::with_id(
            MenuId("stats".into()),
            crate::tr::tray_stats_line(0, 0, 0),
            false,
            None,
        );

        let menu = Menu::new();
        menu.append(&open_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&enabled_item)?;
        menu.append(&rest_item)?;
        menu.append(&pause_item)?;
        menu.append(&sound_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&stats_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit_item)?;

        // 按 DPI 动态渲染托盘图标（16/24/32/48…），避免固定 32×32 被 Shell 拉伸发糊
        #[allow(clippy::missing_safety_doc)]
        let icon_size = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_CXICON,
            )
            .clamp(16, 64) as u32
        };
        let rgba = crate::icon::render_rgba(icon_size);
        let icon = Icon::from_rgba(rgba, icon_size, icon_size)?;

        let tooltip = tr("tray.tooltip").to_string();
        let tray = TrayIconBuilder::new()
            .with_id("eyeflow")
            .with_tooltip(&tooltip)
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?;

        Ok(Self {
            icon: tray,
            open_item,
            enabled_item,
            rest_item,
            sound_item,
            pause_item,
            stats_item,
            quit_item,
            tooltip,
            // 菜单初始文本是「暂停 1 小时」，即未暂停
            paused: Cell::new(false),
            // rest_item 初始带加速键
            hotkey_shown: Cell::new(true),
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
        if self.hotkey_shown.get() == enabled {
            return;
        }
        self.hotkey_shown.set(enabled);
        // 加速键靠 `\t`（Rust 转义出的真制表符）分隔，由 locale 的
        // `tray.rest_now_hotkey` 提供——翻译这条时不得破坏那个制表符
        self.rest_item.set_text(if enabled {
            tr("tray.rest_now_hotkey")
        } else {
            tr("tray.rest_now")
        });
    }

    /// 今日统计行（内容变化时才写）
    pub fn set_stats_line(&self, text: &str) {
        if self.stats_item.text() != text {
            self.stats_item.set_text(text);
        }
    }

    /// 暂停菜单项（内容变化时才写）。
    ///
    /// 与 `set_stats_line` / `set_tooltip` 同款：muda 的 `MenuItem::set_text`
    /// 每次都是一次 `SetMenuItemInfoW`，而 `step()` 每个 tick 都会调到，
    /// 无条件写等于把系统调用铺在整条热路径上。
    pub fn set_paused(&self, paused: bool) {
        if self.paused.get() != paused {
            self.paused.set(paused);
            self.pause_item.set_text(if paused {
                tr("tray.resume")
            } else {
                tr("tray.toggle_pause")
            });
        }
    }

    /// 语言切换后逐项重写托盘文案（ADR-0008 §运行时切换的刷新面）。
    ///
    /// 托盘是这个应用**唯一的高频入口**——用户每天点它几十次——所以中英混杂的
    /// 托盘是 i18n 里最刺眼的一类 bug。`MenuId` 永远是 `mod id` 里的 ASCII 常量，
    /// 这里只碰 `set_text`，所以切语言后 `poll()` 的 match 不会失配
    /// （这是 ADR-0008 点名的红线）。
    pub fn apply_language(&mut self, lang: crate::tr::Lang) {
        crate::tr::set_language(lang);
        self.open_item.set_text(tr("tray.open_settings"));
        self.enabled_item.set_text(tr("tray.toggle_enabled"));
        self.sound_item.set_text(tr("tray.toggle_sound"));
        self.quit_item.set_text(tr("tray.quit"));
        self.rest_item.set_text(if self.hotkey_shown.get() {
            tr("tray.rest_now_hotkey")
        } else {
            tr("tray.rest_now")
        });
        self.pause_item.set_text(if self.paused.get() {
            tr("tray.resume")
        } else {
            tr("tray.toggle_pause")
        });
        // tooltip 由 runtime 立刻重算并经 set_tooltip 推过来；这里先把本地
        // 影子副本对齐，好让 runtime 的「先比指纹」守卫看到一次真实变化
        self.tooltip.clear();
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
