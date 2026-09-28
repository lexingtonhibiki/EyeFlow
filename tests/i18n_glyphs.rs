//! ADR-0008 **T11 字形覆盖**。
//!
//! 「按语言换字体集」设计下真正的失效模式不是「英文表里混进中文」，而是
//! **「某条文案里的某个非 ASCII 字符，在该语言实际加载的字体里没有 glyph」** ——
//! 例如英文模式加载 Segoe UI 后，文案里的 `●`(U+25CF) 画不出来。
//!
//! 实现方式：**思路 1（查真字体数据的 cmap）**。`epaint` / `ab_glyph` 不是本 crate 的
//! 直接依赖（加进去就违反 ADR-0008「零新增依赖」），所以本文件自带一个最小
//! TrueType/OpenType cmap 查询器（format 4 + format 12，TTC 走 `ttcf` 头），
//! 直接读 `C:\Windows\Fonts\*.tt[fc]` 的字符映射表。真数据，不是人工维护的白名单。
//!
//! 字体集合的来源：文件顶部那三个常量写死的是**测试期望值**，产品侧的真实值由
//! **T11e 从 `src/app.rs` 解析出来**（剥注释 + 定位 `const ZH_FONTS` / `EN_FONTS`
//! 的数组区间）。T11e 双向比对这两份清单，所以这个文件里的常量不是「抄一份」，
//! 而是「与产品绑定的契约」：产品改了链而这里没改（或反过来），T11e 立刻变红。
//!
//! v0.6.1 起的两条链：
//! - zh-CN：`msyh.ttc`（对 CJK 统一表意文字区 U+4E00–U+9FFF 覆盖 20992/20992）
//!   + 链尾 `seguisym.ttf` 符号回退
//! - en-US：Segoe UI Regular + 链尾 `seguisym.ttf`
//!
//! 被删掉的 `msyhl.ttc` / `simhei.ttf` 相对这条链**一个文案字符都没多覆盖**。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const FONT_DIR: &str = r"C:\Windows\Fonts";

/// ADR-0008：英文模式加载的字体。**只有 Regular** —— epaint 0.36.1 的 `FontId`
/// 还没有 `FontWeight` 字段（`src/app.rs` 的 `EN_FONTS` 文档注释里有完整推导），
/// 所以 `segoeuib.ttf` 在 v0.6.0 就已经删掉了，链里不该再有它。
const EN_FONTS: &[&str] = &["segoeui.ttf"];
/// v0.6.1 的中文回退链（zh-CN 模式）。`msyh.ttc` 一个字体就覆盖了全部 508 个去重
/// 文案字符中除符号以外的部分；`msyhl.ttc` / `simhei.ttf` 相对它只多出 25 个 PUA
/// （U+E78D–U+E864）和 `ﬁ ﬂ ﴾ ﴿`，而文案里一个都没用到，所以删了。
const ZH_FONTS: &[&str] = &["msyh.ttc"];
/// **两种语言都要挂的符号回退**（`app.rs` 的 `ZH_FONTS` / `EN_FONTS` 都在链里）。
///
/// 2026-09-26 实测（本轮发现并修掉的存量豆腐块 bug）：`✓`(U+2713) 与 `▶`(U+25B6)
/// 在 `msyh.ttc` / `msyhl.ttc` / `simhei.ttf` / `segoeui.ttf` 里**全部没有 glyph**，
/// 而它们出现在「已保存 ✓」「▶ 试听」两处界面上。egui 没有系统级字体回退，缺字
/// 就是方块。修法是把 `seguisym.ttf` 注册为两种语言的回退字体（它覆盖全部风险
/// 字符）。T11d 钉住两条链都真的有它。
const FALLBACK_FONTS: &[&str] = &["seguisym.ttf"];
/// Windows 自带的符号/emoji 字体：egui `default_fonts` 自带 NotoEmoji，
/// 所以 emoji 只需在「非 emoji 字体都画不出」时才要求系统符号字体兜底
const SYMBOL_FONTS: &[&str] = &["seguiemj.ttf", "seguisym.ttf"];

