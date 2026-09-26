//! T9b / T13：托盘的 `MenuId` 红线（ADR-0008 §「运行时切换的刷新面」+ plan-v0.6 §5 D7）。
//!
//! 「`tray.rs:19-26` 的 `MenuId` 必须永久保持 ASCII 标识符、永不翻译」——否则
//! `tray.rs:114` 的 `match ev.id.0.as_str()` 在切语言后全失配，**托盘彻底失效**。
//! 这条红线今天已满足（已是 `pub const OPEN_SETTINGS: &str = "open_settings"`），
//! 本文件的作用是让任何 i18n 重构都改不坏它。
//!
//! 为什么放在 `tests/` 而不是 `src/tray.rs` 的 `#[cfg(test)]`：`mod id` 与 `struct Tray`
//! 的字段都是私有的，集成测试够不着；而本轮另一条约束是**不碰并行 agent 正在改的
//! `src/tray.rs`**。所以这里做「源码契约」断言——只读那一个文件、只查那几个已钉死的
//! 标识，不做 ADR-0008 明确砍掉的「正则扫 `src/**/*.rs` 找裸中文」（T4）。

use std::path::Path;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn tray_src() -> String {
    std::fs::read_to_string(Path::new(ROOT).join("src").join("tray.rs"))
        .expect("读 src/tray.rs 失败")
}

/// 从 `mod id { … }` 里取出 `(常量名, 字面值)`
fn id_constants(src: &str) -> Vec<(String, String)> {
    let start = src
        .find("mod id {")
        .unwrap_or_else(|| panic!("src/tray.rs 里找不到 `mod id`（tray.rs:19-26）"));
    let body_start = start + "mod id {".len();
    let mut depth = 1i32;
    let mut i = body_start;
    for (j, c) in src[body_start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    i = body_start + j;
                    break;
                }
            }
            _ => {}
        }
    }
    src[body_start..i]
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("pub const ")?;
            let (name, value) = rest.split_once(": &str = ")?;
            let value = value.strip_prefix('"')?.strip_suffix("\";")?;
            Some((name.trim().to_owned(), value.to_owned()))
        })
        .collect()
}

/// 抽出 `impl Tray { … }` 里某个方法的名字
fn has_tray_method(src: &str, name: &str) -> bool {
    src.lines()
        .any(|l| l.trim_start().starts_with(&format!("pub fn {name}(")))
        || src.contains(&format!("fn {name}("))
}

/// `struct Tray { … }` 字段里，类型是 `MenuItem` / `CheckMenuItem` 的个数
/// （ADR-0008 §落地前置：`open_item` 与 `quit_item` 今天还是 `Tray::new` 的局部变量，
///  `apply_language` 要改这两项必须先把它们提升为字段）
fn stored_menu_item_fields(src: &str) -> usize {
    let start = src
        .find("pub struct Tray {")
        .unwrap_or_else(|| panic!("src/tray.rs 里找不到 `pub struct Tray`"));
    let body = &src[start + "pub struct Tray {".len()..];
    let end = body.find("\n}").unwrap_or(body.len());
    body[..end]
        .lines()
        .filter(|l| {
            let l = l.trim();
            !l.is_empty()
                && !l.starts_with("//")
                && (l.ends_with("MenuItem,") || l.ends_with("CheckMenuItem,"))
        })
        .count()
}

/// 抽出 `MenuId(...)` 的参数文本
fn menuid_args(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(i) = rest.find("MenuId(") {
        let after = &rest[i + "MenuId(".len()..];
        let mut depth = 1i32;
        let mut end = 0usize;
        for (j, c) in after.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = j;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(after[..end].trim().to_owned());
        rest = &after[end..];
    }
    out
}

