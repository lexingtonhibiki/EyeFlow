//! 防漂移测试：README 单文件中英双段 + `docs/spec.md`（ADR-0008 §README 双语）。
//!
//! 背景：v0.6 把 README 改成**一个文件里放两次完整副本**（English 在前、中文在后），
//! 徽章 / 截图 / 对照表 / 内存实测表这类语言中性内容只出现一次。放弃
//! `README.en.md` 双文件，因为 GitHub 不自动切语言，两文件在 Issues / PR / 搜索上割裂。
//!
//! 代价是「一端加了节另一端没加」变成可能发生的事，所以这里钉三条：
//!
//! 1. **两段 `##` 标题序列结构相同**（抓结构性漂移）
//! 2. **文档声明的配置键集合 == `struct Config` 的 serde 字段集合**
//!    （价值最高的一条：`README.md` 历史上就缺过 4 个字段，且
//!    `custom_sound_path = ""` 与 `config.rs` 的 `Option<String>` 不一致）
//! 3. **文档里每个相对链接真实存在**（死链是唯一用户会立刻发现的文档 bug）
//!
//! **并同时校验 `docs/spec.md`**：`plan-v0.6.md` §2.9 声称 spec.md 是唯一事实来源，
//! 那防漂移测试就必须覆盖它，否则那句话与测试设计自相矛盾。
//!
//! **刻意不加**的三条：两段版本号相等、两段 exe 体积相等、两段测试数相等。
//! 它们把「最容易变的三个数字」绑进「必须精确一致」，会在最高频的动作（改文案、
//! 改体积、加一个测试）上收税，而且为了让它变绿就得往代码里塞一个假常量。
//! 测试数在文档里直接写成 v0.6 落地后的真实数字即可。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn read(rel: &str) -> String {
    let p = Path::new(ROOT).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {} 失败: {e}", p.display()))
}