/// `src/app.rs` 里两条字体链的名字；T11b / T11d 的报错信息要用
const FONTS_HEAD: &str = "ZH_FONTS / EN_FONTS";

/// ADR-0008 / 任务简报点名的危险字符（每个都给出出处）
const RISK_CHARS: &[(char, &str)] = &[
    ('●', "ui.rs:159 托盘状态点"),
    ('✓', "ui.rs:648,685 保存成功"),
    ('▶', "ui.rs:345 状态图例"),
    ('·', "app.rs:412 组合标签分隔"),
    ('×', "wallpaper.rs:258 严格模式关闭标记"),
    // 托盘菜单的 `…`（Windows 菜单惯例，收尾的「打开设置…」）。实测
    // msyh.ttc / segoeui.ttf / seguisym.ttf **都**有 U+2026 的 glyph，所以
    // 两条链现在都画得出；钉在这里是为了「以后换字体」时它不会变成一个豆腐块。
    ('…', "locales tray.open_settings 菜单省略号"),
    ('\u{1F440}', "app.rs:288 眼睛"),
    ('\u{1F44F}', "app.rs:347 鼓掌"),
];

// ---------------------------------------------------------------------------
// 从 src/app.rs 解析字体链（T11d / T11e 专用）
//
// 为什么不能只 `src.contains("msyhl.ttc")`：源码的**注释**里提到一个字体名，
// 断言就会通过。实测这不是假设：`segoeuib.ttf` 早在 v0.6.0 就从 `EN_FONTS` 里
// 删掉了，可旧 T11d 至今仍然绿——它命中的是 app.rs:545 的一行文档注释。
// 同样的盲区意味着：产品把 `msyhl` / `simhei` 删掉之后，全套测试仍然全绿。
// 下面这两步（剥注释 → 定位 `const … = &[ … ];` 的区间）就是把那层盲区关掉。
// ---------------------------------------------------------------------------

/// 剥掉 `//`（含 `///`）行注释与 `/* */`（可嵌套）块注释，其余字节原样保留。
///
/// 走**字节**而不是 char：所有分隔符都是 ASCII，UTF-8 多字节序列里不会出现
/// ASCII 字节，按字节扫描不会切坏中文字符。字符串字面量必须单独处理（路径里的
/// `\\` 不是转义到注释的信号，但 `"` 里的 `//` 不能当注释）；`'` 也要分清字符
/// 字面量 `'x'` 和生命周期 `&'static str`，否则 `'` 会把后面的 `"` 吞掉。
fn strip_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
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
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            depth = 1;
            i += 2;
        } else if c == b'"' {
            // 原始字符串（`r"…"` / `r#"…"#`）本解析器不支持：宁可 panic，
            // 也不要静默把 `r"https://…"` 里的 `//` 当成行注释。
            assert!(
                !(i > 0 && (b[i - 1] == b'r' || (b[i - 1] == b'#' && i > 1 && b[i - 2] == b'r'))),
                "src/app.rs 里出现了原始字符串字面量：T11d/T11e 的注释剥除器 \
                 不支持它，请先扩展 strip_comments（位置 byte {i}）"
            );
            out.push(c);
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' && i + 1 < b.len() {
                    out.push(b[i]);
                    out.push(b[i + 1]);
                    i += 2;
                    continue;
                }
                out.push(b[i]);
                let end = b[i] == b'"';
                i += 1;
                if end {
                    break;
                }
            }
        } else if c == b'\'' {
            // `'<一个字符>'` 或 `'\<转义>'` 才是字符字面量；`'static` 里的 `'`
            // 是生命周期标注，落到 else 分支原样输出。
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
                out.extend_from_slice(&b[i..i + len]);
                i += len;
            } else {
                out.push(c);
                i += 1;
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    String::from_utf8(out).expect("剥注释不改变任何字节，输出必然是合法 UTF-8")
}

