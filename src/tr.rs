//! 运行时查表层（ADR-0008 的「`tr(&str)` 函数」形态）。
//!
//! 形态选型与被否决的方案见 [ADR-0008](../docs/adr/0008-i18n-self-built-tr-layer.md)：
//! 没有 `enum Key` + `tr!` 宏，没有 Fluent / rust-i18n / i18n-embed，**零新增运行时依赖**。
//! 差异化的那一条能力是「缺任何语言的 key 直接让 `build.rs` `panic!` → 构建失败」，
//! 而这正是 `enum Key` 给不了、也不需要给的。
//!
//! ## 分配模型（不要把「零分配」写成空话）
//!
//! - **无参路径 `tr(key)`：真的零分配。** 返回 `&'static str`——静态表里的
//!   字符串字面量，`&'static` 意味着它活在二进制的只读段里。这里做的是一次
//!   `AtomicU8` 读取 + 一次 `binary_search`（170 个键 ≈ 8 次比较），**没有堆分配、
//!   没有 `String`、也没有 `format!`**。
//! - **带参路径 `trn` / `tr_fill`：一次 `String` 分配。** `str::replace` 必然产出
//!   新的 `String`——文案本身在静态表里，但替换结果不是。这类调用点数量很少
//!   （设置窗的 `SettingsAction` 载荷、托盘 tooltip、统计行），且本来就已经在
//!   分配：设置窗每个 `push_save` 都要 clone 整个 `Config`。
//!   把它们说成「零分配」只会让下一个读代码的人按错误的模型做判断。
//!
//! ## 为什么枚举的 `label()` 搬进了 locale 表
//!
//! `FlowSensitivity` / `SoundPreset` / `WallpaperFit` / `ContextState` 原本
//! 各带一个 `label() -> &'static str`。若让它们内部读 `AtomicU8`，`config.rs` 的
//! 10 个测试与 `core.rs` 的 14 个测试会变成**顺序相关**：一个把语言设成 `EnUs` 的
//! 测试会污染同进程里后续所有测试。改为「枚举只给出 locale 的 key，文案由调用点
//! `tr(key)` 现取」之后，`config.rs` / `core.rs` 保持纯数据、零测试污染，
//! 加第三种语言时也永不触碰领域层。

use std::sync::atomic::{AtomicU8, Ordering};

use crate::i18n::{EN_US, KEYS, ZH_CN};

/// 界面语言。`as u8` 就是 `AtomicU8` 里的编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    ZhCn = 0,
    EnUs = 1,
}

impl Lang {
    /// 解析 `config.toml` 的 `language` 字段。
    ///
    /// **`en` 前缀（大小写不敏感）→ 英文；其余一切 → `zh-CN`。**
    ///
    /// 初版这里是严格白名单（`en` / `en-US` / `en_US`），但那样 `en-GB`（真实存在
    /// 的 locale）会拿到中文界面，而用户在界面里看不出原因。前缀匹配让两侧都成立：
    /// `en-GB` / `en-AU` 得英文（用户显然要英文），`chinese` / `klingon` / `zh_CN` /
    /// 空串得中文。唯一残留风险是 `eno` 这类笔误拿到英文——**但那个错误在界面上
    /// 是可见的、且能改回**，与「静默把用户配置改掉」不同质。
    pub fn from_config(value: &str) -> Lang {
        if value.trim().to_ascii_lowercase().starts_with("en") {
            Lang::EnUs
        } else {
            Lang::ZhCn
        }
    }

    /// 该语言在静态表里的取值数组（与 `KEYS` 同序）。
    fn table(self) -> &'static [&'static str; crate::i18n::KEY_COUNT] {
        match self {
            Lang::ZhCn => &ZH_CN,
            Lang::EnUs => &EN_US,
        }
    }

    /// 该语言的 BCP-47 写法，落盘用。
    pub fn code(self) -> &'static str {
        match self {
            Lang::ZhCn => "zh-CN",
            Lang::EnUs => "en-US",
        }
    }

    /// 设置窗语言选择器的取值顺序。
    pub const ALL: [Lang; 2] = [Lang::ZhCn, Lang::EnUs];

    /// 该语言**自己**的名字（`lang.*`）。
    pub fn key(self) -> &'static str {
        match self {
            Lang::ZhCn => "lang.zh_cn",
            Lang::EnUs => "lang.en_us",
        }
    }
}

/// 进程级语言状态。`AtomicU8` 是因为托盘的 `apply_language` 与 `UiSession`
/// 分处不同线程，而语言切换本身发生在主线程的 `Runtime::step` 里。
static LANGUAGE: AtomicU8 = AtomicU8::new(Lang::ZhCn as u8);

pub fn set_language(lang: Lang) {
    LANGUAGE.store(lang as u8, Ordering::Relaxed);
}

pub fn language() -> Lang {
    match LANGUAGE.load(Ordering::Relaxed) {
        1 => Lang::EnUs,
        _ => Lang::ZhCn,
    }
}

/// 无参查表：返回当前语言下的文案。
///
/// `KEYS` 由 `build.rs` 按字典序生成，所以这里可以二分。查不到就原样返回 `key`
/// ——「忘了翻」在界面上会显示成 `tray.open_settings`，这正是 T7 两条断言要防的
/// 症状；`build.rs` 已经保证了查不到只可能是调用点拼错。
///
/// **零分配**：命中时返回值指向静态表里的字符串字面量（`&'static`，住在只读段）。
pub fn tr(key: &str) -> &str {
    let table = language().table();
    // `binary_search_by` 而不是 `binary_search`：后者的 needle 类型是 `&&'static str`，
    // 会把调用点的 `&str` 生命周期强行拉成 `'static`。
    match KEYS.binary_search_by(|p| (*p).cmp(key)) {
        Ok(i) => table[i],
        Err(_) => key,
    }
}

/// 带参查表：把 `{name}` 换成给定值。
///
/// **一次 `String` 分配**（`str::replace` 的固有代价），见模块说明。
pub fn tr_fill(key: &str, name: &str, value: impl std::fmt::Display) -> String {
    tr(key).replace(name, &value.to_string())
}

/// 复数查表：`n == 1` 取 `|` 前的第 1 段，否则取第 2 段；只有一个形态时
/// （例如中文「今日休息 3 次」两种形态相同）也照常工作。
///
/// `runtime.rs` 的托盘统计行是这条约定存在的原因：它一个 key 里塞了三个计数，
/// `|` 的二元约定表达不了 per-placeholder 复数，所以那里拆成三个单数 key 后拼装。
pub fn trn(key: &str, n: impl Into<u64>) -> String {
    let n = n.into();
    let raw = tr(key);
    let form = match raw.split_once('|') {
        Some((one, other)) => {
            if n == 1 {
                one
            } else {
                other
            }
        }
        None => raw,
    };
    form.replace("{n}", &n.to_string())
}

/// 托盘今日统计行：三个计数各自独立决定复数形态后拼装。
///
/// ADR-0008 §(a)：这行用户天天看，一个 key 里的三个计数必须拆开。
pub fn tray_stats_line(completed: u32, skipped: u32, postponed: u32) -> String {
    format!(
        "{}{}{}{}{}",
        trn("stats.done", completed),
        tr("stats.separator"),
        trn("stats.skipped", skipped),
        tr("stats.separator"),
        trn("stats.postponed", postponed),
    )
}
