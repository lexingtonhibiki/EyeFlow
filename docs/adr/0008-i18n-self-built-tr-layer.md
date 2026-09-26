---
status: accepted
date: 2026-09-26
process: A/B 独立审查 + C 终极判决
parent: [0007-v0.6-scope.md](0007-v0.6-scope.md)
revised: 2026-09-26 三次裁决（C 方按 v0.6.1 实测推翻两条断言：`strong()` 是颜色操作不是空操作；`msyhl`/`simhei` 的「glyph 覆盖兜底」理由不成立）
---

# i18n 自研：build.rs 键完整性校验 + `tr(&str)` 函数 + Segoe UI 英文字体

## 决策

EyeFlow 的中英双语用**零新增运行时依赖**的自研方案实现，形态是「构建期校验 + 一个查表函数」，**不是** `enum Key` + 宏，也**不是** Fluent。

## 依据等级

沿用 [ADR-0007](0007-v0.6-scope.md) 的标注：**实测** / **源码** / **权威** / **推断** / **未验证**。**「未验证」的条目不得当作论据。**

## 数据流

```
locales/zh-CN.toml     ← 人类可编辑的纯数据，唯一事实来源（键的集合以它为准）
locales/en-US.toml           178 键 · 扁平 `key = "value"` 单行 · 无 [section] 头
        │
        │  build.rs（str::lines() 解析扁平格式）
        │  · en-US 缺 zh-CN 的任一键 → panic → 构建失败
        │  · en-US 多出 zh-CN 没有的键 → panic → 构建失败
        ▼
   include! 生成的静态表  ──►  tr(key: &str) -> &'static str
```

**实测落地形状**（`target/*/build/eyeflow-*/out/i18n.rs:4-6`）：

```rust
pub const KEY_COUNT: usize = 178;
pub static KEYS: [&str; KEY_COUNT] = [ /* 按 key 字典序 */ ];
pub static ZH_CN: [&str; KEY_COUNT] = [ /* 同序 */ ];
pub static EN_US: [&str; KEY_COUNT] = [ /* 同序 */ ];
```

`tr` 的实现是一次 `AtomicU8` 读取 + 一次 `binary_search_by`（178 键 ≈ 8 次比较）——**无解析、无堆分配**（无参路径）。工作区实测 181 处 `tr(` / `tr_fill(` / `trn(` 调用点。

> **口径更正**：初版本 ADR 写「约 152 键 / 152 处调用点」。**实测 178 键、181 处调用点**（152 是架构师按「用户可见文案」清点的估计，低估了托盘 `MenuId`、统计、设置项等非正文键）。以实测为准。

## 为什么不加 `toml` build-dependency

locale 文件是项目自己维护的扁平格式（`key = "value"` 单行、无转义、无 `[section]`——实测 `grep -cE '^\["' locales/zh-CN.toml` = 0），`build.rs` 用 `lines()` 加少量逻辑足够。加 `toml` 作为 build-dependency 会重复编译整个 toml crate 树——**为解析两个自己写的文件而引入一个 TOML 解析器，这层间接没有回报**。（**推断**，未做 `cargo build --timings` 对照测量；初版记为待测的第 5 条不确定项，本轮仍未测。）

## 为什么不保留 `enum Key` + `tr!` 宏 —— **采纳 B**

「编译期防 key 打字错」这条，`enum Key` 与 build.rs 的键集合校验**重复**——variant 由 key 生成，打错的 variant 根本不存在。真正差异化、且所有 crate 都给不了的是**「缺任何语言的 key 直接让构建失败」**。砍掉宏的额外收益：

- `FlowSensitivity::label()`、`SoundPreset::label()`、`WallpaperFit::label()`、`ContextState::label()` **整体搬进 locale 表**。这样 `config.rs` 与 `core.rs` 保持纯数据、**零测试污染**。
  - **实测复核**：`grep -n "fn label" src/config.rs` 在工作区**零命中**，四个枚举已只给 locale 的 key。
- 若 `label()` 内部读 `AtomicU8` 语言状态，`config.rs` 的 15 个测试与 `core.rs` 的 14 个测试会变成**顺序相关**：一个把语言设成 `EnUs` 的测试会污染同进程后续测试。搬走之后这些测试**零改动**。
- 加第三种语言时**永不触碰领域层**。
- 基础设施约 200 行 → 约 70 行。