/// 解析一个字符串字面量（`b[i] == b'"'`），返回 `(已解转义的内容, 下一个字节下标)`。
fn parse_str_lit(b: &[u8], i: usize) -> Option<(String, usize)> {
    if b.get(i) != Some(&b'"') {
        return None;
    }
    let (mut s, mut j) = (String::new(), i + 1);
    while j < b.len() && b[j] != b'"' {
        if b[j] == b'\\' {
            // 字体路径里只会出现 `\\`；其余转义照 Rust 规则解，解不了就原样保留
            // 那个字符（`'\\'` 之外的整字节，UTF-8 里不会走到这里）。
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

/// 从**已剥注释**的源码里抽出 `const <name>: &[FontFile] = &[ … ];` 中每一个
/// `font("key", "path")` 的 `(key, 已解转义的完整路径)`，保持源码里的顺序。
fn parse_font_chain(code: &str, name: &str) -> Vec<(String, String)> {
    let b = code.as_bytes();
    let decl = code
        .find(&format!("const {name}"))
        .unwrap_or_else(|| panic!("src/app.rs 里找不到 `const {name}` —— 字体链被改名/挪走/内联进别的结构了？T11d/T11e 解析不到字体链等于这两条护栏失效"));
    let open = code[decl..]
        .find("= &[")
        .map(|p| decl + p + 3)
        .unwrap_or_else(|| panic!("`const {name}`（byte {decl}）后面不是 `= &[ … ]` 数组字面量 —— 写法变了的话请同步改 parse_font_chain"));

    // 方括号配平（跳过字符串字面量里的 `]`）
    let (mut depth, mut i) = (0usize, open);
    let close = loop {
        assert!(i < b.len(), "`const {name}` 的数组没有闭合");
        match b[i] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break i;
                }
            }
            b'"' => {
                i = parse_str_lit(b, i).expect("数组里有未闭合的字符串字面量").1;
                continue;
            }
            _ => {}
        }
        i += 1;
    };

    let mut out = Vec::new();
    let mut rest = &code[open..=close];
    while let Some(p) = rest.find("font(") {
        let after = p + "font(".len();
        let (key, next) = parse_str_lit(rest.as_bytes(), after).unwrap_or_else(|| {
            panic!(
                "`font(` 的第一个实参不是字符串字面量: {:?}",
                peek(rest, after)
            )
        });
        let bb = rest.as_bytes();
        let mut q = next;
        while matches!(bb.get(q), Some(b' ' | b',' | b'\n' | b'\r' | b'\t')) {
            q += 1;
        }
        let (path, next2) = parse_str_lit(bb, q)
            .unwrap_or_else(|| panic!("`font(\"{key}\", …)` 的第二个实参不是字符串字面量"));
        out.push((key, path));
        rest = &rest[next2..];
    }
    out
}

fn peek(s: &str, at: usize) -> String {
    s[at..(at + 40).min(s.len())].to_owned()
}

/// 路径 → 文件名（`C:\Windows\Fonts\msyh.ttc` → `msyh.ttc`）
fn file_name(p: &str) -> &str {
    p.rsplit(['\\', '/']).next().unwrap_or(p)
}

fn app_rs_code() -> String {
    let src = std::fs::read_to_string(Path::new(ROOT).join("src").join("app.rs"))
        .expect("读 src/app.rs 失败");
    strip_comments(&src)
}

// ---------------------------------------------------------------------------
// 最小 cmap 查询器
// ---------------------------------------------------------------------------
struct Cmap {
    data: Vec<u8>,
    sub: usize,
    off: usize,
    fmt: u16,
}

fn be16(d: &[u8], o: usize) -> Option<u16> {
    let s = d.get(o..o + 2)?;
    Some(u16::from_be_bytes([s[0], s[1]]))
}

