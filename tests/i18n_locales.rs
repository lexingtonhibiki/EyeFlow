//! ADR-0008 i18n **数据层**确定性测试（T1 / T2 / T3 / T7a / T9a / T12a / T15）。
//!
//! 规格书：`docs/adr/0008-i18n-self-built-tr-layer.md` §「测试策略」。
//! 这些测试只读 `locales/*.toml` 与 `build.rs` 的生成物，**不依赖任何尚不存在的
//! Rust API**，所以它们今天就能编译、并且应当是红的（`locales/` 尚不存在）。
//!
//! 与 `src/` 内的单测不同：这些测试不需要 `egui::Context`、不建窗口、不碰
//! `%APPDATA%`，因此可以在任意机器上跑，也不会污染用户配置。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const ZH: &str = "zh-CN";
const EN: &str = "en-US";

/// 语言代码 → locale 源文件名（ADR-0008 数据流图的第一层）
fn locale_path(code: &str) -> PathBuf {
    Path::new(ROOT).join("locales").join(format!("{code}.toml"))
}

/// 读一个 locale 源文件。文件缺失时 panic —— 这本身就是 T1 该报的错。
fn load(code: &str) -> BTreeMap<String, String> {
    let path = locale_path(code);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "缺少 locale 源文件 {}（{}）—— ADR-0008 要求 locales/{}.toml 与 \
             build.rs 生成流程落地；读不到: {e}",
            path.display(),
            code,
            code
        )
    });
    let table: toml::Table = raw.parse().unwrap_or_else(|e| {
        panic!(
            "{} 不是合法的扁平 TOML（key = \"value\" 单行）: {e}",
            path.display()
        )
    });
    table
        .into_iter()
        .map(|(k, v)| {
            let s = v.as_str().unwrap_or_else(|| {
                panic!(
                    "{} 的 {k} 不是字符串—— locale 表必须是纯字符串值",
                    path.display()
                )
            });
            (k, s.to_owned())
        })
        .collect()
}

/// 一条文案的复数形态（`|` 二元分隔，ADR-0008 §「参数化与复数」）
fn forms<S: AsRef<str>>(value: S) -> Vec<String> {
    value.as_ref().split('|').map(str::to_owned).collect()
}

/// 全部形态里出现过的占位符名集合
fn placeholders<S: AsRef<str>>(value: S) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for form in forms(value) {
        let mut rest = form.as_str();
        while let Some(open) = rest.find('{') {
            let after = &rest[open + 1..];
            let close = after
                .find('}')
                .unwrap_or_else(|| panic!("未闭合的占位符 `{{`: {form:?}"));
            out.insert(after[..close].to_owned());
            rest = &after[close + 1..];
        }
    }
    out
}