## 为什么不选 crate

| 方案 | 否决理由 |
|---|---|
| Fluent / `i18n-embed` | 运行时解析 FTL（漏 key 直接在界面上显示 `"tray.open_setting"`），+4 crate、+400~800 KB（+5~8%）。**Fluent 的核心资产是「语言 × 变体（复数/性别/格）」的组合爆炸；这里是 1 产品 × 2 语言 × 二值复数。为 6 KB 文案引入一套需要学的语法，是把学习成本一次性付清换一个永远用不上的能力。** |
| `rust-i18n` | 体积 +250~400 KB。**必须诚实记账：第二条理由「依赖已被归档的 `serde_yaml`」在 A/B/C 三方全程无网络的情况下未能验证**（标记：未验证）。另一条常见理由「翻译要改 Rust 文件」在这个项目**不存在**——只有一人维护，翻译者就是作者本人。**结论仍是否决，但成立的理由只有体积一条。** |
| `i18n-embed-fl` | **无法确认该 crate 是否存在**（标记：未验证）。 |

决定性理由是**比例**：本项目第一条差异化卖点是体积（`opt-level="z"` + `lto` + `codegen-units=1` + `strip` 是有意识的取舍，README 逐字宣传），为一个 6 KB 的文案问题接受 +5~8% 二进制不成比例。

判据（来自原计划、予以保留）：**迁移成本 = 重写 `locales/*.toml`，代码调用点零改动**。这句话把方案从「200 行负债」变成「200 行可弃的基础设施」。

> **讽刺性注脚（来自 A 方，已复核成立）**：本项目以「体积是第一约束」为由否决了 +250~400 KB 的 `rust-i18n`，同时却让一个**默认关闭**的 `ureq` 把 rustls + ring 无条件拖进二进制 400+ KB 量级。判决与落地见 [ADR-0007 判决五](0007-v0.6-scope.md)（实测 -1,139,200 B / -11.14%）。**「第一约束」应当一致地适用于自己的依赖树，而不只适用于要引入的新依赖。**

## 英文模式的字体：Segoe UI，不是 egui 内置字体 —— **结论采纳 B，论据按源码更正**

**依据：源码。**

`app.rs:514-553` 的做法是按语言分叉：中文模式无条件读 `C:\Windows\Fonts\msyh.ttc`（实测 19,704,352 B = **18.79 MiB**），英文模式只读 Segoe UI。

原计划 M3 说「英文模式跳过 CJK 加载即可 -18.8 MB」。这在**数值上对**。

**决策**：英文模式字体链 = `segoeui.ttf` + `seguisym.ttf` 回退（`app.rs:550-553`），**不加粗体字体文件**。

- 保留 18.6 MB / 18.8 MB ≈ **99%** 的宣称收益（仅英文模式成立）
- ClearType hinting，是 Windows 自己用的字族 → 英文界面看起来是「原生 Windows 应用」（**推断**，无渲染实测）
- Vista 起每台机器都有；约 10 行代码；零构建期子集化的漏字风险；零运行时报错路径

### 实施阶段被推翻的裁决（本 ADR 的核心修订）

**本 ADR 初版写的是「加载 `segoeui.ttf` + `segoeuib.ttf`，有真粗体 → 字重阶梯在英文模式下成立」。该前提经源码复核为假。**

- `epaint 0.36.1` 的 `FontId` 只有 `{ size, family }` 两个字段（`epaint-0.36.1/src/text/fonts.rs:27-34`，源码里明写 `// TODO(emilk): weight (bold), italics, …`），**全文件 `FontWeight` 出现 0 次**。
- **级联复核**：`egui 0.36.1` 的 `src/` 中 `FontWeight` 同样出现 **0 次**——不是 epaint 一层的问题，**整条栈都不认识字重**。
- `FontFamily` 的列表被当作**逐字符的 glyph 回退链**（`app.rs:583-584` 的注释描述正确），不是「常规体 + 粗体」这一对。

**两个直接后果**：