fn be32(d: &[u8], o: usize) -> Option<u32> {
    let s = d.get(o..o + 4)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

impl Cmap {
    /// 打开一个 TTF / TTC / OTF，取最合适的 cmap 子表
    fn open(path: &Path) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        // `ttcf` = TrueType Collection（msyh.ttc 是 3 个 face 的集合）
        let base = if data.get(0..4) == Some(b"ttcf".as_slice()) {
            let n = be32(&data, 8)? as usize;
            if n == 0 {
                return None;
            }
            be32(&data, 12)? as usize
        } else {
            0
        };
        let num_tables = be16(&data, base + 4)? as usize;
        let mut sub = 0usize;
        for i in 0..num_tables {
            let rec = base + 12 + i * 16;
            if data.get(rec..rec + 4) == Some(b"cmap".as_slice()) {
                sub = be32(&data, rec + 8)? as usize;
            }
        }
        if sub == 0 {
            return None;
        }
        // 选子表：优先 (3,10) format 12（完整 Unicode），退 (3,1) format 4
        let n = be16(&data, sub + 2)? as usize;
        let mut best: Option<(i32, usize, u16)> = None;
        for i in 0..n {
            let rec = sub + 4 + i * 8;
            let (plat, enc) = (be16(&data, rec)?, be16(&data, rec + 2)?);
            let off = be32(&data, rec + 4)? as usize;
            // 注意：子表偏移是相对 cmap 表起点，不是文件起点
            let fmt = be16(&data, sub + off)?;
            let score = match (plat, enc, fmt) {
                (3, 10, 12) => 5,
                (0, 4, 12) | (0, 6, 12) => 4,
                (3, 1, 4) => 3,
                (0, 3, 4) | (0, 2, 4) | (0, 1, 4) | (0, 0, 4) => 2,
                (_, _, 4) => 1,
                _ => -1,
            };
            if score >= 0 && best.is_none_or(|(bs, _, _)| score > bs) {
                best = Some((score, off, fmt));
            }
        }
        let (_, off, fmt) = best?;
        Some(Cmap {
            data,
            sub,
            off,
            fmt,
        })
    }

    fn u16(&self, o: usize) -> Option<u16> {
        be16(&self.data, self.sub + o)
    }
    fn u32(&self, o: usize) -> Option<u32> {
        be32(&self.data, self.sub + o)
    }

    fn lookup4(&self, c: u32) -> Option<u16> {
        let off = self.off;
        let segx2 = self.u16(off + 6)? as usize;
        let seg = segx2 / 2;
        let ends = off + 14;
        let starts = ends + segx2 + 2;
        let deltas = starts + segx2;
        let rangeoffs = deltas + segx2;
        let c16 = c as u16;
        for i in 0..seg {
            let end = self.u16(ends + i * 2)?;
            if c16 <= end {
                let start = self.u16(starts + i * 2)?;
                if c16 < start {
                    return None;
                }
                let delta = self.u16(deltas + i * 2)?;
                let ro = self.u16(rangeoffs + i * 2)? as usize;
                if ro == 0 {
                    return Some(c16.wrapping_add(delta));
                }
                let addr = rangeoffs + i * 2 + ro + (c16 - start) as usize * 2;
                let gid = self.u16(addr)?;
                if gid == 0 {
                    return Some(0);
                }
                return Some(gid.wrapping_add(delta));
            }
        }
        None
    }

    fn lookup12(&self, c: u32) -> Option<u16> {
        let off = self.off;
        let groups = self.u32(off + 12)? as usize;
        for i in 0..groups {
            let g = off + 16 + i * 12;
            let (s, e, sg) = (self.u32(g)?, self.u32(g + 4)?, self.u32(g + 8)?);
            if c < s {
                break;
            }
            if c <= e {
                let gid = sg.checked_add(c - s)?;
                return Some(if gid > 0xFFFF { 0 } else { gid as u16 });
            }
        }
        None
    }

    fn gid(&self, c: char) -> Option<u16> {
        let cp = c as u32;
        if self.fmt == 12 {
            self.lookup12(cp)
        } else if cp > 0xFFFF {
            // format 4 用代理对：先查高代理拿到 glyph id，再算低代理的码位
            let v = cp - 0x1_0000;
            let hi = self.lookup4(0xD800 + (v >> 10))?;
            if hi == 0 {
                return Some(0);
            }
            let lo = 0xDC00 + (v & 0x3FF);
            self.lookup4(lo + ((hi as u32) << 10))
        } else {
            self.lookup4(cp)
        }
    }

    /// 该字体是否为 `c` 提供了非零 glyph
    fn covers(&self, c: char) -> bool {
        matches!(self.gid(c), Some(g) if g != 0)
    }
}

