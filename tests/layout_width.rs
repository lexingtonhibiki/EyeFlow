//! **闸门 A：宽度不变量**（v0.6.1 目视验收事故的直接对策）。
//!
//! ## 这条测试为什么存在
//!
//! v0.6.1 目视验收时发现：**英文界面在 700 物理像素窗口（= 560 pt @1.25 DPI）下，
//! 右侧内容被裁掉**——`Postpone once` 那一行的滑条跑出右边界 +207 pt，两条复选框
//! 长标签冲出卡片边框。中文侧正常（汉字宽度远小于等长英文）。
//!
//! 根因之一是本版自己造成的：`src/app.rs` 的 `tune_style` 把
//! `spacing.slider_width` 从 100 抬到 220，两行并排滑条的行从 510.1 pt 涨到
//! 750.1 pt，越过 546.0 pt 的内容区右缘。
//!
//! **为什么既有 78 条测试一条都没抓到**：它们全部在验「字形能不能画出来」
//! （`i18n_glyphs.rs` 查字体 cmap）、或「locale 表一不一致」。**没有任何一条
//! 断言「排得下」**。字形覆盖回答的是"画不画得出来"，宽度回答的是"排不排得
//! 下去"——两个正交的问题，前者全绿不代表后者成立。这正是本版翻车的缺口。
//!
//! ## 不变量（本文件管辖的**全部**范围）
//!
//! > 默认 560 pt 窗口、546.0 pt 卡片内容区下：任何处在不换行布局
//! > （`ui.horizontal` / `horizontal_top` / `horizontal_centered`）里的控件，
//! > 单行渲染宽度必须 ≤ 546.0 pt，中英文各一遍。处在竖直布局或
//! > `ui.horizontal_wrapped` 里的控件由 egui 折行保证不横向裁切，
//! > 不在本闸门管辖内。
//!
//! **为什么竖直布局不在管辖范围内**（egui 0.36.1 源码，依据等级：源码）：
//! `Ui::wrap_mode()`（`egui-0.36.1/src/ui.rs:588-604`）的判定是
//! `layout.is_vertical() || layout.is_horizontal() && layout.main_wrap()`
//! ⇒ `TextWrapMode::Wrap`，否则 `Extend`。而 `ui.label` / `ui.weak` 走
//! `AtomLayout` 的 shrink 分支（`atomics/atom_layout.rs:282-295`：非 Extend 时
//! 至少把第一个文本 atom 标成 shrink），`ui.checkbox` 同理——**它们都会折行**。
//! 只有 `Extend`（`ui.horizontal` / `horizontal_top` / `horizontal_centered`，
//! 三者的 `main_wrap` 都是 `false`，见 `ui.rs:2314-2390`）才真的横向裁切。
//! `ui.horizontal_wrapped` 的 `main_wrap` 是 `true`（`ui.rs:2367`），不裁。
//!
//! ## 两条断言
//!
//! - **A1 内联文案**：每一条**内联** `ui.*` 文案（`ui.checkbox` / `ui.label` /
//!   `ui.weak` / `ui.colored_label` / `ui.button` / `ui.small_button` /
//!   `ui.selectable_value` / `ui.hyperlink_to` 的实参，**排除**
//!   `.on_hover_text(...)` 里的）**且落在 Extend 布局内**的那些，单行渲染宽度
//!   **≤ 546.0 pt**（= 560 pt 窗口扣掉 egui 窗口边框与 `Frame::group` 的 inner
//!   margin 后的卡片内容区宽）。中英文各跑一遍。
//! - **A2 行右缘**：`src/ui.rs` 里每一个 `ui.horizontal` 族的行**整体**真的
//!   交给 egui 布局一遍，断言该行右缘 **≤ 546.0 pt**。这一条才是本次事故的
//!   真正形状——单条文案都合格、并排之后越界。
//!
//! ## 测量是怎么做的（不 mock 任何东西）
//!
//! 真实 `egui::Context` + **从 `src/app.rs` 解析出来的真实字体链** + 从
//! `src/app.rs` 解析出来的 `tune_style` 间距 → `UiBuilder::max_rect(560 × 624)`
//! → `ui.group(...)` 复刻 `ui.rs` 的 `card()` → **真的 add 那些 widget**，读
//! `rect.width()` / `ui.min_rect().width()`。量的是控件实际占掉的宽度（含复选框
//! 方块 + `item_spacing`），不是"用字符串长度乘字号估的"。
//!
//! ## 四件必须自己解决的事（这里是做法）
//!
//! 1. **`tune_style` / 字体链是 `src/app.rs` 里的私有函数，测试拿不到。**
//!    本 crate 是纯二进制 crate（`src/main.rs`，没有 `lib.rs`），集成测试**根本
//!    无法 `use` 它**。所以这里走**解析源码**而不是复制常量：`tune_spacing()` 与
//!    `font_chain()` 都从 `src/app.rs` 现场读出 `tune_style` 的函数体与
//!    `ZH_FONTS` / `EN_FONTS` 两个常量。**因此不存在"抄一份会漂移"的问题**——
//!    产品改了 `tune_style` 或换了字体链，这里下一次跑就跟着变。
//!    （若将来想要一条真正的 seam，让 `app.rs` `pub` 出一份可测的 spacing，
//!    那是产品侧改动，见报告。）
//! 2. **哪些 key 是"内联"的、哪些是 hover 的**：从 `src/ui.rs` 剥掉注释后，
//!    括号配对取出每个 `on_hover_text(...)` 的**实参区间**，把里面的
//!    `tr("K")` / `trn("K", …)` / `tr_fill("K", …)` 字面量收进 hover 集合；
//!    其余出现在 `ui.checkbox(` / `ui.label(` / `ui.weak(` / `ui.colored_label(` /
//!    `ui.button(` / `ui.small_button(` / `ui.selectable_value(` /
//!    `ui.hyperlink_to(` 实参里的 locale key 就是内联。
//! 3. **运行期 key**（`tr(f.key())` / `tr(lang.key())` 这种）：枚举的
//!    `fn key(self)` 只在 `src/config.rs` / `src/core.rs` / `src/tr.rs` 里
//!    返回字符串，本文件把那些 match 分支的**全部取值**收进来当 key
//!    （见 `enum_keys` / `keys_in`）。收进来的是**该枚举**的取值：靠
//!    「`tr(<recv>.key())` 之前最近的一个 `for <recv> in <Enum>::ALL` 绑定」
//!    判定，判不出才退回全部枚举的并集（保守方向）。
//!    `desc_key()` 的取值全部在 `on_hover_text` 里，自动豁免。
//! 4. **中英文各跑一遍**：直接从 `locales/*.toml` 读两套取值，**不碰 `tr()`**。
//!    `tr()` 读进程级 `AtomicU8`（`src/tr.rs`），依赖它就等于把上一轮那次
//!    1/7 间歇性失败的偏斜依赖又请回来。本文件全程无全局状态、无共享可变状态。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const UI_RS: &str = "src/ui.rs";
const APP_RS: &str = "src/app.rs";
const CONFIG_RS: &str = "src/config.rs";
const CORE_RS: &str = "src/core.rs";
const TR_RS: &str = "src/tr.rs";

/// 验收现场那个窗口：700 物理像素 @ 1.25 DPI = 560 pt。
const WINDOW_W: f32 = 560.0;
const WINDOW_H: f32 = 624.0;
/// 卡片内容区宽。C 亲自无头复现的数字，**由 `fixture_content_width_is_546` 现场
/// 核对**——如果 egui 改了 `Frame::group` / 窗口边框的 margin，那条断言会先红，
/// 报错会指名"内容区宽度不再是 546.0"，而不会让后面所有宽度断言悄悄用错上限。
const EXPECTED_CONTENT_W: f32 = 546.0;
/// 允许的浮点抖动（egui 的 margin 累加是 f32）。
const WIDTH_TOL: f32 = 0.5;

// ---------------------------------------------------------------------------
// 源码读取 + 注释剥除
// ---------------------------------------------------------------------------

fn read_src(rel: &str) -> String {
    let p = Path::new(ROOT).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {}: {e}", p.display()))
}