// ---------------------------------------------------------------------------
// T9b：`MenuId` 集合本身是 ASCII 标识符，且托盘确实有语言切换入口
// ---------------------------------------------------------------------------
#[test]
fn t9b_menu_ids_are_ascii_identifiers() {
    let src = tray_src();
    let ids = id_constants(&src);
    assert_eq!(
        ids.len(),
        6,
        "`mod id` 里应有 6 个 MenuId 常量（tray.rs:19-26），实际 {} 个: {ids:?}",
        ids.len()
    );
    for (name, value) in &ids {
        assert!(
            value.is_ascii() && !value.is_empty(),
            "MenuId 常量 {name} 的值 {value:?} 不是 ASCII —— MenuId 是协议标识符，\
             翻译它会让 tray.rs:114 的 match 全失配（ADR-0008 红线）"
        );
        assert!(
            value
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "MenuId 常量 {name} 的值 {value:?} 不是小写下划线标识符"
        );
    }
    let names: Vec<&str> = ids.iter().map(|(n, _)| n.as_str()).collect();
    for want in [
        "OPEN_SETTINGS",
        "TOGGLE_ENABLED",
        "REST_NOW",
        "TOGGLE_PAUSE",
        "TOGGLE_SOUND",
        "QUIT",
    ] {
        assert!(
            names.contains(&want),
            "`mod id` 缺常量 {want}（tray.rs 的 6 条固定菜单项）"
        );
    }
}

#[test]
fn t9c_menu_id_never_receives_a_translated_string() {
    let src = tray_src();
    for arg in menuid_args(&src) {
        assert!(
            arg.is_ascii(),
            "MenuId({arg:?}) 的参数不是 ASCII —— MenuId 必须是稳定标识符，不能是翻译结果"
        );
        assert!(
            !arg.contains("tr(") && !arg.contains("tr!"),
            "MenuId({arg:?}) 收到了翻译结果 —— 切语言后 tray.rs:114 的 match 会全失配"
        );
    }
    // `poll()` 的 match 臂必须比对 id 常量（而不是字面量或翻译结果）
    if let Some(poll) = src.split("pub fn poll").nth(1) {
        let body: String = poll.chars().take(1400).collect();
        assert!(
            body.contains("ev.id.0.as_str()"),
            "找不到 `match ev.id.0.as_str()`（tray.rs:114）"
        );
        for want in ["id::OPEN_SETTINGS", "id::QUIT"] {
            assert!(
                body.contains(want),
                "poll() 的 match 臂里没有 {want} —— 匹配必须打在 id 常量上"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T13：托盘的 `apply_language` 不会破坏 `MenuId`
// ---------------------------------------------------------------------------
#[test]
fn t13_tray_has_a_language_apply_path_that_rewrites_all_items() {
    let src = tray_src();

    // 1. 落地前置（ADR-0008）：7 个菜单项都必须是 struct 字段，否则 apply_language
    //    改不到「打开设置」和「退出」这两项（它们今天是 Tray::new 的局部变量）
    let fields = stored_menu_item_fields(&src);
    assert!(
        fields >= 7,
        "struct Tray 只存了 {fields} 个菜单项字段 —— 托盘有 7 项，\
         「打开设置」「退出」还是 Tray::new 里的局部变量（tray.rs:41,64），\
         apply_language 改不到它们（ADR-0008 §落地前置）"
    );

    // 2. 语言切换入口
    assert!(
        has_tray_method(&src, "apply_language"),
        "src/tray.rs 没有 apply_language —— 切语言后托盘菜单（这个应用唯一的高频入口）\
         会中英混杂（plan-v0.6 §2.7 / ADR-0008 §刷新面）"
    );

    // 3. 它必须逐项 set_text 且同时刷 tooltip
    if let Some(idx) = src.find("fn apply_language") {
        let body: String = src[idx..].chars().take(2500).collect();
        assert!(
            body.contains("set_text"),
            "apply_language 里没有 set_text —— 菜单文案不会跟着语言变"
        );
        assert!(
            body.contains("set_tooltip"),
            "apply_language 里没有 set_tooltip —— 托盘 tooltip 不会跟着语言变"
        );
    }
}