fn open_all<'a>(names: &'a [&'a str]) -> Vec<(&'a str, Cmap)> {
    let mut out = Vec::new();
    for n in names.iter().copied() {
        let p = PathBuf::from(FONT_DIR).join(n);
        if let Some(c) = Cmap::open(&p) {
            out.push((n, c));
        }
    }
    out
}

fn chars_in(values: impl Iterator<Item = String>) -> BTreeSet<char> {
    let mut set = BTreeSet::new();
    for v in values {
        for c in v.chars() {
            if !c.is_ascii() {
                set.insert(c);
            }
        }
    }
    set
}

fn locale_values(code: &str) -> Result<Vec<String>, String> {
    let path = Path::new(ROOT).join("locales").join(format!("{code}.toml"));
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let table: toml::Table = raw
        .parse()
        .map_err(|e| format!("{} 解析失败: {e}", path.display()))?;
    Ok(table
        .into_iter()
        .map(|(_, v)| v.as_str().unwrap_or_default().to_owned())
        .collect())
}

// ---------------------------------------------------------------------------
// T11a：本轮价值最高的一条 —— 每个非 ASCII 字符都必须能被该语言加载的字体画出
// ---------------------------------------------------------------------------
/// 某个语言**实际加载**的字体链：语言字体 + 链尾的符号回退。
///
/// ⚠️ 2026-09-26 修订。原来的 T11a 只查 `EN_FONTS` / `ZH_FONTS` 本身，于是它会把
/// **产品做对的事报成 bug**：`app.rs` 现在给两条链都接上了 `seguisym.ttf` 兜底，
/// 但测试没把它算进「该语言加载的字体集」。`probe_coverage` 的实测矩阵是：
///
/// ```text
/// segoeui.ttf  +● -✓ -▶ +· +×
/// simhei.ttf   +● -✓ -▶ +· +×      ← 连黑体都没有 ✓ / ▶
/// seguisym.ttf +● +✓ +▶ +· +×
/// ```
///
/// 也就是说 `✓`(U+2713) 与 `▶`(U+25B6) 在本机的**每一个**中日韩字体里都没有
/// glyph——v0.5.2 的中文界面今天就在显示豆腐块。测试必须查产品真正依赖的那组
/// 字体，否则唯一的「修好它」动作会让测试变红。T11d 另外钉住 `src/app.rs`
/// 真的提到了 `seguisym.ttf`，所以这条断言没有失去牙齿。
fn font_chain(code: &str) -> Vec<&'static str> {
    let mut chain: Vec<&'static str> = match code {
        "en-US" => EN_FONTS.to_vec(),
        _ => ZH_FONTS.to_vec(),
    };
    chain.extend_from_slice(FALLBACK_FONTS);
    chain
}