1. ~~**`RichText::strong()` 在中英文下都是空操作**——不只是英文模式，**v0.5.2 的中文界面从来没在加粗**。~~ **⚠️ 此句已于 2026-09-26 三次裁决被源码证伪，见下方「推翻 2」。`strong()` 改的是颜色不是字重，中英文下都生效——标题的纯白(255) vs 正文的灰(140) 层级一直成立。被浪费的只有字号那条通道。** 字重阶梯确实在两种语言下都不可用——**这半句仍然成立**。
2. **`segoeuib.ttf` 不会带来任何字重**，只会作为 `segoeui.ttf` 缺字时的第二道回退——947 KB 换零收益。**已从 `EN_FONTS` 移除**（`app.rs:550-553` 实测只有 2 个 `FontFile`）。

**代码改动裁定：保留，不回退。** 依据即上述三条源码证据；`app.rs:536-549` 的注释已把结论与后果写清楚。

**连带修正 [ADR-0007 判决九](0007-v0.6-scope.md)** 里「theme.rs / 6 级字重阶梯推 v0.7，理由是地基不存在」：原措辞暗示地基将来会就绪。**实际是 epaint 上游不支持 `FontWeight`，地基在可预见的未来不会就绪**——v0.7 的排版体系必须**完全绕开字重**，只用字号 / 颜色 / 间距建立层级。

**约束**：中文模式是否也加载雅黑粗体（`msyhb.ttc`）——**无需再议**，加载它买不到字重（同上）。~~`ZH_FONTS` 里的 `msyhl.ttc` / `simhei.ttf` 保留，但它们的理由是**glyph 覆盖兜底**而非字重。~~ **⚠️ 这个理由同样已被实测证伪，见下方「推翻 3」——两者对现有文案贡献 0 个字符，v0.6.1 已把它们从字体链移除。**

**否决**：构建期字体子集化（18.8 MB → 约 0.1 MB）。理由是**漏字在构建期嵌入后运行时无法补救**，而缺字恰是 i18n 最怕的 bug。也否决换 `msyhl.ttc`（省 7.2 MB）：Light 字重在无法加粗的前提下会毁掉标题层级。 **（三次裁决补注：这条否决的**结论**仍然成立——正因为无法加粗才不能只留 Light 字重。但它已经不再是「留着 `msyhl` 的理由」，因为真正有字重需求的那条路（`msyhb.ttc`）从未被采纳。）**

### 推翻 2：「`RichText::strong()` 是空操作」 —— **源码证伪，它改的是颜色**

**依据（源码，C 方逐行复核）**：

1. `egui-0.36.1/src/widget_text.rs:252`：`pub fn strong()` 置 `self.strong = true`。
2. `egui-0.36.1/src/widget_text.rs:484-485`：`} else if self.strong { Some(visuals.strong_text_color()) }` —— **它参与取色**。
3. `egui-0.36.1/src/style.rs:1146-1148`：`strong_text_color() = self.widgets.active.text_color()`。
4. `egui-0.36.1/src/style.rs:1710`（dark）`active.fg_stroke = Stroke::new(2.0, Color32::WHITE)`；`:1686`（dark）`noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_gray(140))`。

**后果**：

- **标题的层级早就成立了**（纯白 vs 灰 140，亮度差 82%）。原文「一直没在加粗，只是没人注意」漏掉了一个已经在生效的通道。
- `ui.rs:149` 的卡片标题 14.0 px 与项目自己在 `app.rs:319` / `app.rs:378` 用的 17.0 px 不一致——**这条不一致是真的，但性质是「补第二条通道」而不是「救一个看不见的标题」**。v0.6.1 把 `ui.rs:149` 对齐到 17.0。
- **必须同步更正的生产注释**：`src/app.rs:544` 那句「`RichText::strong()` 在中英文下**都是空操作**」是同一条错误断言的代码副本，**它已经扩散进源码注释**，本轮一并改。
- **本 ADR 不翻回的半句**：「epaint 0.36.1 不支持 `FontWeight`」仍然成立，字重阶梯仍是 v0.7 议题。

### 推翻 3：「`msyhl.ttc` / `simhei.ttf` 的理由是 glyph 覆盖兜底」 —— **fontTools 实测证伪，两者贡献 0 个字符**