fn is_identifier(k: &str) -> bool {
    !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 把文档切成 English 段与中文段。
///
/// 分界点是中文段第一行 `# ` 级标题里出现「中文」二字的那一条。
fn split_bilingual(md: &str) -> (&str, &str) {
    let mut zh_start = None;
    let mut offset = 0usize;
    // `split_inclusive` 而不是 `lines()`：`lines()` 会把 CRLF 的 `\r` 去掉，
    // 于是 `line.len() + 1` 每行少算 1 字节，偏移量在几百行之后就会落到
    // 某一行的中间——切出来的「中文段」会从表格中间开始。
    for chunk in md.split_inclusive('\n') {
        let line = chunk.trim_end_matches(['\n', '\r']);
        if line.starts_with("# ") && (line.contains("中文") || line.contains("EyeFlow —")) {
            zh_start = Some(offset);
            break;
        }
        offset += chunk.len();
    }
    let zh_start = zh_start.unwrap_or_else(|| {
        panic!(
            "README 里找不到中文段的开头（一条含「中文」的 `# ` 级标题）—— \
             单文件中英双段的约定变了，请同步修改本测试的分界规则"
        )
    });
    (&md[..zh_start], &md[zh_start..])
}

/// 一段文档的 `##` / `###` 标题（`(层级, 文字)`）。
fn heading_shape(md: &str) -> Vec<(u8, String)> {
    let mut out = Vec::new();
    for line in md.lines() {
        let line = line.trim_end_matches('\r');
        let hashes = line.chars().take_while(|c| *c == '#').count();
        if !(2..=3).contains(&hashes) || hashes > line.len() {
            continue;
        }
        let title = line[hashes..].trim();
        if !title.is_empty() {
            out.push((hashes as u8, title.to_string()));
        }
    }
    out
}

/// 一段文档里每个 `##` 下面的 `###` 数量。
fn sub_counts(md: &str) -> Vec<usize> {
    let mut counts = Vec::new();
    let mut cur: isize = -1;
    for (lvl, _) in heading_shape(md) {
        if lvl == 2 {
            cur += 1;
            counts.push(0);
        } else if cur >= 0 {
            counts[cur as usize] += 1;
        }
    }
    counts
}

/// 一段文档里声明的配置键集合。
///
/// 两种形态都收：
/// - ```toml 围栏块（README 的示例配置）
/// - markdown 表格的第一列（`docs/spec.md` 的配置项清单）
///
/// 围栏里**注释掉的行也算**：两个 `Option<String>` 字段在示例里是注释的
/// （未设置时整行不出现），但它们仍然是合法配置键。
fn declared_keys(md: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let mut in_block = false;
    let lines: Vec<&str> = md.lines().map(|l| l.trim_end_matches('\r')).collect();
    let is_separator = |l: &str| {
        let t = l.trim();
        t.starts_with('|')
            && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
            && t.contains("--")
    };
    for (i, raw) in lines.iter().enumerate() {
        let t = raw.trim();
        if t.starts_with("```") {
            if t.trim_start_matches('`').trim() == "toml" {
                in_block = true;
            } else if in_block {
                in_block = false;
            }
            continue;
        }
        if in_block {
            let body = t.strip_prefix('#').unwrap_or(t).trim();
            // 只看**第一个空白分隔的 token**：`custom_sound_path` 那一行的后半段
            // 里含有 `sound_preset = "custom"`，按 `split_once('=')` 会切错位置。
            let Some(tok) = body.split_whitespace().next() else {
                continue;
            };
            let k = tok.split('=').next().unwrap_or("").trim();
            if is_identifier(k) {
                keys.insert(k.to_string());
            }
        } else if let Some(rest) = t.strip_prefix('|') {
            // 表头行（下一行是 `|---|`）整行跳过：README 的对照表表头
            // （Icon / Platform / Price / Size / State …）不是配置键。
            if lines.get(i + 1).is_some_and(|n| is_separator(n)) {
                continue;
            }
            let cell = rest.split('|').next().unwrap_or("").trim();
            // 只收**小写开头**的单元格。`struct Config` 的字段全是 snake_case
            // 小写；README 对照表 / 上下文表的第一列是 `Platform` / `Desktop` /
            // `Context` 这类大写开头的表头与行首，不是配置键。
            if cell.starts_with(|c: char| c.is_ascii_lowercase()) && is_identifier(cell) {
                keys.insert(cell.to_string());
            }
        }
    }
    keys
}

/// `src/config.rs` 里 `struct Config` 的字段名集合。
///
/// **必须限定在 `struct Config` 块内**：只扫全文会把 `FlowSensitivity` /
/// `SoundPreset` / `WallpaperFit` 这三个枚举的 `#[serde(rename)]` 变体名
/// （`gentle_chime` / `cover` / …）也抓进来，它们不是配置键。
fn config_field_names() -> BTreeSet<String> {
    let src = read("src/config.rs");
    let start = src
        .find("pub struct Config {")
        .unwrap_or_else(|| panic!("src/config.rs 里找不到 `pub struct Config`"));
    let body = &src[start + "pub struct Config {".len()..];
    let end = body
        .find("\n}")
        .unwrap_or_else(|| panic!("`struct Config` 没有闭合的 `}}`"));
    let mut fields = BTreeSet::new();
    for line in body[..end].lines() {
        let Some(rest) = line.trim().strip_prefix("pub ") else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if is_identifier(&name) {
            fields.insert(name);
        }
    }
    assert!(
        fields.len() > 20,
        "只从 `struct Config` 里抓到 {} 个字段，解析多半坏了",
        fields.len()
    );
    fields
}

/// 文档里出现的相对链接（markdown 的 `](…)` 与 HTML 的 `src="…"`）。
fn relative_links(md: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let chars: Vec<char> = md.chars().collect();
    let mut i = 1usize;
    while i < chars.len() {
        // markdown 链接的定界是紧邻的 `](`。**不能**只看 `(`——正文里
        // 「macOS 专属($19 买断)」这种括号会被当成链接，产出上百个假死链。
        if chars[i] == '(' && chars[i - 1] == ']' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != ')' {
                j += 1;
            }
            let target: String = chars[i + 1..j.min(chars.len())].iter().collect();
            let t = target.trim();
            if !t.is_empty() && !t.starts_with('#') && !t.contains("://") {
                out.insert(t.to_string());
            }
            i = j;
        }
        i += 1;
    }
    for part in md.split("src=\"").skip(1) {
        if let Some(endq) = part.find('"') {
            let t = part[..endq].trim();
            if !t.is_empty() && !t.contains("://") {
                out.insert(t.to_string());
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// ① 两段 `##` 标题序列结构相同
// ---------------------------------------------------------------------------
#[test]
fn readme_english_and_chinese_sections_have_the_same_structure() {
    let md = read("README.md");
    let (en, zh) = split_bilingual(&md);

    let h2 = |md: &str| -> Vec<String> {
        heading_shape(md)
            .into_iter()
            .filter(|(lvl, _)| *lvl == 2)
            .map(|(_, t)| t)
            .collect()
    };
    let (en_h2, zh_h2) = (h2(en), h2(zh));

    assert!(
        en_h2.len() >= 10,
        "English 段只抓到 {} 个 `##` 标题，分界规则多半坏了：{en_h2:?}",
        en_h2.len()
    );
    assert_eq!(
        en_h2.len(),
        zh_h2.len(),
        "两段的 `##` 数量不同（English {} / 中文 {}）—— 一端加了节另一端没加。\n\
         English: {en_h2:?}\n中文: {zh_h2:?}",
        en_h2.len(),
        zh_h2.len()
    );
    assert_eq!(
        sub_counts(en),
        sub_counts(zh),
        "两段每个 `##` 下的 `###` 数量不同：English {:?} / 中文 {:?}",
        sub_counts(en),
        sub_counts(zh)
    );
}

// ---------------------------------------------------------------------------
// ② 文档声明的配置键集合 == struct Config 的 serde 字段集合
// ---------------------------------------------------------------------------
#[test]
fn documented_config_keys_match_the_config_struct() {
    let fields = config_field_names();
    for rel in ["README.md", "docs/spec.md"] {
        let keys = declared_keys(&read(rel));
        assert!(
            !keys.is_empty(),
            "{rel} 里没抓到任何配置键——示例块/配置表可能被删了"
        );
        let missing: Vec<&String> = fields.difference(&keys).collect();
        let extra: Vec<&String> = keys.difference(&fields).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "{rel} 声明的配置键与 `struct Config` 的字段集合对不上：\n\
             缺 {} 个: {missing:?}\n多 {} 个: {extra:?}\n\
             （`Option<String>` 的两个字段在示例里是注释掉的，但仍必须出现——\
             未设置时整行不出现，不是空串；`update_last_checked` 由程序维护，\
             也要以注释形式列出）",
            missing.len(),
            extra.len()
        );
    }
}

// ---------------------------------------------------------------------------
// ③ 文档里每个相对链接真实存在
// ---------------------------------------------------------------------------
#[test]
fn relative_links_in_docs_resolve() {
    for rel in [
        "README.md",
        "docs/spec.md",
        "CHANGELOG.md",
        "CONTRIBUTING.md",
    ] {
        let md = read(rel);
        let base: PathBuf = Path::new(ROOT).join(Path::new(rel).parent().unwrap_or(Path::new(".")));
        let links = relative_links(&md);
        assert!(
            !links.is_empty(),
            "{rel} 里没抓到任何相对链接，解析多半坏了"
        );
        let dead: Vec<String> = links
            .iter()
            .filter(|l| {
                let path = l.split('#').next().unwrap_or(l);
                !path.is_empty() && !base.join(path).exists()
            })
            .cloned()
            .collect();
        assert!(
            dead.is_empty(),
            "{rel} 里有 {} 个死链: {dead:?}",
            dead.len()
        );
    }
}
