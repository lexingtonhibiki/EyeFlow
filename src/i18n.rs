//! 生成的 i18n 静态表（`locales/*.toml` → `build.rs` → `$OUT_DIR/i18n.rs`）。
//!
//! 本文件只做 `include!`，不放任何手写代码：**键集合的唯一事实来源是
//! `locales/zh-CN.toml`**，两个语言的键集合不一致时 `build.rs` 会 `panic!`
//! 让构建失败（ADR-0008 的差异化能力）。
//!
//! 导出的三个静态量：
//! - `KEYS`   按字典序排列的键（`tr()` 二分查找的依据）
//! - `ZH_CN`  与 `KEYS` 同序的中文取值
//! - `EN_US`  与 `KEYS` 同序的英文取值
//!
//! 三个数组长度都是 `KEY_COUNT`，所以 `KEYS[i]` / `ZH_CN[i]` / `EN_US[i]`
//! 指向同一条文案——T7b 正是断言这个不变式。

include!(concat!(env!("OUT_DIR"), "/i18n.rs"));