**依据（实测，C 方本轮亲跑 fontTools 直读系统字体 cmap）**：

- 现有 `locales/*.toml` 全部去重字符 **508** 个。
- **「需要 `msyhl` 或 `simhei` 才画得出」的字符数 = 0。** 换句话说，把 `ZH_FONTS` 缩短成 `msyh.ttc` + `seguisym.ttf` 之后，**一个字都不会变成豆腐块**。
- BMP 层面 `(msyhl ∪ simhei) − (msyh ∪ seguisym)` 只有 **29** 个码位：**25 个在私有区 PUA（U+E78D…U+E864，本应用不可能用到）+ 4 个 `ﬁ` `ﬂ` `﴾` `﴿`（egui 的 harfrust 自己做拉丁连字，不会去字体要预连字码位）**。非 BMP 额外码位 **0**。
- CJK 统一表意文字区 U+4E00–U+9FFF：`msyh` **20992/20992**；`simhei` **20902/20992**——**`simhei` 比 `msyh` 还少 90 个码位，从来就不是超集**。
- **反向确认 `seguisym.ttf` 必须保留**：文案里的 `▶`(U+25B6) 与 `✓`(U+2713) **只有它**有 glyph。

**后果（v0.6.1 落地）**：`msyhl.ttc` + `simhei.ttf` 共 **21,935,404 B = 20.92 MiB** 从字体链移除，中文字体链 **44,153,812 B → 22,218,408 B（−49.68%）**。因 egui 内部把每份字体字节存两份（`ContextImpl.font_definitions` 的 `Arc<FontData>` + `FontFace.blob` 的 `Arc<Vec<u8>>`），**会话内峰值杠杆约 2 份 ≈ 43.9 MB**——与语言对比实测的峰值差 74.2 MB 自洽（英文链 3.3 MB vs 中文链 42.1 MB，×2 = 77 MB，预测 77 实测 74.2）。

**⚠️ 常驻内存的杠杆接近零**（用语言实验的传递率推算约 1.3 MB，落在噪声带内）。**CHANGELOG 里一个字都不许提「降低常驻内存」。**

**⚠️ 本仓库的测试抓不住这个改动**（A 方指出，C 方复核确认）：

- `tests/i18n_glyphs.rs:25` 的 `const ZH_FONTS: &[&str] = &["msyh.ttc", "msyhl.ttc", "simhei.ttf"];` 是一份**手抄字面量，不从 `src/app.rs` 解析**。改 `app.rs` 不会改它。
- T11a / T11b 的覆盖判据是**整条链的并集**（`loaded.iter().any(...)`），所以即使把 `app.rs` 的链砍到只剩 `msyh`，只要测试里的字面量没同步，它仍然全绿。
- T11d（`:358-374`）用 `src.contains("msyh.ttc")` 扫 `src/app.rs` **全文**——**注释里出现这个字符串就算通过**。实测 `segoeuib.ttf` 早已从 `EN_FONTS` 移除，T11d 却仍然绿，因为它命中的是 `app.rs:545` 的**注释**。
- **所以 v0.6.1 必须加一条新断言**，把「测试里的字体链」与「`src/app.rs` 里的字体链」绑死（见实施清单）。**这是本 ADR 最重要的一条实施约束。**


## 字形覆盖：`seguisym.ttf` 尾部回退 —— **本轮修复的真实 bug**

**实测**：`✓`(U+2713) 与 `▶`(U+25B6) 在 `msyh.ttc` / `msyhl.ttc` / `simhei.ttf` **全部没有 glyph**（`docs/lessons.md:70-74` 已记录该教训）。egui 没有系统级字体回退，缺字就是方块。**也就是说 v0.5.2 的中文界面今天就在显示豆腐块，并且已经发版五个版本了。**

两条链都以 `seguisym.ttf` 收尾（`app.rs:528,552`），因为实测 `✓` / `▶` 在 `msyh` / `msyhl` / `simhei` / `segoeui` 里同样没有 glyph。emoji（👀 👏）由 egui `default_fonts` 自带的 emoji 字体承担，**不可关闭 `default_fonts`**（`app.rs:555-559`）。