/// 剥掉 `//`（含 `///`）与 `/* */`（可嵌套）注释，**并把注释字节替换成等长空格**。
///
/// 与 `tests/i18n_glyphs.rs` 里那个"删掉注释"的版本不同，这里必须**保长**：
/// 本文件后面所有断言的报错都要带**行号**，字节下标与原文一一对应才能算行号。
///
/// 走字节而不是 char：全部分隔符都是 ASCII，UTF-8 多字节序列里不会出现 ASCII
/// 字节，按字节扫描不会切坏中文字符（汉字的每个字节都 ≥ 0x80）。
fn strip_comments_keep_len(src: &str) -> String {
    let b = src.as_bytes();
    let mut out: Vec<u8> = b.to_vec();
    let (mut i, mut depth) = (0usize, 0usize);
    while i < b.len() {
        let c = b[i];
        if depth > 0 {
            if c == b'/' && b.get(i + 1) == Some(&b'*') {
                depth += 1;
                i += 2;
            } else if c == b'*' && b.get(i + 1) == Some(&b'/') {
                depth -= 1;
                i += 2;
            } else {
                if c != b'\n' {
                    out[i] = b' ';
                }
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            depth = 1;
            i += 2;
        } else if c == b'"' {
            i = skip_str_lit(b, i);
        } else if c == b'\'' {
            // `'<一个字符>'` 或 `'\<转义>'` 才是字符字面量；`'static` / `<'a>` 里的
            // `'` 是生命周期标注，原样输出。
            let len = if b.get(i + 1) == Some(&b'\\') {
                if b.get(i + 3) == Some(&b'\'') {
                    4
                } else {
                    0
                }
            } else if b.get(i + 2) == Some(&b'\'') {
                3
            } else {
                0
            };
            if len > 0 {
                i += len;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    String::from_utf8(out).expect("等长替换不改变字节数，输出必然是合法 UTF-8")
}

/// 跳过 `b[i] == b'"'` 处的字符串字面量，返回其后的字节下标。
fn skip_str_lit(b: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    while j < b.len() {
        if j >= b.len() {
            break;
        }
        if b[j] == b'\\' {
            j += 2;
        } else if b[j] == b'"' {
            return j + 1;
        } else {
            j += 1;
        }
    }
    b.len()
}

/// 字节下标 → 1 基行号。
fn line_of(src: &str, at: usize) -> usize {
    src[..at].bytes().filter(|b| *b == b'\n').count() + 1
}

/// 括号配对：给定 `(` 的下标，返回其匹配的 `)` 的下标（跳过字符串字面量）。
fn match_paren(b: &[u8], open: usize) -> usize {
    assert_eq!(b[open], b'(', "match_paren 的入参必须是 `(`");
    let mut depth = 0usize;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            b'"' => i = skip_str_lit(b, i),
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    panic!("括号不配对：`(` 在字节 {open}");
}

/// 括号配对：`o` 处是 `open`，`c` 处是 `close`，返回匹配的 `close` 下标。
fn match_bracket(b: &[u8], open: u8, close: u8, o: usize) -> usize {
    assert_eq!(b[o], open, "match_bracket 的入参位置必须是 {open:?}");
    let mut depth = 0usize;
    let mut i = o;
    while i < b.len() {
        match b[i] {
            b'"' => i = skip_str_lit(b, i),
            x if x == open => {
                depth += 1;
                i += 1;
            }
            x if x == close => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    panic!("{open:?} 在字节 {o} 处不配对");
}

fn match_brace(b: &[u8], o: usize) -> usize {
    match_bracket(b, b'{', b'}', o)
}

/// 区间 `span` 内所有 `needle` 的起始下标（跳过字符串字面量里的命中）。
fn find_all(b: &[u8], span: (usize, usize), needle: &[u8]) -> Vec<usize> {
    let (lo, hi) = span;
    let (mut out, mut i) = (Vec::new(), lo);
    while i + needle.len() <= hi {
        if &b[i..i + needle.len()] == needle {
            // 命中位置不能落在字符串字面量内部
            if !inside_str_lit(b, lo, i) {
                out.push(i);
            }
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

/// 下标 `at` 是否落在 `from..` 之内某个字符串字面量的内部。
fn inside_str_lit(b: &[u8], from: usize, at: usize) -> bool {
    let mut i = from;
    while i < at {
        if b[i] == b'"' {
            let end = skip_str_lit(b, i);
            if at > i && at < end {
                return true;
            }
            i = end;
        } else {
            i += 1;
        }
    }
    false
}

fn in_any(spans: &[(usize, usize)], at: usize) -> bool {
    spans.iter().any(|(lo, hi)| at > *lo && at < *hi)
}

fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn src_slice(b: &[u8], lo: usize, hi: usize) -> &str {
    std::str::from_utf8(&b[lo..hi]).expect("源码切片必然是合法 UTF-8（边界都在 ASCII 上）")
}

/// 解析 `b[i] == b'"'` 处的字符串字面量，返回 `(已解转义的内容, 结束下标)`。
fn parse_str_lit(b: &[u8], i: usize) -> Option<(String, usize)> {
    if b.get(i) != Some(&b'"') {
        return None;
    }
    let (mut s, mut j) = (String::new(), i + 1);
    while j < b.len() && b[j] != b'"' {
        if b[j] == b'\\' {
            s.push(match *b.get(j + 1)? {
                b'n' => '\n',
                b't' => '\t',
                b'r' => '\r',
                other => other as char,
            });
            j += 2;
        } else {
            let start = j;
            while j < b.len() && b[j] != b'"' && b[j] != b'\\' {
                j += 1;
            }
            s.push_str(std::str::from_utf8(&b[start..j]).ok()?);
        }
    }
    (j < b.len()).then_some((s, j + 1))
}

/// 从 `span` 内抽出所有**字符串字面量**，返回 `(已解转义的内容, 字面量起始下标)`。
fn str_lits_in(b: &[u8], span: (usize, usize)) -> Vec<(String, usize)> {
    let (lo, hi) = span;
    let (mut out, mut i) = (Vec::new(), lo);
    while i < hi {
        if b[i] == b'"' {
            let end = skip_str_lit(b, i);
            if let Some((s, _)) = parse_str_lit(b, i) {
                out.push((s, i));
            }
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// locale key 抽取
// ---------------------------------------------------------------------------

/// `b[at..]` 往前回退（跨空白），`tr*` 三个名字之一的**标识符**起点。
///
/// 命中返回 `(名字, 该名字的起始下标)`，没命中返回 `None`。
/// `at` 传的是**实参的第一个字节**（`tr(` 的 `(` 之前可能有空格）。
fn tr_name_before(b: &[u8], lo: usize, at: usize) -> Option<(&'static str, usize)> {
    let mut k = at;
    while k > lo && b[k - 1].is_ascii_whitespace() {
        k -= 1;
    }
    if k == lo || b[k - 1] != b'(' {
        return None;
    }
    let mut j = k - 1;
    while j > lo && is_ident_byte(b[j - 1]) {
        j -= 1;
    }
    if j == k - 1 {
        return None;
    }
    match src_slice(b, j, k - 1) {
        "tr" => Some(("tr", j)),
        "trn" => Some(("trn", j)),
        "tr_fill" => Some(("tr_fill", j)),
        _ => None,
    }
}

/// 从 `span` 内抽出所有 `tr("K")` / `trn("K", …)` / `tr_fill("K", …)` 的**字面量**
/// key；并把 `tr(<recv>.key())` 这种**运行期** key 展开成该枚举的全部取值。
///
/// 三个字面量形态都要覆盖——`on_hover_text` 与 `ui.weak` 现场三种都用过。
/// 返回 `(key, 该 key 在源码里的定位下标)`；定位下标只用于报错指行。
fn keys_in(
    b: &[u8],
    span: (usize, usize),
    enums: &BTreeMap<String, Vec<String>>,
) -> Vec<(String, usize)> {
    let (lo, _) = span;
    let mut out = Vec::new();
    for (s, i) in str_lits_in(b, span) {
        if tr_name_before(b, lo, i).is_some() {
            out.push((s, i));
        }
    }
    for (recv, at) in key_call_receivers(b, span) {
        if tr_name_before(b, lo, at).is_none() {
            continue;
        }
        for k in resolve_enum_keys(b, at, &recv, enums) {
            out.push((k, at));
        }
    }
    out
}

/// 区间里每个 `<recv>.key()` 的 `(recv 文本, receiver 的起始下标)`。
///
/// 认的形状是 `key()` 前面挂着一个**方法调用链**（`f.key()` /
/// `s.draft.flow_sensitivity.key()`）；链上只允许标识符、`.`、`::`。
fn key_call_receivers(b: &[u8], span: (usize, usize)) -> Vec<(String, usize)> {
    let (lo, _) = span;
    let mut out = Vec::new();
    for at in find_all(b, span, b"key()") {
        // 回退收集 receiver
        let mut s = at;
        while s > lo {
            let c = b[s - 1];
            if is_ident_byte(c) || c == b'.' || c == b':' {
                s -= 1;
            } else {
                break;
            }
        }
        // 去掉末尾的 `.`
        let mut e = at;
        while e > s && b[e - 1] == b'.' {
            e -= 1;
        }
        if e == s {
            continue;
        }
        out.push((src_slice(b, s, e).to_owned(), s));
    }
    out
}

/// 枚举名归一化：`FlowSensitivity` / `flow_sensitivity` / `flow sensitivity`
/// 都变成同一个串。
fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// `src/config.rs` / `src/core.rs` / `src/tr.rs` 里每个
/// `pub fn key(self) -> &'static str` 的 match 分支返回值，按枚举名归组。
///
/// 这些就是 `tr(<枚举>.key())` 在运行期可能产出的**全部** locale key。
fn enum_keys() -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for rel in [CONFIG_RS, CORE_RS, TR_RS] {
        let raw = read_src(rel);
        let src = strip_comments_keep_len(&raw);
        let b = src.as_bytes();
        let whole = (0usize, b.len());
        for at in find_all(b, whole, b"fn key(self)") {
            let Some(name) = enclosing_impl_name(b, at) else {
                continue;
            };
            // `fn key(self) -> &'static str {` —— 跳过返回类型再找函数体
            let paren = at + b"fn key".len();
            let after = match_paren(b, paren) + 1;
            let Some(brace) = b[after..]
                .iter()
                .position(|c| *c == b'{')
                .map(|i| i + after)
            else {
                continue;
            };
            let body = (brace + 1, match_brace(b, brace));
            let keys: Vec<String> = str_lits_in(b, body).into_iter().map(|(s, _)| s).collect();
            assert!(
                !keys.is_empty(),
                "{rel} 的 {name}::key() 一个字符串字面量都没返回——解析器跟不上写法变化"
            );
            out.insert(name, keys);
        }
    }
    assert!(
        out.len() >= 5,
        "只从领域层解析到 {} 个 `fn key(self)`（预期 5：FlowSensitivity / SoundPreset / \
         WallpaperFit / ContextState / Lang）。解析器退化了，T5 的运行期 key 展开会静默失效。",
        out.len()
    );
    out
}

/// `at` 之前、**块体仍包住 `at`** 的最近那个 `impl <名字> {`。
fn enclosing_impl_name(b: &[u8], at: usize) -> Option<String> {
    let mut from = 0usize;
    loop {
        let hits = find_all(b, (from, at), b"impl ");
        let i = hits.last().copied()?;
        let brace = b[i..at].iter().position(|c| *c == b'{')? + i;
        if at > brace && at < match_brace(b, brace) {
            let mut e = brace;
            while e > i && (b[e - 1] == b' ' || b[e - 1] == b':') {
                e -= 1;
            }
            let mut s = e;
            while s > i && is_ident_byte(b[s - 1]) {
                s -= 1;
            }
            return (s < e).then(|| src_slice(b, s, e).to_owned());
        }
        from = i + 1;
    }
}

/// 判定 `<recv>.key()` 里的 `recv` 是**哪个**枚举，并把该枚举的全部取值返回。
///
/// 判据一（权威）：`at` 之前最近的那个 `for <recv 尾段> in <路径>::ALL` 绑定。
/// 判据二（保守）：尾段归一化后与枚举名精确相等，或以枚举名结尾。
/// 都不中 → 全部枚举取值的并集（保守方向：多测不少测）。
fn resolve_enum_keys(
    b: &[u8],
    at: usize,
    recv: &str,
    enums: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    let tail = recv.rsplit(['.', ':']).next().unwrap_or(recv).trim();
    let all: Vec<String> = enums.values().flatten().cloned().collect();

    // 判据一
    if let Some(bound) = nearest_for_binding(b, at) {
        if bound.var == tail {
            if let Some(v) = enums.get(&bound.enum_name) {
                return v.clone();
            }
        }
    }
    // 判据二
    let n = normalize(tail);
    if !n.is_empty() {
        let exact: Vec<&String> = enums.keys().filter(|k| normalize(k) == n).collect();
        if exact.len() == 1 {
            return enums[exact[0]].clone();
        }
        let suffix: Vec<&String> = enums
            .keys()
            .filter(|k| n.ends_with(&normalize(k)))
            .collect();
        if suffix.len() == 1 {
            return enums[suffix[0]].clone();
        }
        if !exact.is_empty() || !suffix.is_empty() {
            return exact
                .into_iter()
                .chain(suffix)
                .flat_map(|k| enums[k].clone())
                .collect();
        }
    }
    all
}

struct ForBinding {
    var: String,
    enum_name: String,
}

/// `at` 之前最近的那个 `for <var> in <路径>::…` 的 `(var, 类型名)`。
fn nearest_for_binding(b: &[u8], at: usize) -> Option<ForBinding> {
    let hits = find_all(b, (0, at), b"for ");
    let i = *hits.last()?;
    let s = i + b"for ".len();
    let mut e = s;
    while e < at && is_ident_byte(b[e]) {
        e += 1;
    }
    if e == s {
        return None;
    }
    let var = src_slice(b, s, e).to_owned();
    // `in` 之后到 `::` / `;` 之间的类型路径
    let mut j = e;
    while j + 2 <= at
        && !(b[j] == b'i'
            && b[j + 1] == b'n'
            && !is_ident_byte(b[j + 2])
            && (j == 0 || !is_ident_byte(b[j - 1])))
    {
        j += 1;
    }
    if j + 2 > at {
        return None;
    }
    let mut path = j + 2;
    while path < at && b[path] == b' ' {
        path += 1;
    }
    let mut pe = path;
    while pe < at && (is_ident_byte(b[pe]) || b[pe] == b':') {
        pe += 1;
    }
    let full = src_slice(b, path, pe);
    let enum_name = full.rsplit("::").next().unwrap_or(full).to_owned();
    if enum_name.is_empty() {
        return None;
    }
    Some(ForBinding { var, enum_name })
}

// ---------------------------------------------------------------------------
// 内联文案出现点（T1：附带「是否落在 Extend 布局内」）
// ---------------------------------------------------------------------------

/// 一处内联文案：哪个控件、第几行、哪个 locale key。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Checkbox,
    Label,
    Weak,
    ColoredLabel,
    Button,
    SmallButton,
    SelectableValue,
    HyperlinkTo,
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.fn_name())
    }
}

impl Kind {
    fn fn_name(self) -> &'static str {
        match self {
            Kind::Checkbox => "ui.checkbox",
            Kind::Label => "ui.label",
            Kind::Weak => "ui.weak",
            Kind::ColoredLabel => "ui.colored_label",
            Kind::Button => "ui.button",
            Kind::SmallButton => "ui.small_button",
            Kind::SelectableValue => "ui.selectable_value",
            Kind::HyperlinkTo => "ui.hyperlink_to",
        }
    }
    /// 去掉 `ui.` 前缀的方法名（`widget_call` / `find_widget_calls` 要的是它）。
    fn bare(self) -> &'static str {
        &self.fn_name()["ui.".len()..]
    }
    fn all() -> [Kind; 8] {
        [
            Kind::Checkbox,
            Kind::Label,
            Kind::Weak,
            Kind::ColoredLabel,
            Kind::Button,
            Kind::SmallButton,
            Kind::SelectableValue,
            Kind::HyperlinkTo,
        ]
    }
}

/// 一处内联文案 + 它所处的**布局上下文**。
#[derive(Debug, Clone)]
struct Site {
    kind: Kind,
    key: String,
    line: usize,
    /// 是否落在不换行（Extend）布局内——只有这种才真会横向裁切。
    extend: bool,
}

/// 抽出 `src/ui.rs` 里所有**内联**文案出现点。
///
/// 「内联」= 出现在 `ui.checkbox(` / `ui.label(` / `ui.weak(` / `ui.colored_label(` /
/// `ui.button(` / `ui.small_button(` / `ui.selectable_value(` / `ui.hyperlink_to(`
/// 的实参区间里，**且**不在任何 `on_hover_text(` 的实参区间内。
fn inline_sites() -> Vec<Site> {
    let raw = read_src(UI_RS);
    let src = strip_comments_keep_len(&raw);
    let b = src.as_bytes();
    let whole = (0usize, b.len());
    let enums = enum_keys();

    // 1) 先收 hover 区间：`on_hover_text(` 的实参
    let mut hover_spans: Vec<(usize, usize)> = Vec::new();
    for at in find_all(b, whole, b"on_hover_text(") {
        let open = at + "on_hover_text".len();
        let close = match_paren(b, open);
        hover_spans.push((open + 1, close));
    }

    // 2) 布局上下文：不换行（Extend）的水平布局块
    let hspans = horizontal_spans(b);

    // 3) 再收内联区间
    let mut sites = Vec::new();
    for kind in Kind::all() {
        let fname = kind.bare();
        for open in find_widget_calls(b, fname) {
            let close = match_paren(b, open);
            for (key, at_key) in keys_in(b, (open + 1, close), &enums) {
                if hover_spans.iter().any(|s| at_key > s.0 && at_key < s.1) {
                    continue; // hover 里的排除掉
                }
                sites.push(Site {
                    kind,
                    key,
                    line: line_of(&src, at_key),
                    extend: in_any(&hspans, at_key),
                });
            }
        }
    }
    sites.sort_by_key(|s| (s.line, s.key.clone()));
    sites
}

/// 所有**不换行**（`Extend`）水平布局块的 `(左括号下标, 右括号下标)`。
///
/// - `ui.horizontal(` / `ui.horizontal_top(` / `ui.horizontal_centered(` → **Extend**
///   （`egui-0.36.1/src/ui.rs:2314/2334/2319`：三者都只设 `cross_align`，
///   `main_wrap` 保持 `false` ⇒ `Ui::wrap_mode()` 给 `TextWrapMode::Extend`）
/// - `ui.horizontal_wrapped(` → **不算**（`ui.rs:2367` 传 `main_wrap = true`
///   ⇒ `TextWrapMode::Wrap`，由 egui 自己折行）
///
/// 注意 `find_all(b, …, b"ui.horizontal(")` 不会命中 `ui.horizontal_top(` /
/// `ui.horizontal_wrapped(`：`ui.horizontal` 之后紧跟的是 `_top(` / `_wrapped(`，
/// 模式里的 `(` 对不上。
fn horizontal_spans(b: &[u8]) -> Vec<(usize, usize)> {
    let whole = (0usize, b.len());
    let mut out = Vec::new();
    for name in [
        "ui.horizontal(",
        "ui.horizontal_top(",
        "ui.horizontal_centered(",
    ] {
        for at in find_all(b, whole, name.as_bytes()) {
            let open = at + name.len() - 1;
            out.push((open, match_paren(b, open)));
        }
    }
    out.sort_unstable();
    out
}

// ---------------------------------------------------------------------------
// locale 取值（直接读 TOML，不经过 tr() 的进程级 AtomicU8）
// ---------------------------------------------------------------------------

type Table = BTreeMap<String, String>;

fn load_locale(code: &str) -> Table {
    let p = Path::new(ROOT).join("locales").join(format!("{code}.toml"));
    let raw = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {}: {e}", p.display()));
    let table: toml::Table = raw
        .parse()
        .unwrap_or_else(|e| panic!("{} 不是合法 TOML: {e}", p.display()));
    table
        .into_iter()
        .map(|(k, v)| {
            let s = v
                .as_str()
                .unwrap_or_else(|| panic!("{} 的 {k} 不是字符串", p.display()));
            (k, s.to_owned())
        })
        .collect()
}

/// 渲染一条 locale 取值：`trn` 的复数形态取**较长**的那一档（最保守），`{n}`
/// 代入 `"12"`。`tr_fill` / 其余形态原样量模板（见报告「已知盲区」）。
fn render(value: &str) -> String {
    let forms: Vec<&str> = value.split('|').collect();
    let pick = *forms.iter().max_by_key(|f| f.len()).unwrap_or(&value);
    pick.replace("{n}", "12")
}

/// 从表里取一条 key 并渲染；取不到返回 `None`（缺 key 由调用方单独报）。
fn txt(table: &Table, key: &str) -> Option<String> {
    table.get(key).map(|v| render(v))
}

// ---------------------------------------------------------------------------
// 从 src/app.rs 解析 tune_style 间距 + 字体链
// ---------------------------------------------------------------------------

/// `tune_style` 的一份快照，**从 `src/app.rs` 现场解析**（不是抄的常量）。
///
/// 解析 `fn tune_style(style: &mut egui::Style) { … }` 的函数体，取出
/// `style.spacing.<name> = <数字>;` 的每一项。
fn tune_spacing() -> Vec<(String, f32)> {
    let raw = read_src(APP_RS);
    let src = strip_comments_keep_len(&raw);
    let b = src.as_bytes();
    let at = src
        .find("fn tune_style")
        .unwrap_or_else(|| panic!("{APP_RS} 里找不到 `fn tune_style`"));
    // `fn tune_style(style: &mut egui::Style) {` —— 第一个 `(` 是**参数表**，
    // 真正要扫的是它后面的 `{`（函数体）。参数表里也可能有 `(`。
    let paren = b[at..]
        .iter()
        .position(|c| *c == b'(')
        .expect("tune_style 后必有 (")
        + at;
    let after_params = match_paren(b, paren) + 1;
    let brace = b[after_params..]
        .iter()
        .position(|c| *c == b'{')
        .map(|i| i + after_params)
        .unwrap_or_else(|| panic!("tune_style 的参数表后找不到 `{{`"));
    let body = (brace + 1, match_brace(b, brace));

    let mut out = Vec::new();
    for hit in find_all(b, body, b"style.spacing.") {
        let s = hit + "style.spacing.".len();
        let mut e = s;
        while e < body.1 && is_ident_byte(b[e]) {
            e += 1;
        }
        let name = src_slice(b, s, e).to_owned();
        // 跳过 `.`（`style.spacing.item_spacing.x = …`）与 `=` 之间的空白
        let mut j = e;
        while j < body.1 && (b[j] == b'.' || b[j].is_ascii_whitespace()) {
            j += 1;
        }
        if j < body.1 && b[j] == b'.' {
            j += 1;
            while j < body.1 && b[j].is_ascii_whitespace() {
                j += 1;
            }
        }
        assert_eq!(
            b.get(j),
            Some(&b'='),
            "tune_style 的 spacing 赋值形状变了: {name}"
        );
        // 两种形态：`= <浮点>;` 与 `= egui::vec2(<浮点>, <浮点>);`
        // （`button_padding` / `item_spacing` 是 vec2，`slider_width` 是标量）
        let rest = src_slice(b, j + 1, body.1);
        let v = match_f32(rest)
            .map(|(x, _)| x)
            .or_else(|| {
                let l = rest.find("vec2(")? + "vec2(".len();
                match_f32(&rest[l..]).map(|(x, _)| x)
            })
            .unwrap_or_else(|| {
                panic!("tune_style 的 {name} 赋值既不是 f32 也不是 egui::vec2(f32, f32): {rest:?}")
            });
        out.push((name, v));
    }
    assert!(!out.is_empty(), "tune_style 里一条 spacing 赋值都没解析到");
    out
}

/// 从 `s` 开头解析一个 f32 字面量，返回 `(值, 消耗掉的字节数)`。
fn match_f32(s: &str) -> Option<(f32, usize)> {
    let t = s.trim_start();
    let skipped = s.len() - t.len();
    let end = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e'))
        .unwrap_or(t.len());
    if end == 0 {
        return None;
    }
    t[..end].parse::<f32>().ok().map(|v| (v, skipped + end))
}

/// `ZH_FONTS` / `EN_FONTS` 字体链，同样从 `src/app.rs` 现场解析。
///
/// `font("key", "C:\\Windows\\Fonts\\msyh.ttc")` 逐项取第二个参数并解转义。
fn font_chain(which: &str) -> Vec<String> {
    let raw = read_src(APP_RS);
    let src = strip_comments_keep_len(&raw);
    let b = src.as_bytes();
    let marker = format!("const {which}: &[FontFile] = &[");
    let at = src
        .find(&marker)
        .unwrap_or_else(|| panic!("{APP_RS} 里找不到 `const {which}`"));
    let open = at + marker.len() - 1; // 指向 `[`
    assert_eq!(b[open], b'[', "const {which} 的数组字面量应以 `[` 开头");
    let body = (open + 1, match_bracket(b, b'[', b']', open));

    let mut out = Vec::new();
    for hit in find_all(b, body, b"font(") {
        let f_open = hit + "font".len();
        let f_close = match_paren(b, f_open);
        let lits: Vec<String> = str_lits_in(b, (f_open + 1, f_close))
            .into_iter()
            .filter(|(k, _)| k.contains(".tt") || k.contains(".ttc"))
            .map(|(k, _)| k)
            .collect();
        if let Some(p) = lits.first() {
            out.push(p.clone());
        }
    }
    assert!(!out.is_empty(), "{APP_RS} 的 {which} 里没解析到字体");
    out
}

/// 按语言构建真实字体定义（`src/app.rs` 的 `build_font_defs` 逻辑：链首优先，
/// 逐字符回退）。
fn build_fonts(which: &str) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let mut order = Vec::new();
    for (i, path) in font_chain(which).iter().enumerate() {
        let key = format!("eyeflow_{which}_{i}");
        match std::fs::read(path) {
            Ok(bytes) => {
                fonts
                    .font_data
                    .insert(key.clone(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
                order.push(key);
            }
            Err(e) => panic!(
                "读不到界面字体 {path}（{APP_RS} 的 {which} 指定的那条链）: {e}。这条测试量的是真实字形宽度，缺字体就必须失败，不能退回 egui 内置字体凑一个绿。"
            ),
        }
    }
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let mut list = order.clone();
        let entry = fonts.families.entry(family).or_default();
        list.append(entry);
        *entry = list;
    }
    fonts
}

/// `tune_style` 的间距施加到一个 `Style` 上。
fn apply_tune_style(style: &mut egui::Style) {
    for (name, v) in tune_spacing() {
        match name.as_str() {
            "button_padding" => style.spacing.button_padding = egui::vec2(v, v),
            "item_spacing" => style.spacing.item_spacing = egui::vec2(v, v),
            "slider_width" => style.spacing.slider_width = v,
            other => panic!(
                "tune_style 里出现了本测试不认识的 spacing 字段 {other:?}——\
                 请在 tests/layout_width.rs 的 apply_tune_style 里补上，否则这次测量用的\
                 间距已经不是产品真实间距了"
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// 行模型：从 src/ui.rs 现场解析每一个 ui.horizontal 族块
// ---------------------------------------------------------------------------

/// 一个滑条的几何：从 `egui::Slider::new(&mut x, A..=B).suffix(tr("K")).text(tr("J"))`
/// 现场解析。
#[derive(Debug, Clone)]
struct SliderSpec {
    lo: f32,
    hi: f32,
    /// 起始显示值——它决定滑条右侧 `DragValue` 的文本宽度。
    start: f32,
    /// 两端都是整数（`u32` 滑条）⇒ 数值按整数显示。
    int: bool,
    /// `.step_by(..)`
    step: Option<f64>,
    /// `.suffix(tr("K"))`
    unit: Option<String>,
    /// `.text(tr("K"))`（滑条上方那个说明标签）
    text: Option<String>,
    /// 链上有 `.custom_formatter(..)` ⇒ 用 `hours_label` 那种去尾零形态。
    custom_fmt: bool,
}

/// 行里的一个条目，**全部从源码解析出来**（不手抄控件清单）。
#[derive(Debug, Clone)]
enum Item {
    /// `ui.label(tr(..))`
    Label(String),
    /// `ui.weak(tr(..))`
    Weak(String),
    Colored(String),
    Checkbox(String),
    Button(String),
    SmallButton(String),
    Link(String),
    /// `ui.label(":")`
    Colon,
    /// `ui.label(format!(..))`——状态条那行，内容是运行期字符串。
    StatusLine {
        keys: Vec<String>,
    },
    /// `egui::Label::new(RichText::from(&s.config_path_display).weak()).truncate().ui(ui)`
    ConfigPathTrunc,
    Slider(SliderSpec),
    /// `time_editor(ui, ..)`：`h : m` 两个 `DragValue` + 一个 `:`。
    TimeEditor,
    /// `ui.add_space(<常量>)`
    SpaceConst(f32),
    /// `ui.add_space(label_column_width(ui, tr("K")) + ui.spacing().item_spacing.x)`
    SpaceLabelCol {
        key: String,
        plus_item_spacing: bool,
    },
    /// `egui::ComboBox::from_id_salt(..).selected_text(tr(<枚举>.key()))`
    Combo {
        options: Vec<String>,
    },
    /// `wallpaper::preview(ui, ..)`
    Preview,
    /// `match … { A => {…} B => {…} }` 的**各分支**——同一时刻只有一个分支渲染，
    /// 所以它们是**互斥的备选**，不是并列条目。行宽取各分支里的最大那个。
    Alternatives(Vec<Vec<Item>>),
}

impl Item {
    /// 本条目在测量时用到的 locale key（同步核对用）。
    fn keys(&self) -> Vec<&str> {
        match self {
            Item::Label(k)
            | Item::Weak(k)
            | Item::Colored(k)
            | Item::Checkbox(k)
            | Item::Button(k)
            | Item::SmallButton(k)
            | Item::Link(k) => vec![k.as_str()],
            Item::StatusLine { keys } => keys.iter().map(String::as_str).collect(),
            Item::Slider(s) => s
                .unit
                .iter()
                .chain(s.text.iter())
                .map(String::as_str)
                .collect(),
            Item::SpaceLabelCol { key, .. } => vec![key.as_str()],
            Item::Combo { options } => options.iter().map(String::as_str).collect(),
            Item::Colon
            | Item::ConfigPathTrunc
            | Item::TimeEditor
            | Item::SpaceConst(_)
            | Item::Preview => Vec::new(),
            Item::Alternatives(arms) => arms
                .iter()
                .flat_map(|a| a.iter().flat_map(Item::keys))
                .collect(),
        }
    }

    /// 报错时给人看的名字。
    fn tag(&self) -> String {
        match self {
            Item::Label(k) | Item::Weak(k) | Item::Colored(k) | Item::Checkbox(k) => {
                format!("{k:?}")
            }
            Item::Button(k) | Item::SmallButton(k) | Item::Link(k) => format!("{k:?}"),
            Item::Colon => "\":\"".to_owned(),
            Item::StatusLine { .. } => "状态条那行".to_owned(),
            Item::ConfigPathTrunc => "配置路径（truncate）".to_owned(),
            Item::Slider(s) => format!(
                "滑条 {}..={}{}{}",
                s.lo,
                s.hi,
                s.unit
                    .as_deref()
                    .map(|u| format!(" unit={u}"))
                    .unwrap_or_default(),
                s.text
                    .as_deref()
                    .map(|t| format!(" text={t}"))
                    .unwrap_or_default()
            ),
            Item::TimeEditor => "时间编辑器 hh:mm".to_owned(),
            Item::SpaceConst(v) => format!("add_space({v})"),
            Item::SpaceLabelCol { key, .. } => format!("label_column_width({key:?})"),
            Item::Combo { options } => format!("下拉框 [{}]", options.join(", ")),
            Item::Preview => "壁纸预览图".to_owned(),
            Item::Alternatives(arms) => {
                let inner: Vec<String> = arms
                    .iter()
                    .map(|a| a.iter().map(Item::tag).collect::<Vec<_>>().join(" + "))
                    .collect();
                format!("match 的互斥分支 {}", inner.join(" | "))
            }
        }
    }
}

/// 把一行展开成若干**变体**：`Item::Alternatives` 的每个分支各出一个变体。
///
/// 一行里其余条目是并列的，只有 `match` 的各分支互斥——变体数 = 各 Alternatives
/// 的分支数之积（本项目实际是 1~2 个）。
fn row_variants(items: &[Item]) -> Vec<Vec<Item>> {
    let mut out: Vec<Vec<Item>> = vec![Vec::new()];
    for it in items {
        match it {
            Item::Alternatives(arms) => {
                let mut next = Vec::new();
                for base in &out {
                    for arm in arms {
                        let mut v = base.clone();
                        v.extend(arm.iter().cloned());
                        next.push(v);
                    }
                }
                out = next;
            }
            other => out.iter_mut().for_each(|v| v.push(other.clone())),
        }
    }
    out
}

/// 一个 `ui.horizontal` 族块 = 一行。
#[derive(Debug, Clone)]
struct Row {
    line: usize,
    items: Vec<Item>,
}

/// `ui.rs` 里的 `INDENT` 常量（现场解析，不抄）。
fn ui_indent() -> f32 {
    let raw = read_src(UI_RS);
    let src = strip_comments_keep_len(&raw);
    let at = src
        .find("const INDENT: f32")
        .unwrap_or_else(|| panic!("{UI_RS} 里找不到 `const INDENT: f32`"));
    let rest = &src[at..];
    let eq = rest.find('=').expect("INDENT 必有 =") + 1;
    match_f32(&rest[eq..])
        .map(|(v, _)| v)
        .expect("INDENT 的初值不是 f32")
}

/// 最坏情况的配置路径 fixture：20 个字符的用户目录名。
///
/// **这是假设，不是实测**：本机真实路径短得多。但闸门不该只在「恰好短」的机器
/// 上成立，所以按 Windows 允许的最长情形取一个值。**兜底是 `src/ui.rs` 里那一行
/// 的 `Label::truncate()`**——真超出时它会省略并把全文挂到 hover 上，而这一行
/// 断言的是「这么长的路径 + `Open folder` 按钮仍然排得下」。
const WORST_CONFIG_PATH: &str =
    r"C:\Users\abcdefghijklmnopqrst\AppData\Roaming\eyeflow\config.toml";
/// 壁纸预览图那行的宽度上限（`ui.rs:655` 的 `.clamp(240.0, 480.0)` 的上界）。
const PREVIEW_MAX_W: f32 = 480.0;

/// 弹出层实参区间：`on_hover_text(...)` 与 `ComboBox::show_ui(...)`。
///
/// 它们的实参不在那一行里渲染（一个是 tooltip，一个是下拉弹窗），必须整段跳过，
/// 否则里面的 `tr(..)` 会被当成行内条目。
fn popup_spans(b: &[u8]) -> Vec<(usize, usize)> {
    let whole = (0usize, b.len());
    let mut out = Vec::new();
    for pat in [b"on_hover_text(".as_slice(), b"show_ui(".as_slice()] {
        for at in find_all(b, whole, pat) {
            let open = at + pat.len() - 1;
            out.push((open, match_paren(b, open)));
        }
    }
    out
}

/// 在 `b[i..]` 处是否出现了字面量 `pat`（且 `i` 前面不是标识符字符）。
fn hit_at(b: &[u8], i: usize, hi: usize, pat: &[u8]) -> bool {
    i + pat.len() <= hi && &b[i..i + pat.len()] == pat && (i == 0 || !is_ident_byte(b[i - 1]))
}

/// `ui.<fname>(` **或**行首链式 `.<fname>(`（`src/ui.rs:797-799` 那种
/// `ui\n    .checkbox(…)` 写法）的**左括号下标**。
///
/// 两种形态都要认：只认 `ui.<fn>(` 会漏掉整条 builder 链——本文件自己的
/// `ui.hotkey` 就是这样丢的（2026-09-26 实测）。
fn widget_call(b: &[u8], i: usize, hi: usize, fname: &str) -> Option<usize> {
    let p1 = format!("ui.{fname}(");
    if hit_at(b, i, hi, p1.as_bytes()) {
        return Some(i + p1.len() - 1);
    }
    let p2 = format!(".{fname}(");
    if hit_at(b, i, hi, p2.as_bytes()) {
        let mut j = i;
        while j > 0 && (b[j - 1] == b' ' || b[j - 1] == b'\t') {
            j -= 1;
        }
        if j == 0 || b[j - 1] == b'\n' {
            return Some(i + p2.len() - 1);
        }
    }
    None
}

/// `fname` 在整个文件里的全部调用点（两种形态），返回左括号下标。
fn find_widget_calls(b: &[u8], fname: &str) -> Vec<usize> {
    let hi = b.len();
    let p1 = format!("ui.{fname}(");
    let p2 = format!(".{fname}(");
    let mut out: Vec<usize> = find_all(b, (0, hi), p1.as_bytes())
        .into_iter()
        .map(|at| at + p1.len() - 1)
        .collect();
    for at in find_all(b, (0, hi), p2.as_bytes()) {
        if let Some(open) = widget_call(b, at, hi, fname) {
            out.push(open);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// 解析出 `src/ui.rs` 里每一个 `ui.horizontal` 族块的内容。
fn rows() -> Vec<Row> {
    let raw = read_src(UI_RS);
    let src = strip_comments_keep_len(&raw);
    let b = src.as_bytes();
    let skips = popup_spans(b);
    let enums = enum_keys();
    let indent = ui_indent();

    let mut out = Vec::new();
    for (open, close) in horizontal_spans(b) {
        let body = (open + 1, close);
        let line = line_of(&src, open);
        let items = row_items(b, &src, body, &skips, &enums, indent, line);
        assert!(
            !items.is_empty(),
            "{UI_RS}:{line} 的 ui.horizontal 块一个条目都没解析出来——解析器退化了，\
             这一行等于没被测量"
        );
        out.push(Row { line, items });
    }
    assert!(
        out.len() >= 20,
        "只从 {UI_RS} 解析到 {} 个 ui.horizontal 族块（预期 ≥20）",
        out.len()
    );
    out
}

/// 跳到 `i` 之后第一个非空白字节的下标。
fn skip_ws(b: &[u8], mut i: usize, hi: usize) -> usize {
    while i < hi && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// 从 `at`（`(` 的下标）起吃掉「方法链」：`.name` / `.name(..)` 反复，直到下一个
/// 非空白字符不是 `.`。返回吃掉之后的字节下标。
fn eat_chain(b: &[u8], mut at: usize, hi: usize) -> usize {
    loop {
        let j = skip_ws(b, at, hi);
        if j >= hi || b[j] != b'.' {
            return at;
        }
        let mut e = j + 1;
        while e < hi && is_ident_byte(b[e]) {
            e += 1;
        }
        if e == j + 1 {
            return at;
        }
        at = e;
        let k = skip_ws(b, at, hi);
        if k < hi && b[k] == b'(' {
            at = match_paren(b, k) + 1;
        }
    }
}

/// 实参区间里的**唯一**那个 `tr*` 字面量 key（`ui.label(tr("K"))` 这类）。
fn one_key(
    b: &[u8],
    args: (usize, usize),
    enums: &BTreeMap<String, Vec<String>>,
) -> Option<String> {
    let mut ks = keys_in(b, args, enums);
    if ks.is_empty() {
        return None;
    }
    // 同一条实参里 `tr` 与 `tr_fill` 不会同时出现；真出现了取第一个并由调用方核对
    Some(ks.swap_remove(0).0)
}

/// 解析一个 `ui.horizontal` 块的条目（纯扫描，不做同步核对）。
#[allow(clippy::too_many_arguments)]
fn scan_items(
    b: &[u8],
    body: (usize, usize),
    skips: &[(usize, usize)],
    enums: &BTreeMap<String, Vec<String>>,
    indent: f32,
    line: usize,
) -> Vec<Item> {
    let (lo, hi) = body;
    let mut items: Vec<(usize, Item)> = Vec::new();
    let mut i = lo;
    while i < hi {
        if in_any(skips, i) {
            i += 1;
            continue;
        }
        // 快筛：下面所有标记的**首字节**只可能是这几个（`ui.` / 行首链式 `.` /
        // `egui::` / `time_editor(` / `wallpaper::preview(` / `match`）。不筛的话
        // 每个字节位都要 `format!` 七八次。
        if !matches!(b[i], b'u' | b'.' | b'e' | b't' | b'w' | b'm') {
            i += 1;
            continue;
        }
        // `name` 是**不含左括号**的完整调用前缀；左括号紧跟其后。
        let arg = |name: &str, at: usize| -> (usize, usize) {
            let open = at + name.len();
            (open + 1, match_paren(b, open))
        };
        macro_rules! keyed_at {
            ($a:expr, $c:expr, $mk:expr) => {
                match one_key(b, ($a, $c), enums) {
                    Some(k) => items.push((i, $mk(k))),
                    None => panic!(
                        "{UI_RS}:{line} 的实参里没有 tr* key：{:?}。\
                         行模型不认识这一行了，先补 parser。",
                        src_slice(b, $a, $c)
                    ),
                }
            };
        }
        /// 「`ui.<fname>(` / 行首 `.<fname>(` + 一个 tr* key」→ 一个条目。
        macro_rules! keyed {
            ($fname:literal, $mk:expr) => {
                if let Some(open) = widget_call(b, i, hi, $fname) {
                    let (a, c) = (open + 1, match_paren(b, open));
                    keyed_at!(a, c, $mk);
                    i = eat_chain(b, c + 1, hi).max(c + 1);
                    continue;
                }
            };
        }
        if let Some(open) = widget_call(b, i, hi, "label") {
            let (a, c) = (open + 1, match_paren(b, open));
            let text = src_slice(b, a, c).trim();
            if text == "\":\"" {
                items.push((i, Item::Colon));
            } else if text.starts_with("format!") {
                items.push((i, Item::StatusLine { keys: Vec::new() }));
            } else if let Some(k) = one_key(b, (a, c), enums) {
                items.push((i, Item::Label(k)));
            } else {
                panic!(
                    "{UI_RS}:{line} 的 `ui.label` 实参 {text:?} 既不是 tr* key 也不是 \
                     `\":\"` / `format!`——行模型不认识这一行了，先补 parser。"
                );
            }
            i = c + 1;
            continue;
        }
        keyed!("weak", Item::Weak);
        keyed!("colored_label", Item::Colored);
        keyed!("checkbox", Item::Checkbox);
        keyed!("small_button", Item::SmallButton);
        keyed!("button", Item::Button);
        keyed!("hyperlink_to", Item::Link);
        if hit_at(b, i, hi, b"ui.add_space(") {
            let (a, c) = arg("ui.add_space", i);
            let expr = src_slice(b, a, c);
            let items_at = i;
            i = c + 1;
            let trimmed = expr.trim();
            let item = if let Some(v) = match_f32(trimmed).map(|(v, _)| v) {
                Item::SpaceConst(v)
            } else if trimmed == "INDENT" {
                Item::SpaceConst(indent)
            } else if let Some(k) = one_key(b, (a, c), enums) {
                Item::SpaceLabelCol {
                    key: k,
                    plus_item_spacing: trimmed.contains("item_spacing"),
                }
            } else {
                panic!(
                    "{UI_RS}:{line} 的 `ui.add_space` 实参 {expr:?} 不认识——行模型要先补 parser。"
                )
            };
            items.push((items_at, item));
        } else if hit_at(b, i, hi, b"ui.add(") {
            let (a, c) = arg("ui.add", i);
            let inner = find_all(b, (a, c), b"egui::Slider::new(");
            let Some(hit) = inner.first().copied() else {
                panic!(
                    "{UI_RS}:{line} 的 `ui.add` 里没有 Slider：{:?}——行模型不认识这一行了。",
                    src_slice(b, a, c)
                );
            };
            let (spec, _) = parse_slider(b, hit, c);
            items.push((i, Item::Slider(spec)));
            i = c + 1;
        } else if hit_at(b, i, hi, b"egui::Slider::new(") {
            let (spec, end) = parse_slider(b, i, hi);
            items.push((i, Item::Slider(spec)));
            i = end;
        } else if hit_at(b, i, hi, b"egui::Button::new(") {
            let (a, c) = arg("egui::Button::new", i);
            keyed_at!(a, c, Item::Button);
            i = eat_chain(b, c + 1, hi).max(c + 1);
        } else if hit_at(b, i, hi, b"egui::Label::new(") {
            let (_a, c) = arg("egui::Label::new", i);
            let end = eat_chain(b, c + 1, hi);
            let chain = src_slice(b, c, end);
            assert!(
                chain.contains(".truncate()"),
                "{UI_RS}:{line} 的 `egui::Label::new` 链上没有 `.truncate()`：{chain:?}。\
                 配置路径是**无上界的运行期字符串**，没有 truncate 就一定会横向越界。"
            );
            items.push((i, Item::ConfigPathTrunc));
            i = end;
        } else if hit_at(b, i, hi, b"egui::ComboBox::from_id_salt(") {
            let (_a, c) = arg("egui::ComboBox::from_id_salt", i);
            let end = eat_chain(b, c + 1, hi);
            let chain = (c, end);
            let mut options: Vec<String> = Vec::new();
            // ⚠️ **只看 `.selected_text(..)` 的实参**：再往后的 `show_ui(..)` 闭包里
            // 还有 `tr(p.key())`（下拉弹窗的候选项），它们渲染在弹窗里、**不在这一行**。
            // 扫整条链会把四个下拉框全部喂成「所有枚举的全集」，行宽虚高一大截。
            let sel = find_all(b, chain, b".selected_text(");
            let Some(h) = sel.first().copied() else {
                panic!(
                    "{UI_RS}:{line} 的 ComboBox 链里没有 `.selected_text(`：{:?}",
                    src_slice(b, chain.0, chain.1)
                );
            };
            let s_open = h + ".selected_text".len();
            let s_args = (s_open + 1, match_paren(b, s_open));
            for (recv, at) in key_call_receivers(b, s_args) {
                if tr_name_before(b, s_args.0, at).is_none() {
                    continue;
                }
                options.extend(resolve_enum_keys(b, at, &recv, enums));
            }
            options.sort();
            options.dedup();
            assert!(
                !options.is_empty(),
                "{UI_RS}:{line} 的 ComboBox 链里没解析出 `selected_text(tr(<枚举>.key()))`：{:?}",
                src_slice(b, s_args.0, s_args.1)
            );
            items.push((i, Item::Combo { options }));
            i = end;
        } else if hit_at(b, i, hi, b"time_editor(") {
            let (_a, c) = arg("time_editor", i);
            items.push((i, Item::TimeEditor));
            i = c + 1;
        } else if hit_at(b, i, hi, b"wallpaper::preview(") {
            let (_a, c) = arg("wallpaper::preview", i);
            items.push((i, Item::Preview));
            i = c + 1;
        } else if hit_at(b, i, hi, b"match")
            && !b[i..].starts_with(b"matches!")
            && (i == 0 || !is_ident_byte(b[i - 1]))
        {
            // `match … { A => {…} B => {…} }`：各分支**互斥**，行宽取最宽的那个分支。
            // 不这么建模就会把「更新状态」那一行的 5 个状态文案当成并列条目，
            // 量出一个 720 pt 的假越界。
            let brace = b[i..hi]
                .iter()
                .position(|c| *c == b'{')
                .map(|k| k + i)
                .unwrap_or_else(|| panic!("{UI_RS}:{line} 的 match 没找到 `{{`"));
            let mbody = (brace + 1, match_brace(b, brace));
            let arms = match_arms(b, mbody);
            assert!(
                !arms.is_empty(),
                "{UI_RS}:{line} 的 match 一个分支都没解析出来"
            );
            let parsed: Vec<Vec<Item>> = arms
                .iter()
                .map(|a| scan_items(b, *a, skips, enums, indent, line))
                .collect();
            items.push((i, Item::Alternatives(parsed)));
            i = match_brace(b, brace) + 1;
        } else if BENIGN_UI.iter().any(|n| hit_at(b, i, hi, n.as_bytes())) {
            // 容器 / 只读访问器：不消费，继续往里扫（`add_enabled_ui`、`with_layout`
            // 的闭包里还有真控件）
            i += 1;
        } else {
            i += 1;
        }
    }

    let items: Vec<Item> = {
        let mut v = items;
        v.sort_by_key(|(at, _)| *at);
        v.into_iter().map(|(_, it)| it).collect()
    };
    items
}

/// 一个 `match` 块体里**各分支的实参区间**。
///
/// 只认 `Pat => { … }` 这种带花括号的分支（本项目全部是）；不带花括号就 panic
/// ——宁可当场红，也不许把一个分支悄悄漏掉然后量出一个偏小的行宽。
fn match_arms(b: &[u8], mbody: (usize, usize)) -> Vec<(usize, usize)> {
    let arrows: Vec<usize> = find_all(b, mbody, b"=>")
        .into_iter()
        .filter(|a| b.get(a + 2) != Some(&b'=') && b.get(a.wrapping_sub(1)) != Some(&b'-'))
        .collect();
    let mut out = Vec::new();
    for a in arrows {
        let start = skip_ws(b, a + 2, mbody.1);
        assert!(
            b.get(start) == Some(&b'{'),
            "{UI_RS} 的 match 分支不是 `Pat => {{ … }}` 形态（第 {} 行附近）：{:?}——\
             行模型不认识它，先补 parser。",
            line_of_bytes(b, start),
            src_slice(b, a, (a + 40).min(mbody.1))
        );
        out.push((start + 1, match_brace(b, start)));
    }
    out
}

/// 把 `b` 当 UTF-8 源码用（只在报错里出现）。
fn line_of_bytes(b: &[u8], at: usize) -> usize {
    src_slice(b, 0, at.min(b.len()))
        .bytes()
        .filter(|c| *c == b'\n')
        .count()
        + 1
}
/// 解析一个 `ui.horizontal` 块 = 扫描 + 两道同步核对。
#[allow(clippy::too_many_arguments)]
fn row_items(
    b: &[u8],
    src: &str,
    body: (usize, usize),
    skips: &[(usize, usize)],
    enums: &BTreeMap<String, Vec<String>>,
    indent: f32,
    line: usize,
) -> Vec<Item> {
    let items = scan_items(b, body, skips, enums, indent, line);

    // 状态条那行：把它用的 locale key 记上去，好让下面的同步核对能对上
    let mut items = items;
    if let Some(pos) = items
        .iter()
        .position(|it| matches!(it, Item::StatusLine { .. }))
    {
        let keys = status_line_keys(b, body, skips, enums);
        items[pos] = Item::StatusLine { keys };
    }

    // 同步核对①：块里每一个 `tr*` 字面量 key 都必须被行模型认领。
    let claimed: BTreeSet<String> = items
        .iter()
        .flat_map(|it| it.keys())
        .map(str::to_owned)
        .collect();
    let mut orphans = Vec::new();
    for (k, at) in keys_in(b, body, enums) {
        if in_any(skips, at) {
            continue;
        }
        if !claimed.contains(&k) {
            orphans.push(format!("{UI_RS}:{} {k:?}", line_of(src, at)));
        }
    }
    assert!(
        orphans.is_empty(),
        "{UI_RS}:{line} 这一行里这些 locale key 没有被行模型认领——它多半是**新加的**控件，\
         本行的右缘从此没人量了：\n    {}\n  请在 tests/layout_width.rs 的 row_items 里补上对应的 Item。",
        orphans.join("\n    ")
    );

    // 同步核对②：块里每一个 `ui.<方法>(` / `egui::<类型>::` 都得在已知名单里。
    // 这是「不能静默漏测」的最后一道闸：有人往一行里塞了个新控件，这里立刻红。
    // 弹出层（`on_hover_text` / `show_ui`）的实参不在这一行里渲染，跳过。
    let mut unknown = Vec::new();
    for at in find_all(b, body, b"ui.") {
        if in_any(skips, at) {
            continue;
        }
        let mut e = at + 3;
        while e < body.1 && is_ident_byte(b[e]) {
            e += 1;
        }
        let name = src_slice(b, at + 3, e);
        if name.is_empty() {
            continue;
        }
        let after = skip_ws(b, e, body.1);
        if after < body.1 && b[after] == b'(' {
            let known = KNOWN_UI.contains(&name);
            if !known {
                unknown.push(format!("{UI_RS}:{} ui.{name}()", line_of(src, at)));
            }
        }
    }
    for at in find_all(b, body, b"egui::") {
        if in_any(skips, at) {
            continue;
        }
        let mut e = at + 6;
        while e < body.1 && is_ident_byte(b[e]) {
            e += 1;
        }
        let name = src_slice(b, at + 6, e);
        let after = skip_ws(b, e, body.1);
        if after < body.1 && b[after] == b'(' && !KNOWN_EGUI.contains(&name) {
            unknown.push(format!("{UI_RS}:{} egui::{name}::", line_of(src, at)));
        }
    }
    unknown.dedup();
    assert!(
        unknown.is_empty(),
        "{UI_RS} 的 ui.horizontal 行里出现了行模型不认识的控件：\n    {}\n  \
         请在 tests/layout_width.rs 的 row_items 里补上对应的 Item——**这一行的右缘会\
         从此没人量**。",
        unknown.join("\n    ")
    );

    items
}

/// 行模型已知的 `ui.<方法>`：要么能当条目解析，要么是纯容器 / 只读访问器。
const KNOWN_UI: &[&str] = &[
    "label",
    "weak",
    "colored_label",
    "checkbox",
    "button",
    "small_button",
    "hyperlink_to",
    "add_space",
    "add",
    "add_enabled_ui",
    "add_enabled",
    "with_layout",
    "push_id",
    "spacing",
    "spacing_mut",
    "style",
    "style_mut",
    "ctx",
    "interact",
    "close",
    "visuals",
    "painter",
    "input",
    "memory",
    "output",
    "horizontal",
    "set_min_width",
    "allocate_space",
    "min_rect",
    "available_width",
    "cursor",
    "rect",
];

/// 行模型已知的 `egui::<类型>`。
const KNOWN_EGUI: &[&str] = &[
    "Slider",
    "Button",
    "ComboBox",
    "Label",
    "DragValue",
    "RichText",
    "Color32",
    "Layout",
    "Align",
    "Response",
    "Style",
];

/// 容器 / 只读访问器：命中就跳过一个字节继续扫（不消费，让闭包里的真控件被看到）。
const BENIGN_UI: &[&str] = &[
    "ui.add_enabled_ui(",
    "ui.add_enabled(",
    "ui.with_layout(",
    "ui.push_id(",
    "ui.spacing(",
    "ui.spacing_mut(",
    "ui.style(",
    "ui.style_mut(",
    "ui.ctx(",
    "ui.interact(",
    "ui.close(",
    "ui.visuals(",
    "ui.painter(",
    "ui.input(",
    "ui.memory(",
    "ui.output(",
];

/// 状态条那行 `ui.label(format!(..))` 里用到的 key。
fn status_line_keys(
    b: &[u8],
    body: (usize, usize),
    skips: &[(usize, usize)],
    enums: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (k, at) in keys_in(b, body, enums) {
        if !in_any(skips, at) {
            out.push(k);
        }
    }
    out
}

/// 解析一个 `egui::Slider::new(&mut x, A..=B)` 及其方法链，返回 `(spec, 链尾下标)`。
fn parse_slider(b: &[u8], at: usize, hi: usize) -> (SliderSpec, usize) {
    let open = at + "egui::Slider::new".len();
    let close = match_paren(b, open);
    let args = src_slice(b, open + 1, close);
    // `&mut <ident>, <range>`
    let (_, range) = args
        .split_once(',')
        .unwrap_or_else(|| panic!("Slider::new 实参里没有逗号: {args:?}"));
    let range = range.trim().replace("..=", "..");
    let (lo_s, hi_s) = range
        .split_once("..")
        .unwrap_or_else(|| panic!("滑条范围不是 `A..B` 形态: {range:?}"));
    let lo: f32 = lo_s
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("滑条下界 {lo_s:?} 解析失败: {e}"));
    let hh: f32 = hi_s
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("滑条上界 {hi_s:?} 解析失败: {e}"));
    let int = !lo_s.contains('.') && !hi_s.contains('.');

    let end = eat_chain(b, close + 1, hi);
    let chain = src_slice(b, close, end);
    let unit = after_call_lit(b, close, end, ".suffix");
    let text = after_call_lit(b, close, end, ".text");
    let step = if chain.contains(".step_by(") {
        let s = chain.find(".step_by(").unwrap() + ".step_by(".len();
        chain[s..]
            .split(|c: char| !(c.is_ascii_digit() || c == '.'))
            .find(|t| !t.is_empty())
            .and_then(|t| t.parse::<f64>().ok())
    } else {
        None
    };
    (
        SliderSpec {
            lo,
            hi: hh,
            start: (lo + hh) / 2.0,
            int,
            step,
            unit,
            text,
            custom_fmt: chain.contains(".custom_formatter("),
        },
        end,
    )
}

/// 在方法链里找 `.<name>(tr*(..))` 并取出那个字面量。
fn after_call_lit(b: &[u8], close: usize, end: usize, name: &str) -> Option<String> {
    let chain = src_slice(b, close, end);
    let i = chain.find(name)?;
    let open = close + i + name.len();
    if b.get(open) != Some(&b'(') {
        return None;
    }
    let c = match_paren(b, open);
    let lits: Vec<String> = str_lits_in(b, (open + 1, c))
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    lits.first().cloned()
}

// ---------------------------------------------------------------------------
// 测量台
// ---------------------------------------------------------------------------

/// 一次测量：一个 `egui::Context`（真实字体链 + `tune_style` 间距）在
/// 560 × 624 的窗口里复刻 `ui.rs` 的 `card()`。
///
/// `add(ui)` 在**卡片内容区**里干活；`content_w` 是那张卡片的可用宽。
fn measure<F>(which: &str, mut add: F) -> (f32, Vec<f32>)
where
    F: FnMut(&mut egui::Ui) -> Vec<f32>,
{
    let ctx = egui::Context::default();
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_fonts(build_fonts(which));
    ctx.set_pixels_per_point(1.25);

    let mut content_w = 0.0f32;
    let mut widths: Vec<f32> = Vec::new();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let rect =
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::Vec2::new(WINDOW_W, WINDOW_H));
        // `ui.rs:121` 的 `show()` 会把 item_spacing 覆盖成 (8, 6)
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
        apply_tune_style(ui.style_mut());
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            // `ui.rs:146` 的 `card()`：`ui.group` + `set_min_width(available)`
            ui.group(|ui| {
                ui.set_min_width(ui.available_width());
                content_w = ui.available_width();
                widths = add(ui);
            });
        });
    });
    // 纹理没人消费；丢掉以免几十 MB 字体纹理留到进程退出
    out.textures_delta.clear();
    std::mem::forget(out);
    (content_w, widths)
}

/// 状态条那行里「最宽」的一组：最长的 `core.state_*` + 最长的 `runtime.reminder_*`。
///
/// ⚠️ **这是假设不是实测**——真实取值由调度状态决定，测试里拿不到。取最坏
/// 组合是保守方向：真实值只会更短。
fn status_line_text(table: &Table) -> String {
    let longest = |prefix: &str| -> String {
        table
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(_, v)| render(v))
            .max_by_key(|v| v.chars().count())
            .unwrap_or_default()
    };
    let state = longest("core.state_");
    let reminder = longest("runtime.reminder_")
        .replace("{t}", "14:32")
        .replace("{n}", "12");
    let dot = txt(table, "stats.dot").unwrap_or_default();
    let sep = txt(table, "stats.separator").unwrap_or_default();
    format!("{dot} {state}{sep}{reminder}")
}

/// 把 `Item` 真的 add 进 `ui`（它就是 `src/ui.rs` 里那个 `ui.horizontal` 的实参）。
fn lay_item(ui: &mut egui::Ui, item: &Item, table: &Table, widths: &mut Vec<f32>) {
    let need = |key: &str| -> String {
        txt(table, key).unwrap_or_else(|| panic!("locale 表里没有 {key:?}"))
    };
    let mut push = |w: f32, _tag: &str| widths.push(w);
    match item {
        Item::Label(k) => push(ui.label(need(k)).rect.width(), k),
        Item::Weak(k) => push(ui.weak(need(k)).rect.width(), k),
        Item::Colored(k) => push(
            ui.colored_label(ui.visuals().text_color(), need(k))
                .rect
                .width(),
            k,
        ),
        Item::Checkbox(k) => {
            let mut b = false;
            push(ui.checkbox(&mut b, need(k)).rect.width(), k)
        }
        Item::Button(k) => push(ui.button(need(k)).rect.width(), k),
        Item::SmallButton(k) => push(ui.small_button(need(k)).rect.width(), k),
        Item::Link(k) => push(
            ui.hyperlink_to(need(k), "https://example.invalid/")
                .rect
                .width(),
            k,
        ),
        Item::Colon => push(ui.label(":").rect.width(), ":"),
        Item::StatusLine { .. } => push(ui.label(status_line_text(table)).rect.width(), "status"),
        Item::ConfigPathTrunc => {
            let r = egui::Widget::ui(
                egui::Label::new(egui::RichText::from(WORST_CONFIG_PATH).weak()).truncate(),
                ui,
            );
            push(r.rect.width(), "config_path");
        }
        Item::Slider(s) => {
            let mut v = s.start;
            let mut sl = egui::Slider::new(&mut v, s.lo..=s.hi);
            if let Some(u) = &s.unit {
                sl = sl.suffix(need(u));
            }
            if let Some(st) = &s.step {
                sl = sl.step_by(*st);
            }
            if s.int {
                sl = sl.custom_formatter(|v, _| format!("{}", v.round() as i64));
            } else if s.custom_fmt {
                sl = sl.custom_formatter(|v, _| hours_label(v));
            }
            if let Some(t) = &s.text {
                let w = ui.add(sl.text(need(t))).rect.width();
                push(w, t);
            } else {
                let w = ui.add(sl).rect.width();
                push(w, "slider");
            }
        }
        Item::TimeEditor => {
            let (mut h, mut m) = (12u32, 0u32);
            let w = ui
                .add(
                    egui::DragValue::new(&mut h)
                        .range(0..=23)
                        .speed(0.05)
                        .custom_formatter(|v, _| format!("{:02}", v as u32)),
                )
                .rect
                .width()
                + ui.spacing().item_spacing.x
                + ui.label(":").rect.width()
                + ui.spacing().item_spacing.x
                + ui.add(
                    egui::DragValue::new(&mut m)
                        .range(0..=59)
                        .speed(0.2)
                        .custom_formatter(|v, _| format!("{:02}", v as u32)),
                )
                .rect
                .width();
            widths.push(w);
        }
        Item::SpaceConst(v) => ui.add_space(*v),
        Item::SpaceLabelCol {
            key,
            plus_item_spacing,
        } => {
            let font_id = ui.style().text_styles[&egui::TextStyle::Body].clone();
            let color = ui.visuals().text_color();
            let w = ui
                .fonts_mut(|f| f.layout_no_wrap(need(key), font_id, color))
                .size()
                .x;
            let extra = if *plus_item_spacing {
                ui.spacing().item_spacing.x
            } else {
                0.0
            };
            ui.add_space(w + extra);
        }
        Item::Combo { options } => {
            let opts: Vec<String> = options.iter().map(|k| need(k)).collect();
            let longest = opts
                .iter()
                .max_by_key(|s| s.chars().count())
                .cloned()
                .unwrap_or_default();
            let r = egui::ComboBox::from_id_salt("layout_width_probe")
                .selected_text(longest)
                .show_ui(ui, |_| {});
            push(r.response.rect.width(), "combo");
        }
        Item::Preview => {
            ui.allocate_space(egui::vec2(PREVIEW_MAX_W, 16.0));
        }
        // `Alternatives` 在 `row_variants` 里就展开掉了，走不到这里
        Item::Alternatives(_) => unreachable!("Alternatives 由 row_variants 预先展开"),
    }
}

/// `src/ui.rs` 的 `hours_label`（滑条数值的显示形态）。
fn hours_label(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn lang_name_to_chain(lang: &str) -> &'static str {
    if lang == "zh-CN" {
        "ZH_FONTS"
    } else {
        "EN_FONTS"
    }
}

/// 一次性把所有行放进**同一个**测量台，返回 `(卡片内容区宽, 每行的右缘)`。
///
/// 一行里若含 `match` 的互斥分支，取**各分支里最宽的那个**（同一时刻只渲染一个）。
fn measure_rows(which: &str, table: &Table, rs: &[Row]) -> (f32, Vec<f32>) {
    let (content_w, right_edges) = measure(which, |ui| {
        let mut edges = Vec::with_capacity(rs.len());
        for r in rs {
            let mut best = 0.0f32;
            for variant in row_variants(&r.items) {
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let mut item_widths = Vec::new();
                        for it in &variant {
                            lay_item(ui, it, table, &mut item_widths);
                        }
                        let w = ui.min_rect().width();
                        if w > best {
                            best = w;
                        }
                    });
                });
            }
            edges.push(best);
        }
        edges
    });
    (content_w, right_edges)
}

// ---------------------------------------------------------------------------
// fixture 自检
// ---------------------------------------------------------------------------

/// fixture 自检：卡片内容区宽确实是 546.0。
///
/// 这条**先于**所有宽度断言跑（名字排序 + 独立测试都保证它是最早暴露问题的）——
/// 上限本身错了的话，后面每一条宽度断言都会用错分母。
#[test]
fn fixture_content_width_is_546() {
    let (w, _) = measure("EN_FONTS", |_ui| Vec::new());
    assert!(
        (w - EXPECTED_CONTENT_W).abs() <= WIDTH_TOL,
        "卡片内容区宽度是 {w:.2} pt，不再是验收现场那个 {EXPECTED_CONTENT_W:.1} pt。\n\
         窗口 {WINDOW_W} pt @1.25 DPI，扣掉 egui 窗口边框 + `Frame::group` 的 inner margin。\n\
         数字变了通常意味着：egui 改了窗口/group 的默认 margin，或 src/ui.rs 的 card() 改了内边距。\n\
         无论哪种，**所有宽度断言的分母都得跟着改**——先确认改哪边，再改 EXPECTED_CONTENT_W。"
    );
}

/// 自检：字体链与 `tune_style` 真的被解析到了（否则下面的断言会安静地量错东西）。
#[test]
fn fixture_parses_app_rs_style_and_fonts() {
    let sp = tune_spacing();
    let names: Vec<&str> = sp.iter().map(|(n, _)| n.as_str()).collect();
    for want in ["button_padding", "item_spacing", "slider_width"] {
        assert!(
            names.contains(&want),
            "从 {APP_RS} 的 tune_style 里没解析出 spacing.{want}，实际解析到 {names:?}。解析器跟不上 tune_style 的写法变化时，测量用的间距就不是产品间距了。"
        );
    }
    assert!(
        tune_spacing()
            .iter()
            .any(|(n, v)| n == "slider_width" && *v > 100.0),
        "tune_style 的 slider_width 应为本版调高后的 220.0，实际解析到 {:?}。\
         本版事故的直接成因就是这一行从 100 抬到 220。",
        tune_spacing()
    );
    for which in ["ZH_FONTS", "EN_FONTS"] {
        let chain = font_chain(which);
        assert!(
            chain.iter().any(|p| p.ends_with("seguisym.ttf")),
            "{APP_RS} 的 {which} 链里没有 seguisym.ttf 回退，实际是 {chain:?}。"
        );
    }
}

// ---------------------------------------------------------------------------
// A1：Extend 布局里的内联文案单行宽度
// ---------------------------------------------------------------------------

/// A1 主断言：**落在不换行布局里的**内联文案，单行渲染宽度 ≤ 卡片内容区宽。
///
/// 竖直布局 / `horizontal_wrapped` 里的控件由 egui 自己折行（见文件头的口径
/// 说明），不在本闸门管辖内。
#[test]
fn inline_text_fits_card_content_width() {
    let sites = inline_sites();
    let extend: Vec<&Site> = sites.iter().filter(|s| s.extend).collect();
    assert!(
        sites.len() >= 30,
        "只从 {UI_RS} 抽到 {} 处内联文案（预期 ≥30）。解析器退化了，\
         这条断言会安静地什么都不检查。抽样: {sites:?}",
        sites.len()
    );
    assert!(
        extend.len() >= 20,
        "{UI_RS} 里只有 {} 处内联文案被判成 Extend 布局（预期 ≥20）。\
         布局上下文判定（horizontal_spans）退化了，这条断言几乎不检查任何东西。",
        extend.len()
    );

    let zh = load_locale("zh-CN");
    let en = load_locale("en-US");

    let mut problems: Vec<String> = Vec::new();
    // 缺 key 不是宽度问题，单独报（build.rs 本该拦住，出现在这里是别处漏了）
    let mut missing: Vec<String> = Vec::new();

    for (lang_name, table) in [("zh-CN", &zh), ("en-US", &en)] {
        let mut pending: Vec<Site> = extend.iter().map(|s| (*s).clone()).collect();
        let (_, widths) = measure(lang_name_to_chain(lang_name), |ui| {
            let mut out = Vec::new();
            for s in pending.drain(..) {
                let Some(value) = table.get(&s.key) else {
                    missing.push(format!(
                        "  {lang_name}  {UI_RS}:{} {} 缺失 locale key {:?}",
                        s.line, s.kind, s.key
                    ));
                    continue;
                };
                let text = render(value);
                // 复刻「它本来所在的那种布局」：`ui.horizontal`（Extend ⇒ 单行不换行）
                ui.horizontal(|ui| {
                    let w = match s.kind {
                        Kind::Checkbox => {
                            let mut b = false;
                            ui.checkbox(&mut b, text.clone()).rect.width()
                        }
                        Kind::Label => ui.label(text.clone()).rect.width(),
                        Kind::Weak => ui.weak(text.clone()).rect.width(),
                        Kind::ColoredLabel => ui
                            .colored_label(ui.visuals().text_color(), text.clone())
                            .rect
                            .width(),
                        Kind::Button => ui.button(text.clone()).rect.width(),
                        Kind::SmallButton => ui.small_button(text.clone()).rect.width(),
                        Kind::SelectableValue => {
                            ui.selectable_label(false, text.clone()).rect.width()
                        }
                        Kind::HyperlinkTo => ui
                            .hyperlink_to(text.clone(), "https://example.invalid/")
                            .rect
                            .width(),
                    };
                    out.push(w);
                });
            }
            out
        });
        for (w, s) in widths.iter().zip(extend.iter()) {
            if *w > EXPECTED_CONTENT_W + WIDTH_TOL {
                problems.push(format!(
                    "  {lang_name}  {UI_RS}:{} {} {:?}  实测 {w:.1} pt > 上限 {EXPECTED_CONTENT_W:.1} pt  (超出 {:+.1} pt)",
                    s.line,
                    s.kind,
                    s.key,
                    *w - EXPECTED_CONTENT_W
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "以下内联文案在 locale 表里查不到 key（build.rs 按理会让构建失败，出现这里\
         说明是 `locales/*.toml` 与 `{UI_RS}` 脱节）：\n{}\n\
         这些位置没有被测量，宽度断言对它们是**没检查**的。",
        missing.join("\n")
    );

    assert!(
        problems.is_empty(),
        "以下 {} 处内联文案**在不换行布局里**排不下（{} pt 窗口 / {:.1} pt 内容区）：\n{}\n\n\
         英文侧通常才是危险的一侧（汉字宽度远小于等长英文）。修法二选一：\n\
         · 把长说明搬进 `.on_hover_text(...)`（本项目已有 6 处同款先例，`ui.weak_*` 系列）；\n\
         · 或者把标签改短——文案在 en-US.toml / zh-CN.toml 里，键名不变。\n\n\
         口径：**只有 Extend 布局会裁**。处在竖直布局或 `ui.horizontal_wrapped` 里的控件，\n\
         egui 的 `Ui::wrap_mode()`（`egui-0.36.1/src/ui.rs:588-604`）给的是 \
         `TextWrapMode::Wrap`，`ui.label` / `ui.weak` / `ui.checkbox` 走 `AtomLayout` 的 \
         shrink 分支（`atomics/atom_layout.rs:282-295`）同样折行，**不会横向裁切**，\
         所以不在本闸门管辖内。上面报出来的每一处都真的落在 `ui.horizontal` 里。",
        problems.len(),
        WINDOW_W,
        EXPECTED_CONTENT_W,
        problems.join("\n")
    );
}

// ---------------------------------------------------------------------------
// A2：每个 ui.horizontal 行的右缘
// ---------------------------------------------------------------------------

/// A2：`src/ui.rs` 里**每一个** `ui.horizontal` 族块，整体右缘 ≤ 546.0 pt。
///
/// 这一条才是本次事故的真正形状：**单条文案全都合格，并排之后越界。** A1
/// 逐条量文案，看不见「标签 + 220 滑条」这种加法。
///
/// 行的内容不是手抄的：`row_items` 从源码现场解析出每一个条目，**真的** add 进
/// 一个 `ui.horizontal`，读 `ui.min_rect().width()`。任何新增的、解析器不认识的
/// 控件都会让 `row_items` 里的两道同步核对当场 panic——宁可红，也不许静默漏测。
#[test]
fn ui_horizontal_rows_fit_card_content_width() {
    let rs = rows();
    let zh = load_locale("zh-CN");
    let en = load_locale("en-US");

    let mut problems: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    for (lang_name, table) in [("zh-CN", &zh), ("en-US", &en)] {
        for r in &rs {
            for it in &r.items {
                for k in it.keys() {
                    if !table.contains_key(k) {
                        missing.push(format!(
                            "  {lang_name}  {UI_RS}:{} {} 缺失 locale key {k:?}",
                            r.line,
                            it.tag()
                        ));
                    }
                }
            }
        }
        let (content_w, edges) = measure_rows(lang_name_to_chain(lang_name), table, &rs);
        assert!(
            (content_w - EXPECTED_CONTENT_W).abs() <= WIDTH_TOL,
            "{lang_name} 的卡片内容区宽是 {content_w:.2} pt，不是 {EXPECTED_CONTENT_W:.1} pt"
        );
        for (r, w) in rs.iter().zip(edges.iter()) {
            if *w > EXPECTED_CONTENT_W + WIDTH_TOL {
                let items: Vec<String> = r.items.iter().map(|i| i.tag()).collect();
                let sliders = r
                    .items
                    .iter()
                    .filter(|i| matches!(i, Item::Slider(_)))
                    .count();
                problems.push(format!(
                    "  {lang_name}  {UI_RS}:{} 这一行右缘 {w:.1} pt > 上限 {EXPECTED_CONTENT_W:.1} pt  (超出 {:+.1} pt)\n      条目: {}\n      （该行含 {sliders} 个滑条；`ui.horizontal` 是 Extend，宽度直接相加，没有折行兜底）",
                    r.line,
                    *w - EXPECTED_CONTENT_W,
                    items.join(" + ")
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "以下行内条目在 locale 表里查不到 key（build.rs 按理会让构建失败）：\n{}\n\
         这些行的右缘测量用的是错的文案。",
        missing.join("\n")
    );

    assert!(
        problems.is_empty(),
        "以下 {} 行在 560 pt 窗口（{:.1} pt 内容区）里排不下：\n{}\n\n\
         本版事故的直接成因：`tune_style` 把 `slider_width` 从 100 抬到 220 之后，\
         并排两滑条的行从 510.1 pt 涨到 750.1 pt，越过 546.0 pt 的内容区右缘。\n\
         修法：拆成上下两行（每行「标签 + 一个滑条」），或把标签改短。",
        problems.len(),
        EXPECTED_CONTENT_W,
        problems.join("\n")
    );
}

/// 最坏情况的配置路径行（**假设，不是实测**；兜底是 `src/ui.rs` 里的
/// `Label::truncate()`）。
///
/// 该行的源码是 `ui.horizontal`（Extend）里的「配置路径 + `Open folder` 按钮」。
/// 路径是**唯一无上界的运行期字符串**，所以这里按 20 字符的用户目录名构造一个
/// 最坏 fixture 断言 ≤ 546.0：本机真实路径短得多，但闸门不该只在「恰好短」的
/// 机器上成立。
#[test]
fn worst_case_config_path_row_fits() {
    let zh = load_locale("zh-CN");
    let en = load_locale("en-US");
    let items = vec![
        Item::ConfigPathTrunc,
        Item::SmallButton("ui.open_dir".to_owned()),
    ];
    for (lang_name, table) in [("zh-CN", &zh), ("en-US", &en)] {
        let (_, edges) = measure_rows(
            lang_name_to_chain(lang_name),
            table,
            &[Row {
                line: 0,
                items: items.clone(),
            }],
        );
        let w = edges[0];
        assert!(
            w <= EXPECTED_CONTENT_W + WIDTH_TOL,
            "{lang_name} 的最坏情况配置路径行右缘 {w:.1} pt > 上限 {EXPECTED_CONTENT_W:.1} pt (超出 {:+.1} pt)。\n\
             fixture: {WORST_CONFIG_PATH}\n  \
             这一行的兜底是 `src/ui.rs` 里的 `Label::truncate()`——它会把路径省略成一行、\
             把全文挂到 hover 上。真要收口就把按钮挪到下一行，或给 Label 一个显式的 \
             `max_width`（留下按钮的宽度）。",
            w - EXPECTED_CONTENT_W
        );
    }
}

// ---------------------------------------------------------------------------
// 解析器自检（防止 A1 / A2 因为解析退化而空转）
// ---------------------------------------------------------------------------

#[test]
fn parser_finds_inline_and_hover_sites() {
    let sites = inline_sites();
    let keys: BTreeSet<&str> = sites.iter().map(|s| s.key.as_str()).collect();

    // 已知内联：直接传给控件的短标签
    for want in ["ui.enable_reminders", "ui.sound", "ui.esc_skip", "ui.saved"] {
        assert!(
            keys.contains(want),
            "{want} 应当被认成内联文案，实际抽到 {keys:?}。\
             若它是被误判成 hover（或根本没抽到），A1 就漏检了这条。"
        );
    }
    // 已知 hover：只在 on_hover_text 里出现的长说明
    let hover_only = [
        "ui.weak_aoa",
        "ui.weak_away",
        "ui.weak_hotkey",
        "ui.weak_sound_advice",
        "ui.weak_visual",
        "ui.weak_strict",
    ];
    for want in hover_only {
        assert!(
            !keys.contains(want),
            "{want} 只出现在 `.on_hover_text(...)` 里（长说明搬进 hover 是本项目\
             已定的做法），不该被算成内联。它被算进来了说明 hover 区间匹配坏了。"
        );
    }
    // 已知内联的「短标签 + hover 说明」那一对
    assert!(
        keys.contains("ui.short_interval") || keys.contains("ui.heads_up"),
        "滑条行的短标签（ui.short_interval / ui.heads_up）应被认成内联，实际 {keys:?}"
    );
    // T5：运行期 key（`tr(<枚举>.key())`）也必须进被测集合
    assert!(
        keys.contains("flow.high"),
        "`flow.high` 来自 `src/ui.rs` 的 `tr(f.key())`（FlowSensitivity::key），\
         T5 应当把该枚举的全部取值展开收进来。实际抽到 {keys:?}"
    );
    for want in ["sound.triple_beep", "fit.contain", "lang.zh_cn"] {
        assert!(
            keys.contains(want),
            "`{want}` 同属 `tr(<枚举>.key())` 的运行期 key，应当也被展开收进来。实际 {keys:?}"
        );
    }
    // T1：布局上下文判定（`ui.quiet_hours` 在 `ui.horizontal` 里 ⇒ Extend；
    // `ui.weak_flow` 在卡片的竖直布局里 ⇒ 不是 Extend）
    let find = |key: &str| -> &Site {
        sites
            .iter()
            .find(|s| s.key == key)
            .unwrap_or_else(|| panic!("内联集合里没有 {key}，实际 {keys:?}"))
    };
    assert!(
        find("ui.quiet_hours").extend,
        "ui.quiet_hours 位于 `ui.horizontal(...)` 里（免打扰时段那一行），必须被判成 Extend"
    );
    assert!(
        !find("ui.weak_flow").extend,
        "ui.weak_flow 是心流灵敏度卡片里 `ui.horizontal` **外面**的竖直 `ui.weak`，\
         必须被判成非 Extend——它在竖直布局里会被 egui 折行，横向裁不到"
    );
    // 每个 site 都要有行号，报错才指得准
    assert!(sites.iter().all(|s| s.line > 0), "有 site 没算出行号");
}