#[test]
fn t11a_locale_non_ascii_chars_are_covered_by_that_languages_font() {
    for code in ["en-US", "zh-CN"] {
        let values = locale_values(code).unwrap_or_else(|e| {
            panic!("读 locale 表失败（ADR-0008 要求 locales/{code}.toml 存在）: {e}")
        });
        let fonts = font_chain(code);
        let loaded = open_all(&fonts);
        assert!(
            !loaded.is_empty(),
            "{FONT_DIR} 下一个候选字体都打不开（找的是 {fonts:?}）—— \
             T11 无法判定，等于这条护栏失效"
        );
        let mut missing = Vec::new();
        for c in chars_in(values.into_iter()) {
            if !loaded.iter().any(|(_, m)| m.covers(c)) {
                missing.push(format!("{c} (U+{:04X})", c as u32));
            }
        }
        assert!(
            missing.is_empty(),
            "{code} 文案里有 {} 个字符在 {fonts:?} 里没有 glyph —— \
             切到 {code} 后这些位置会画成方块（ADR-0008 T11）:\n  {}",
            missing.len(),
            missing.join("\n  ")
        );
    }
}

// ---------------------------------------------------------------------------
// T11b：BMP 内的符号字符必须被「该语言实际加载的字体集」覆盖
//
// ⚠️ 2026-09-26 修订。原版断言 `RISK_CHARS` **全部**（含 👀 👏）都要被
// `EN_FONTS` / `ZH_FONTS` 覆盖，那是本机可证伪的：五个字体**没有一个**有
// 👀/👏 的 glyph，✓/▶ 也一样。而同一文件下面的 T11c 明写着「emoji 不在语言
// 字体里是设计允许的」——两条断言互相矛盾，T11b 的那半边是不成立的。
//
// 修法不是放宽，而是把断言改成**产品真正依赖的不变式**：BMP 符号由
// 「语言字体 + 链尾的 seguisym 回退」覆盖，astral 平面（emoji）交给 T11c。
// T11d 另外钉住 `src/app.rs` 真的把 `seguisym.ttf` 装进了两条字体链——
// 少了它这条会立刻变红，所以这里没有丢断言，是换了一个能判真假的判据。
// ---------------------------------------------------------------------------
#[test]
fn t11b_known_risk_symbols_are_covered_by_each_languages_font() {
    for code in ["en-US", "zh-CN"] {
        let chain = font_chain(code);
        let loaded = open_all(&chain);
        assert!(!loaded.is_empty(), "候选字体都打不开: {chain:?}");
        let mut missing = Vec::new();
        for (c, where_) in RISK_CHARS.iter().filter(|(c, _)| *c as u32 <= 0xFFFF) {
            if !loaded.iter().any(|(_, m)| m.covers(*c)) {
                missing.push(format!("{c} (U+{:04X}) @ {where_}", *c as u32));
            }
        }
        assert!(
            missing.is_empty(),
            "{code} 加载 {chain:?}，但这些字符没有 glyph（{missing:?}）—— \
             检查 app.rs 的 {FONTS_HEAD} 是否仍然带着 seguisym.ttf 这道符号回退\
             （T11d 钉的就是「在链里」，不是「在链尾」：egui 逐字符取 family \
             列表里第一个有 glyph 的字体，位置不影响能不能兜住）"
        );
    }
}

// ---------------------------------------------------------------------------
// T11c：emoji 由系统 emoji 字体兜底（egui default_fonts 自带 NotoEmoji，
// 所以 emoji 不在语言字体里是设计允许的；但系统里必须存在能画它的字体）
// ---------------------------------------------------------------------------
#[test]
fn t11c_risk_emoji_are_covered_by_a_system_symbol_font() {
    let loaded = open_all(SYMBOL_FONTS);
    assert!(
        !loaded.is_empty(),
        "{FONT_DIR} 下没有符号/emoji 字体: {SYMBOL_FONTS:?}"
    );
    for (c, where_) in RISK_CHARS.iter().filter(|(c, _)| *c as u32 > 0xFFFF) {
        assert!(
            loaded.iter().any(|(_, m)| m.covers(*c)),
            "{c} (U+{:04X}) @ {where_} 在 {SYMBOL_FONTS:?} 里都没有 glyph —— \
             egui 没有系统级字体回退，画不出来就是方块",
            *c as u32
        );
    }
}