**保留 T11（字形覆盖测试，A 方提出）**：`tests/i18n_glyphs.rs` 实测 5 个用例全绿（`t11a` locale 非 ASCII 逐字覆盖、`t11b` 已知风险符号逐语言覆盖、`t11c` 风险 emoji 由系统符号字体覆盖、`probe_coverage` 诊断）。**这是「按语言换字体集」设计下真正的失效模式**——T5「英文表不含 CJK」是字符串表的性质，与字形覆盖无关。全套测试里原本没有一条覆盖它。

## 默认语言与配置兼容 —— **采纳 A 方案 9 + B 方案 9 的折中**

**依据：源码 + 实测（74 测试含 T8 系列全绿）。**

- `Config` 加 `#[serde(default = "d_language")] language: String`（`config.rs:235-236`），`d_language()` 返回 `"zh-CN"`。
- **现有用户的 `config.toml` 走 serde default，行为与 v0.5.2 完全一致**；不触发 `migrate_legacy`（判据是「没有 `short_break_min_secs`」，加字段不影响）；不触发解析失败备份重建。
- `Config` **没有** `deny_unknown_fields` ⇒ 新版本写入的 `language` 字段被 v0.5.2 读到时**静默忽略**，**降级也安全**。
- **非法值不得静默切成英文并落盘**（A 方案 9 指出的真实缺陷）：**`en` 前缀（大小写不敏感）→ 英文；其余一切 → `zh-CN`；且 `sanitized()` 对无法识别的原值不写回**（`config.rs:402-414`）。理由：README 明确鼓励手改 config，拼错一个字母导致界面语言静默变英文且无法自动恢复，是对「不动用户个人配置」硬约束的违反。
  - **口径修订**：初版写的是严格白名单 `en` / `en-US` / `en_US`。改为 `en` 前缀匹配后同时满足两侧：`en-GB` / `en-AU` 得英文（用户显然要英文），`chinese` / `klingon` / `zh_CN` 得中文。唯一残留风险是 `eno` 这类笔误拿到英文——**但该错误在界面上是可见的、且能改回，不构成静默数据丢失**，与原缺陷不同质。
- **首次运行读 `GetUserDefaultLocaleName()`（采纳 B 方案 9，A 方案 9 反对）**：`windows` crate 的 `Win32_Globalization` feature 已在依赖里，加约 5 行（`main.rs:61-77,214-222`）。**验收面没有新增**——首跑设置窗（`main.rs:118-119`）已经无条件自动打开，里面就有语言选择器，「读一个配置值当初值」这个动作的验收面 = 现有 first-run 的验收面。落盘后永久定型，不再分叉。
  - 现有用户（config.toml 已存在）**恒 `zh-CN`**，不受影响。
- 语言选择器放在**设置窗「系统」卡片首行**，且首次运行时是该窗第一行第一项——英文母语用户装上第一眼看到中文（**托盘右键菜单也是 7 项中文，而托盘是这个应用唯一的高频入口**），必须一步可达。

## 运行时切换的刷新面

| 表面 | 机制 |
|---|---|
| egui 视口内容（设置窗 / 预告浮窗 / 休息面板） | 每帧从 `tr()` 现取，`ctx.request_repaint()` 即可，**无需重建视口** |
| 字体 | 英文模式跳过 CJK 加载，切回中文时装载；`app.rs:250` 在 `App::logic` 里比对 `tr::language()`，变了就 `ctx.set_fonts()` |
| 原生窗口标题 | **未验证**——egui 在只改 title 时是否下发 `ViewportCommand::Title`，全程未实测 |
| 托盘菜单 + tooltip | 逐项 `set_text`。**红线：`tray.rs` 的 `MenuId` 常量必须永久保持 ASCII 标识符、永不翻译**——否则 `tray.rs:146` 的 match 在切语言后全失配、托盘彻底失效。**现状已满足**，`tests/i18n_tray_ids.rs` 的 `t9b` / `t9c` 钉死这条 |

**落地前置**：`Tray` 需把 `open_item` / `quit_item` 提升为字段，`apply_language` 才能改「打开设置」和「退出」这两项。**已落地**，`tests/i18n_tray_ids.rs` 的 `t13_tray_has_a_language_apply_path_that_rewrites_all_items` 覆盖。