/// key 的形状约定（plan-v0.6 §2.2「key 全小写点分」）
fn key_is_well_formed(key: &str) -> bool {
    !key.is_empty()
        && key == key.to_ascii_lowercase()
        && !key.starts_with('.')
        && !key.ends_with('.')
        && !key.contains("..")
        && key.split('.').all(|seg| {
            !seg.is_empty()
                && seg
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
}

/// `build.rs` 生成物落点：`<target>/{debug,release}/build/eyeflow-*/out/i18n.rs`
/// （ADR-0008 数据流图的最后一段；文件名按 plan-v0.6 §2.2 `src/i18n.rs` 的注释）
fn newest_generated_i18n() -> Option<PathBuf> {
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(ROOT).join("target"));
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for profile in ["debug", "release"] {
        let build = target_dir.join(profile).join("build");
        let Ok(entries) = std::fs::read_dir(&build) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("eyeflow-") {
                continue;
            }
            for candidate in [e.path().join("out").join("i18n.rs")] {
                let Ok(md) = std::fs::metadata(&candidate) else {
                    continue;
                };
                let Ok(t) = md.modified() else { continue };
                if best.as_ref().is_none_or(|(bt, _)| t > *bt) {
                    best = Some((t, candidate));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

// ---------------------------------------------------------------------------
// T1 键对齐：两个 locale 文件的键集合完全相同，且非空
// 依据：ADR-0008 §「数据流」——以 zh-CN 为唯一事实来源，en-US 缺键/多键 → 构建 panic。
// ---------------------------------------------------------------------------
#[test]
fn t1_locale_key_sets_are_identical_and_non_empty() {
    let zh = load(ZH);
    let en = load(EN);

    assert!(
        !zh.is_empty(),
        "locales/{ZH}.toml 没有任何键 —— 它是唯一事实来源，空表说明生成流程没落地"
    );
    assert_eq!(
        zh.len(),
        en.len(),
        "键数不同：{ZH}={} 键，{EN}={} 键（ADR-0008 要求两者键集合完全相同，\
         缺键/多键都应在 build.rs 里 panic）",
        zh.len(),
        en.len()
    );

    let only_zh: Vec<&String> = zh.keys().filter(|k| !en.contains_key(*k)).collect();
    let only_en: Vec<&String> = en.keys().filter(|k| !zh.contains_key(*k)).collect();
    assert!(
        only_zh.is_empty(),
        "en-US.toml 缺 {ZH} 的 {len} 个键（build.rs 本应 panic）: {only_zh:?}",
        len = only_zh.len()
    );
    assert!(
        only_en.is_empty(),
        "en-US.toml 多出 {ZH} 没有的 {len} 个键（build.rs 本应 panic）: {only_en:?}",
        len = only_en.len()
    );
}

#[test]
fn t1b_locale_keys_are_lowercase_dot_separated() {
    for code in [ZH, EN] {
        let table = load(code);
        for key in table.keys() {
            assert!(
                key_is_well_formed(key),
                "{code} 的键 {key:?} 不符合「全小写点分」约定（plan-v0.6 §2.2）"
            );
        }
    }
}

/// 生成物与源文件一致：build.rs 生成的表必须覆盖 zh-CN 的每一个键。
/// 这是「构建期校验真的跑了」的唯一可观测证据——T1 只看源文件，看不到生成物。
#[test]
fn t1c_generated_table_covers_every_source_key() {
    let zh = load(ZH);
    let en = load(EN);
    let gen = newest_generated_i18n().unwrap_or_else(|| {
        panic!(
            "找不到 build.rs 的生成物 i18n.rs（应在 <target>/debug/build/eyeflow-*/out/i18n.rs）\
             —— ADR-0008 要求 build.rs 用 str::lines() 解析 locales/*.toml 并生成静态表"
        )
    });
    let text =
        std::fs::read_to_string(&gen).unwrap_or_else(|e| panic!("读 {} 失败: {e}", gen.display()));

    for key in zh.keys().chain(en.keys()) {
        assert!(
            text.contains(&format!("\"{key}\"")),
            "生成物 {} 里找不到键 {key:?} —— 静态表与 locales/*.toml 漂移",
            gen.display()
        );
    }
}

// ---------------------------------------------------------------------------
// T2 占位符对齐
// 规则（ADR-0008 §测试策略，**不是**「完全相同」）：英文允许是中文的子集，
// 但不得出现中文没有的占位符名（英文单数形态可以写死 `1` 而没有 `{n}`）。
// ---------------------------------------------------------------------------
#[test]
fn t2_english_placeholders_are_a_subset_of_chinese() {
    let zh = load(ZH);
    let en = load(EN);

    let mut violations: Vec<String> = Vec::new();
    for (key, en_value) in &en {
        let Some(zh_value) = zh.get(key) else {
            continue; // 键不齐由 T1 负责报
        };
        let en_ph = placeholders(en_value);
        let zh_ph = placeholders(zh_value);
        for name in en_ph.difference(&zh_ph) {
            violations.push(format!(
                "{key}: 英文有 {{{name}}}，中文没有（中文: {zh_value:?}）"
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "占位符对齐失败 {} 处 —— 英文表出现中文没有的占位符名，\
         调用点会把 {{n}} 之类替换到一个不存在的字段上:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn t2b_placeholders_are_well_formed() {
    for code in [ZH, EN] {
        for (key, value) in load(code) {
            for form in forms(&value) {
                let mut depth = 0i32;
                for c in form.chars() {
                    match c {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                    assert!(depth <= 1, "{code} 的 {key:?} 里嵌套/多余的 `{{`: {form:?}");
                }
                assert_eq!(depth, 0, "{code} 的 {key:?} 里有未闭合的 `{{`: {form:?}");
            }
            // 占位符名只能是标识符（调用点写成 .replace("{name}", …)）
            for name in placeholders(&value) {
                assert!(
                    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                    "{code} 的 {key:?} 占位符名 {name:?} 不是标识符"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// T3 无空串：任一语言的任一键不得为空串或纯空白；复数形态也不得为空
// ---------------------------------------------------------------------------
#[test]
fn t3_no_empty_or_blank_values() {
    for code in [ZH, EN] {
        for (key, value) in load(code) {
            assert!(
                !value.trim().is_empty(),
                "{code} 的 {key:?} 是空串/纯空白 —— 界面上会是一块空白区域"
            );
            for (i, form) in forms(&value).iter().enumerate() {
                assert!(
                    !form.trim().is_empty(),
                    "{code} 的 {key:?} 第 {i} 个复数形态是空串: {value:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// T7a 查表的前置性质（数据层）：任何一条文案的取值都不能等于它自己的 key。
// 运行时版 T7b 在 src/ 单测里（`tr(key)` 不返回 key 本身），因为它需要尚不存在的
// `crate::tr`；这一条能在今天就把「忘了翻 → 原样显示 key」在数据上抓住。
// ---------------------------------------------------------------------------
#[test]
fn t7a_no_value_equals_its_own_key() {
    for code in [ZH, EN] {
        for (key, value) in load(code) {
            assert_ne!(
                value, key,
                "{code} 的 {key:?} 的值就是 key 本身 —— 说明这条没翻（i18n-embed 式 \
                 实现漏 key 时正是显示 key 本身）"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T9a 托盘 7 项文案在两种语言下均非空
// 现状：tray.rs:41-65 一次性硬编码 7 条中文，运行时.rs:245 的今日统计行是
// 「今日休息 {} 次 · 跳过 {} · 延后 {}」三计数（ADR-0008 风险清单第 1 条，
// 必须拆成单数 key 后拼装，所以 stats.* 下必须有 3 条带复数形态的 key）。
// ---------------------------------------------------------------------------
#[test]
fn t9a_tray_menu_keys_exist_and_are_non_empty_in_both_languages() {
    let zh = load(ZH);
    let en = load(EN);

    // tray.rs 的 6 条固定项（今日统计行是第 7 项，形态不同，见下）
    for key in [
        "tray.open_settings",
        "tray.toggle_enabled",
        "tray.rest_now",
        "tray.toggle_pause",
        "tray.toggle_sound",
        "tray.quit",
    ] {
        for (code, table) in [(ZH, &zh), (EN, &en)] {
            let value = table.get(key).unwrap_or_else(|| {
                panic!("{code} 缺托盘文案 key {key:?}（tray.rs:41-65 今天的 7 项硬编码中文）")
            });
            assert!(!value.trim().is_empty(), "{code} 的 {key:?} 是空串");
        }
    }

    // 第 7 项：runtime.rs:245 的三计数统计行。ADR-0008 §(a) 要求拆成单数 key，
    // 因此两个语言表里都要有 ≥3 条带复数形态（en 含 `|`）的 stats 计数 key。
    let plural_stats_en = en
        .iter()
        .filter(|(k, v)| k.starts_with("stats.") && v.contains('|'))
        .count();
    assert!(
        plural_stats_en >= 3,
        "en-US 只有 {plural_stats_en} 条带复数形态的 `stats.*` key，\
         至少需要 3 条（done / skipped / postponed）—— runtime.rs:245 的三计数行 \
         不能整条塞进一个 key，`|` 二元约定表达不了 per-placeholder 复数（ADR-0008 §a）"
    );
}

// ---------------------------------------------------------------------------
// T12a `human_duration` 的英文形态不得出现带 0 的分钟
// 依据：ADR-0008 §「风险集中在 15 条」第 3 条 —— ui.rs:691-700 今天输出
// `"{n} 小时 {m} 分"`，英文直译成 `"{n} h {m} min"` 会在 m == 0 时显示
// `"2 h 0 min"`，需要一条 m > 0 的分支。
// 数据层的等价断言：英文表里不应再有需要 `{m}` 的复合形态。
// 运行时版 T12b 在 src/ui.rs 单测里。
// ---------------------------------------------------------------------------
#[test]
fn t12a_english_never_emits_a_zero_minute_form() {
    for (key, value) in load(EN) {
        assert!(
            !value.contains("0 min"),
            "en-US 的 {key:?} 写死了 `0 min`: {value:?} —— human_duration 在 \
             m == 0 时会显示成「2 h 0 min」（ADR-0008 风险 3）"
        );
        for form in forms(&value) {
            assert!(
                !form.contains("{m}"),
                "en-US 的 {key:?} 形态 {form:?} 仍带 `{{m}}` —— 英文形态不该需要分钟 \
                 占位符，m == 0 的分支应由「只拼小时」解决（ADR-0008 风险 3）"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T15 英文表真的被翻了 —— **「英文没翻」这一种失败模式的前哨**
//
// 为什么 T7a 不够：T7a 只拦住「值被写成 key 本身」这一种粘贴事故，它给译者
// 一种虚假的安全感。真正的失败模式是**把中文原样留进英文表**——键齐全、值非空、
// 占位符对齐、没有任何一个是 key 本身，测试全绿，而英文界面上是一整片中文。
//
// T7b 已经有一点跨语言比较，但它比的是「**至少有一个键不同**」
// （`src/config.rs` 的 `differs_somewhere`），那是布尔值，不报是哪几个键。
// 实测（T15 落地前的伪失败验证）：把 `tray.toggle_enabled` 单独改成中文、
// 其余 177 条保持英文 —— **T7a / T7b / T1 / T2 / T3 / T9a / T12a 全部照样绿**，
// 只有 T15 变红并点名是哪个键、值是什么。这才是译者真正会犯的错，
// 也是本条存在的理由。
//
// 附一条实测更正（别照抄「整份替换全绿」的旧说法）：把 `locales/en-US.toml`
// 整份替换成 `zh-CN.toml` 的内容，**并不是全绿**——T7b（`differs_somewhere` 变
// false）与 T9a（英文 `stats.*` 带 `|` 的 key 数为 0 < 3）都会红。但那两条是**顺带**
// 抓到的：它们抓的是「复数约定」与「两表全等」，不是「漏翻」；把 zh 的表整体搬
// 过去才会触发。**局部漏翻（几条键没翻）此前无人看守。**
//
// 断言的形状不是「全部必须不同」：确实有一小批键**在两种语言下本来就该是同一个
// 字符串**（品牌名、纯符号、纯单位、专有名词）。这些进 [`ALLOWED_IDENTICAL`]，
// 每一条都带理由。其余任何一条 `en[k] == zh[k]` 都判为漏翻。
// ---------------------------------------------------------------------------

/// 允许 `en[k] == zh[k]` 的键，附「为什么它可以相同」的理由。
///
/// **每加一条都要能回答「这句话翻译成英文应该变吗？」——答案是「不应该」。**
/// 理由写成字符串而不是注释，是为了让「白名单里塞了没写理由的键」也能被断言抓到。
///
/// 现状：178 个键里 6 个相同（3.4%）。T15 另有一条断言把白名单长度钉死在键数的
/// 10% 以内，使「加白名单」不会退化成绕开 T15 的后门。
const ALLOWED_IDENTICAL: &[(&str, &str)] = &[
    (
        "lang.en_us",
        "语言名必须是母语名：英文界面显示「English」而不是「英语」。中文界面同样 \
         显示「English」（同一张表里 `lang.zh_cn` 才显示「简体中文」），两边相同。",
    ),
    (
        "runtime.tooltip_head",
        "「EyeFlow — {state}」= 品牌名 + 分隔符 + 占位符。`{state}` 在运行时由 \
         `core.state_*` 的英文/中文取值填入，这一段本身不含可翻译的英文。",
    ),
    (
        "stats.dot",
        "纯符号 `●`（U+25CF），状态条里的计数分隔点，不是文字。",
    ),
    (
        "stats.separator",
        "纯分隔符 ` · `（含两侧空格），用来拼装状态条，不是文字。",
    ),
    (
        "unit.percent",
        "纯单位 ` %`，与 `unit.second` 的 ` s` / `unit.minute` 的 ` min` 同类：\
         单位本身没有语言差异。",
    ),
    (
        "update.api_error",
        "「GitHub API: {msg}」= 产品专有名词 + 占位符。GitHub 是品牌名，两种语言 \
         都不翻译，`{msg}` 是原文回显。",
    ),
];

#[test]
fn t15_english_is_actually_translated_not_a_copy_of_chinese() {
    let zh = load(ZH);
    let en = load(EN);

    // (1) 白名单不得腐化：每一条都要有理由，且键必须真的存在于两个表中。
    //     少了这一步，白名单会退化成「写了就不报」的万能豁免。
    for (key, why) in ALLOWED_IDENTICAL {
        assert!(
            !why.trim().is_empty(),
            "白名单条目 {key:?} 没写「为什么它可以两种语言相同」的理由"
        );
        assert!(
            zh.contains_key(*key),
            "白名单里的 {key:?} 在 {ZH} 里已经不存在了（改名或删除？）—— \
             请把它从 ALLOWED_IDENTICAL 里删掉，别留着一条永不生效的豁免"
        );
        assert!(
            en.contains_key(*key),
            "白名单里的 {key:?} 在 {EN} 里不存在了 —— 键不齐由 T1 负责报"
        );
    }

    // (2) 白名单必须**小**。这是它唯一的防线作用：白名单一旦变肥，T15 就失效了。
    //     10% 是一个远离现状（3.4%）但仍能抓住「整份中文贴进英文表」的阈值。
    let cap = (zh.len() / 10).max(1);
    assert!(
        ALLOWED_IDENTICAL.len() <= cap,
        "ALLOWED_IDENTICAL 有 {} 条，超过键数的 10%（{cap}）—— 白名单是「本来就该 \
         相同」的例外清单，不是「懒得翻」的收容所。请真正翻译那些键，而不是加白名单。",
        ALLOWED_IDENTICAL.len()
    );

    // (3) 主断言：任何 `en[k] == zh[k]` 的键都必须落在白名单里。
    let allowed: BTreeSet<&str> = ALLOWED_IDENTICAL.iter().map(|(k, _)| *k).collect();
    let untranslated: Vec<String> = zh
        .iter()
        .filter(|(k, zh_value)| en.get(*k) == Some(*zh_value) && !allowed.contains(k.as_str()))
        .map(|(k, v)| format!("  {k:>28} = {v}"))
        .collect();

    assert!(
        untranslated.is_empty(),
        "{} 个键的英文取值与中文**完全相同**，而它们不在 ALLOWED_IDENTICAL 里—— \
         这是漏翻，不是「本来就该一样」。英文字面上会原样显示中文。\n\
         两种可能：(a) 真的漏翻了，补上英文；(b) 确实该相同（比如品牌名 / 纯符号 / \
         纯单位），把它加进 ALLOWED_IDENTICAL 并写明理由。\n\
         键 = 值：\n{}",
        untranslated.len(),
        untranslated.join("\n")
    );
}