// ---------------------------------------------------------------------------
// T11d：两条字体链里**必须有**哪些字体（正向存在性）
//
// ⚠️ 2026-09-26 修订。原来这条是 `src.contains("segoeui.ttf")` 之类的源码全文
// grep，而 `src/app.rs` 的文档注释里把四个字体名全提了一遍 —— 于是它在
// `segoeuib.ttf` 早在 v0.6.0 就被删掉之后**依然绿**（命中的是 app.rs:545 的注释）。
// 现在改成解析真实的 `ZH_FONTS` / `EN_FONTS` 数组：注释里提到不算数。
//
// 断言的是「在链里」，不是「在链尾」：egui 0.36 逐字符从 family 列表**头**开始
// 取第一个有 glyph 的字体，所以 `seguisym.ttf` 放在链里任何位置都能兜住 ✓ / ▶
//（它前面那些字体都没有这两个字），位置只影响「谁先命中」的美观问题。
//
// 正向清单刻意不含 `segoeuib.ttf`：epaint 0.36.1 的 `FontId` 还没有 `FontWeight`
// 字段，粗体字体文件挂在链上换不到任何字重，v0.6.0 已经把它删了。T11e 负责保证
// 「删了的东西不许偷偷回来」。
// ---------------------------------------------------------------------------
#[test]
fn t11d_font_install_mentions_both_languages_fonts() {
    let code = app_rs_code();
    for (name, lang_font, lang) in [
        ("ZH_FONTS", "msyh.ttc", "zh-CN"),
        ("EN_FONTS", "segoeui.ttf", "en-US"),
    ] {
        let chain = parse_font_chain(&code, name);
        let files: Vec<&str> = chain.iter().map(|(_, p)| file_name(p)).collect();
        assert!(
            !files.is_empty(),
            "`const {name}` 解析出 0 个字体条目（数组区间没找到 `font(..)` 调用）—— \
             {lang} 模式会退回 egui 默认字体，✓ / ▶ / ● 全部画不出来"
        );
        assert!(
            files.contains(&lang_font),
            "`const {name}` = {files:?} 里没有 {lang_font} —— {lang} 模式的正文 \
             字形靠它画（ADR-0008 判决：中文用微软雅黑、英文用 Segoe UI）。\
             完整条目: {chain:?}"
        );
        // 符号回退：没有它 ✓(U+2713) 与 ▶(U+25B6) 在两种语言下都是豆腐块，
        // 而它们出现在「已保存 ✓」「▶ 试听」两处界面上（见 FALLBACK_FONTS 注释）。
        assert!(
            files.contains(&"seguisym.ttf"),
            "`const {name}` = {files:?} 里没有 seguisym.ttf —— ✓(U+2713) / ▶(U+25B6) \
             只有 seguisym 有 glyph，链里少了它就是两个豆腐块。完整条目: {chain:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// T11e：**字体链绑定** —— 产品真实的链 == 测试期望的清单（双向）
//
// T11a / T11b 查的是「并集覆盖」：砍掉一个字体之后并集变小，但只要剩下的字体
// 仍然覆盖全部文案字符，它们照样绿。所以那两条**抓不到「删多了/删错了字体」**。
// 这条填的就是那个洞：把 `src/app.rs` 里 `ZH_FONTS` / `EN_FONTS` 的真实条目
// 解析出来，与本文件顶部的同名常量逐项比对，两个方向都断言：
//   (a) 源码里有、清单里没有的 → 覆盖测试是**失明**的（多出来的字体从没被检查）
//   (b) 清单里有、源码里没有的 → 产品链被改小了（会画成方块而没人报）
// 改字体链时两边必须一起改。
// ---------------------------------------------------------------------------
#[test]
fn t11e_font_chain_in_app_rs_equals_expected_files() {
    let code = app_rs_code();
    for (name, code_id) in [("ZH_FONTS", "zh-CN"), ("EN_FONTS", "en-US")] {
        // 期望值 = **覆盖测试实际检查的那一组**（语言字体 + 符号回退，见
        // `font_chain`）。这样 T11e 断言的正是「T11a / T11b 度量的那个集合
        // == 产品真正加载的那个集合」——两者一旦分叉，覆盖测试就在替一个
        // 产品没加载的字体组背书。
        let expected = font_chain(code_id);
        let chain = parse_font_chain(&code, name);
        assert!(
            !chain.is_empty(),
            "`const {name}` 解析出 0 个字体条目 —— T11e 抓不到「整条链被换掉」，\
             等于没有牙齿。检查 parse_font_chain 是否还认得现在的写法"
        );
        let got: Vec<&str> = chain.iter().map(|(_, p)| file_name(p)).collect();

        // (a) 源码多出来的：测试没预期 → 覆盖检查对它完全失明
        let unexpected: Vec<&&str> = got.iter().filter(|f| !expected.contains(f)).collect();
        // (b) 测试期望的：源码里没有 → 产品链缺字
        let missing: Vec<&&str> = expected.iter().filter(|f| !got.contains(f)).collect();
        assert!(
            unexpected.is_empty() && missing.is_empty(),
            "`const {name}` 的真实字体链与测试期望的不一致（这是**绑定**断言，\
             与「有没有 glyph」无关）：\n  \
             src/app.rs 解析出 : {got:?}\n  \
             测试期望（清单）   : {expected:?}\n  \
             源码多出（测试没预期 → T11a/T11b 对它失明）: {unexpected:?}\n  \
             源码缺少（产品链被改小 → 这些字会画成方块）: {missing:?}\n  \
             完整条目: {chain:?}\n  \
             改字体链时两边都要改：`src/app.rs` 的 `const {name}` + 本文件顶部的 \
             {name} / FALLBACK_FONTS 常量。"
        );

        // (c) 路径必须落在 FONT_DIR 下。T11a / T11b 是按 `FONT_DIR\\<文件名>`
        //     打开字体的，产品若把字体换到别的目录（或改成相对路径），覆盖测试
        //     会一直在检查一个产品根本不加载的文件。
        for (key, path) in &chain {
            assert_eq!(
                path,
                &format!("{FONT_DIR}\\{}", file_name(path)),
                "`const {name}` 的条目 font({key:?}, {path:?}) 不在 {FONT_DIR} 下 —— \
                 T11a / T11b 是从 {FONT_DIR} 打开字体的，路径对不上时它们检查的 \
                 字体和运行时加载的不是一个"
            );
        }
    }
}

/// 诊断探针：`cargo test --test i18n_glyphs probe_coverage -- --nocapture`
/// 打印每个候选字体对 7 个危险字符的覆盖矩阵（`+` 有 glyph / `-` 没有）。
/// T11 变红时先跑它，就知道是「字体选错了」还是「文案里有画不出的字」。
/// 断言部分只校验环境本身可用（字体能打开），覆盖结论由上面三条测试负责。
#[test]
fn probe_coverage() {
    for n in EN_FONTS.iter().chain(ZH_FONTS).chain(SYMBOL_FONTS) {
        let p = PathBuf::from(FONT_DIR).join(n);
        match Cmap::open(&p) {
            Some(m) => {
                let yes: Vec<String> = RISK_CHARS
                    .iter()
                    .map(|(c, _)| format!("{}{}", if m.covers(*c) { "+" } else { "-" }, c))
                    .collect();
                println!("{n}: fmt={} {}", m.fmt, yes.join(" "));
            }
            None => println!("{n}: <open failed>"),
        }
    }
    assert!(
        !open_all(EN_FONTS).is_empty() && !open_all(ZH_FONTS).is_empty(),
        "{FONT_DIR} 下打不开语言字体（{EN_FONTS:?} / {ZH_FONTS:?}）—— T11 在本机无法判定"
    );
}