## 参数化与复数

- 占位符 `{name}`。**不引入 ICU MessageFormat**——1 产品 × 2 语言 × 二值复数，`|` 分隔足够。
- 复数约定：`"1 break"` vs `"{n} breaks"`，`n == 1 ? 0 : 1`（`tr.rs` 的 `trn()`）。
- **时间单位不是复数**，单独 key。
- 已知局限：无法覆盖 ru/pl 的 6 形态复数。因为约定在**数据层而非代码层**，迁移 = 重写 `locales/*.toml`，调用点零改动。

### 已修的三处翻译形态缺陷

**（a）三计数托盘行**（B 方案 5，**成立**）

原 `"今日休息 {} 次 · 跳过 {} · 延后 {}"` 一个 key 里三个计数，`|` 二元约定给的是「整个字符串两种形式」，**无法表达 per-placeholder 复数**。中文里 `次` 是可省量词所以看不出来，英文这一行需要 **3 个独立复数决策**。这一行在托盘 tooltip 里，用户天天看。→ **已拆成单数 key 后拼装**：`tr.rs` 的 `tray_stats_line()`，`runtime.rs:293` 调用。

**（b）`FlowSensitivity` 的 ComboBox 长度预算**（B 方案 6，**成立**）

原 `"低（30 秒内持续输入 > 40 键）"` 是「短名字 + 括号里的规则」被塞进一个宽度受行宽约束的 `ComboBox::selected_text`，而它是用户打开下拉框之前唯一能看到的那个值。英文直译 `"Low (more than 40 keys within 30 s of continuous typing)"` = 45 字符，在 700 px 窗口里要么撑爆布局要么被省略号截断。→ **已拆成短名（`"低"` / `"Low"`）+ 独立的 `on_hover_text` 说明。** `SoundPreset` / `WallpaperFit` 同样是 ComboBox 内容，程度轻得多，但长度预算同样存在。

**（c）`human_duration` 的 `m == 0`**（B 方案 7，**成立**）

`"{n} 小时 {m} 分"` → `"{n} h {m} min"`，`m == 0` 时会输出 `"2 h 0 min"`。→ **已加 `m > 0` 分支**（`ui.rs:787`），2 小时整点输出 `"2 h"`。`tests/i18n_locales.rs` 的 `t12a_english_never_emits_a_zero_minute_form` 钉死。

## 风险集中在约 15 条文案上，不是均匀分布

178 键里，**约 15 条承担了 80% 的风险**，其余翻错了也只是难看。必须逐条手工在英文模式下目视验收的是：

- 托盘三计数行（复数）
- `FlowSensitivity` ComboBox（长度预算）
- `human_duration`（`m == 0` 分支）
- `reminder_line` 五个变体 + 「（其中自然休息 {}）· 跳过」——都是「中文里不像复数、英文里必须复数」的句子，**原计划按「复数点 3 处」计数漏掉了这些**
- 托盘「暂停 1 小时」——桌面/菜单语境下中文用数字、英文惯例用单词（`Pause for one hour`），直译会显得生硬
- 10 处单位后缀：**这条其实安全**——中文「5 分钟」与英文「5 min」顺序一致，`{n} min` 直接可用，`DragValue` 的 suffix 机制保证数字由 egui 渲染

## 测试策略：只保留确定性的，砍掉启发式的

**保留**（`tests/i18n_locales.rs` 10 + `i18n_glyphs.rs` 5 + `i18n_tray_ids.rs` 3 = **18 个集成测试，实测全绿**）

| 测试 | 断言 |
|---|---|
| T1 键对齐 | 两个语言的键集合完全相同，且非空（`t1`） |
| T1b 键格式 | 键是小写点分（`t1b`） |
| T1c 生成表覆盖 | 生成的静态表覆盖每个源键（`t1c`） |
| T2 占位符对齐 | 逐 key 提取 `{name}` 集合。**注意：不是「完全相同」**——英文单数形式里写死了 `1` 没有 `{n}`，所以规则是「英文允许是中文的子集，但不得出现中文没有的占位符名」（`t2`）；占位符本身格式良好（`t2b`） |
| T3 无空串 | 任一语言的任一键不得为空串或纯空白 |
| T7 运行时查表 | 每键在两种语言下都返回非 `key` 本身的值（防 key 拼错返回原样，`t7a`） |
| T8 老配置兼容 | 无 `language` 字段的 config.toml 解析后为 `zh-CN`；非法值回落 `zh-CN` 且 `sanitized()` 不写回（`src/config.rs` 内） |
| T9 托盘文案 | 托盘 7 项菜单文案在两种语言下均非空（`t9a`）；`MenuId` 仍为 ASCII（`t9b`）、永不被翻译（`t9c`） |
| T11 字形覆盖 | 每条文案里的每个非 ASCII 字符，都能被该语言实际加载的字体集渲染（`t11a` / `t11b` / `t11c`） |
| T12 零分钟形态 | 英文不输出 `"2 h 0 min"` 这类形态 |
| T13 语言应用路径 | 托盘存在改写全部菜单项文案的 `apply_language` 路径 |
| T14 状态条「连续坚持」 | `stats.streak_label` 在 n = 0 / 1 / 3 下三种读法都成立，且不出现 `-day` 复合名词（`src/config.rs` 内） |
| T15 英文真的被翻了 | 除一张 6 条的显式白名单（品牌名 / 纯符号 / 纯单位 / 专有名词，每条带理由）外，`en[k] != zh[k]`；白名单长度不得超过键数的 10% |

**T15 为什么必须存在（2026-09-26 补记）**：T7a 只拦住「值被写成 key 本身」这一种粘贴事故，它给的是**虚假的安全感**——真正的失败模式是**整段中文原样留在英文表**。T7b 有一点跨语言比较，但比的是「**至少有一个键不同**」这个布尔值（`differs_somewhere`），不报是哪几个键。

实测的伪失败验证（T15 落地前）：

- **局部漏翻**（把 `tray.toggle_enabled` 单独改成中文，其余 177 条保持英文）——`T1 / T1b / T1c / T2 / T2b / T3 / T7a / T9a / T12a` **全部照样绿**，整个 i18n 数据层无人报错。**这才是译者真正会犯的错，也是 T15 的正当理由。**
- **整份替换**（`en-US.toml` 整份换成 `zh-CN.toml` 的内容）——**并非全绿**：`T7b` 红（`differs_somewhere` 变 false）、`T9a` 红（英文 `stats.*` 带 `|` 的 key 数为 0 < 3）。但那是**顺带**抓到的：两条抓的是「两表全等」与「复数约定」，不是「漏翻」；只有把整张表搬过去才会触发。

T15 是这一种失败模式的前哨；白名单的作用是让「本来就该相同」的键（`EyeFlow` / `●` / ` · ` / ` %` / `GitHub API`）不必被逐个豁免，同时用 10% 的上限防止白名单变肥。现状：178 键中 6 键相同（3.4%），全部落在白名单内。

**砍掉**

| 测试 | 理由 |
|---|---|
| T4 正则扫 `src/**/*.rs` 找裸中文字面量 | 要可靠排除注释、`log::!` 跨行调用、`#[cfg(test)]` 块、行内注释，基于行的正则做不到，只能写一个粗糙的 Rust 词法扫描器。会持续误报、持续需要打补丁。**更严重的是：第一次误报就会有人去「翻译注释让测试变绿」，直接威胁用户拍板的「CHANGELOG / ADR / 决策记录 / 内部注释保留中文」。** 豁免必须在解析层做，不能靠字符串启发式——那就意味着要写词法分析器，成本远超收益（A、B 双方独立指出，B 方案 12 成立） |
| T5「英文表不含 CJK」 | 性质错误（见 T11） |
| T6「中文表不得出现 3 个连续 ASCII 单词」 | 太脆：`Ctrl+Shift+E`、`wav`、`mp3`、`AOA` 全要白名单，等于要维护例外表 |
| T10 的版本号 / exe 体积 / 测试计数三条断言 | 把「最容易变的三个数字」绑进「必须精确一致」，会在最高频动作上收税；且为让体积断言变绿而向代码里塞一个假常量，是把文档问题转化成代码负债 |

**T11 优先于 T4。** 若 T11 与 T4 只能活一个，活 T11。

**架构师清单漂移的证据**：原计划称全角空格 17 处、列了 17 个行号，实测 **14 处**（本轮改造后 `src/*.rs` 剩余 **0 处**），其中 7 个行号根本不存在、6 处实际存在的漏列。这正是 T4 本该当场抓出来的——**测试还不存在，清单已经开始漂移**（A 方案 7 的数字成立；其「切英文会错位」的触发条件不成立，见下）。

## README 双语

**单文件、两次完整副本**（English 段在前 + 中文段在后），徽章/截图/对照表这类语言中性内容**只出现一次**。

放弃 `.github/README.en.md` 双文件：GitHub 不自动切语言，且两文件在 Issues / PR / 搜索上完全割裂。

**防漂移测试只保留三条**（`tests/readme_bilingual.rs` 实测 3 个全绿）：

1. 两段 `##` 标题序列结构相同（抓住「一端加了节另一端没加」）
2. 两段 ```toml 块的 key 集合 == `config.rs` 中 `struct Config` 的 serde 字段集合 —— **价值最高的一条**，它本就能当场抓出 README 缺 4 个字段、以及 `custom_sound_path = ""` 与 `Option<String>` 不一致。**注意限定在 `struct Config` 块内**，否则会把 `FlowSensitivity` / `SoundPreset` / `WallpaperFit` 的字段也抓进来
3. 文档里的每个相对链接真实存在（死链是唯一用户会立刻发现的文档 bug）

**并同时校验 `docs/spec.md`**——原计划 §2.9 声称「`docs/spec.md` 保持唯一事实来源」，那防漂移测试就必须覆盖它，否则那句话与测试设计自相矛盾。

## 决策的已知代价

1. **i18n 是长期承诺。** 每次改文案要改两个文件。判据「迁移成本 = 重写 `locales/*.toml`，调用点零改动」覆盖架构迁移，不覆盖日常维护。可接受——它就是双语项目本身。
2. **`|` 二元复数约定表达不了 per-placeholder 复数。** 已用「拆成单数 key 后拼装」绕过（`tray_stats_line`），代价是键数量增加与调用点略啰嗦。
3. **178 键 × 2 语言 = 约 30 KB 纯数据**进二进制。按项目自己的体积标准（拒绝 +250~400 KB 的 rust-i18n）这属于可接受量级，但**它换来的是 0 KB 运行时依赖**——自研方案真正的成本在维护，不在体积。
4. **本 ADR 的两条核心论据（rust-i18n 的 `serde_yaml` 依赖、`i18n-embed-fl` 是否存在）全程未验证**（无网络）。**结论仍是否决，但只剩体积一条理由成立。**

## Sources

- **本轮源码复核**（取代初版引用的 0.31.1 路径）：`epaint-0.36.1/src/text/fonts.rs:27-34`（`FontId` 仅 `{size, family}`，含 `// TODO(emilk): weight (bold), italics, …`）、`:527-529`（默认 Proportional 仅 `Ubuntu-Light`）；`grep -rn "FontWeight"` 在 `epaint-0.36.1/` 与 `egui-0.36.1/src/` 均**零命中**
- **本轮实测**：`msyh.ttc` = 19,704,352 B（18.79 MiB）；`locales/*.toml` 各 178 键、扁平无 section；生成的 `KEY_COUNT = 178`；181 处 `tr` 调用点；`clippy --bins` 零警告；74 个测试双工具链全绿
- `GetUserDefaultLocaleName`: https://learn.microsoft.com/en-us/windows/win32/api/localeapi/nf-localeapi-getuserdefaultlocalename
- serde 容器属性与 `deny_unknown_fields` 缺省行为: https://serde.rs/container-attrs.html
- Fluent 复数/性别/格模型: https://projectfluent.org/fluent/guide/plurals.html
- dtolnay/serde-yaml 归档: https://github.com/dtolnay/serde-yaml —— **rust-i18n 当前版本是否仍依赖它，全程无网络，未验证**
- `docs/lessons.md:70-74`：字形覆盖必须实测 cmap，不能靠「这个字体是中文字体所以应该有」
