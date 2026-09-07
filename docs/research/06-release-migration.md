# EyeFlow 迁移正式开源仓库 — 发布与迁移调研

> 调研日期:2026-09-08 · 范围:本地盘点(E:\Workspaces\Hanako\eyeflow、E:\Projects\MyGitHub)+ 网络调研
> 结论先行:**新建 `E:\Projects\MyGitHub\eyeflow`,复制 src/Cargo.toml/Cargo.lock/docs/installer,排除全部 target 目录(约 648MB)与编译产物,MIT 协议,tag 触发的 GitHub Actions 自动构建 NSIS 安装包并发布到 Releases;渠道按 Releases → scoop 自有 bucket → winget PR 的低成本顺序推进。**

---

## ① 迁移文件清单

### 1.1 原样复制(进新仓库)

| 路径 | 说明 |
|---|---|
| `src/*.rs` | 全部 9 个模块(audio/config/detector/event/main/reminder/state/tray/ui,约 52KB),核心资产 |
| `Cargo.toml` | 见 1.3 改写项(补 license/repository 字段) |
| `Cargo.lock` | **应提交**。bin crate 惯例 + 官方口径:cargo new 默认跟踪;"When in doubt, check Cargo.lock into the version control system";配合 CI `cargo build --locked` 保证可复现构建(来源见⑥-Cargo Book) |
| `docs/spec.md` | 设计文档,保留(可视为 ADR/设计说明) |
| `installer/installer.nsi` | NSIS 脚本,见 1.3(版本号解耦) |

### 1.2 排除(不进新仓库)

| 路径 | 大小 | 理由 |
|---|---|---|
| `target/` | 496MB | cargo 构建缓存 |
| `target_fresh/` | 75MB | 同上(历史实验残留) |
| `target_fresh2/` | 76MB | 同上 |
| `target6/` | 872KB | 同上 |
| `installer/EyeFlow-Setup.exe` | 741KB | 编译产物,由 CI 生成,Release 页托管 |
| `installer/eyeflow.exe` | 1.37MB | 同上 |
| `tasks.md` | — | 个人开发任务清单(含大量未完成任务与内部口吻),不适宜公开;可留在私人笔记 |
| `build-check.bat` | — | 硬编码 `D:\DevTools\SDK\Rust\cargo\bin\cargo.exe` 本机绝对路径,无通用价值 |
| `install-rust.ps1` | — | 面向本机 D:\ 盘的个人 Rust 安装脚本;公开仓库写 README 指向官方 rustup 即可 |
| `INSTALL.md` | — | 内容有价值但属"安装+构建+profile 说明"混合体;拆入新 README 的 Build/Install 章节,不单独迁移 |

target 目录合计约 **648MB**,占当前 eyeflow 目录(649MB)的 **99.8%** —— 迁移时只按白名单复制,切勿整目录拷贝。

### 1.3 改写

| 文件 | 改写要点 |
|---|---|
| `README.md` | 现在只有 5 行。重写为正式开源 README(大纲见②) |
| `Cargo.toml` | 补 `license = "MIT"`、`repository = "https://github.com/<你>/eyeflow"`、`rust-version`(MSRV);description 建议中英双语 |
| `installer/installer.nsi` | `PRODUCT_VERSION "0.1.0"` 目前硬编码,与 Cargo.toml 双写易漂移。改为 `!ifndef PRODUCT_VERSION / !endif` 默认值,CI 用 `makensis /DPRODUCT_VERSION=...` 注入;`PRODUCT_PUBLISHER "EyeFlow Team"` 改为真实作者名 |
| `build-release.cmd` | 可保留为本地一键构建(内容已可移植,仅依赖 PATH 中的 cargo 与 NSIS 默认路径);低优先级,CI 建成后甚至可删 |

---

## ② 新仓库骨架建议

### 2.1 目录树

```
E:\Projects\MyGitHub\eyeflow\
├── .github/
│   └── workflows/
│       ├── ci.yml          # push/PR: cargo check + clippy + build(可选但推荐)
│       └── release.yml     # tag 触发,见③
├── src/                    # 9 个 .rs,原样
├── docs/
│   └── spec.md             # 原样;私有 research/ 笔记不建议公开
├── installer/
│   └── installer.nsi
├── assets/                 # 新增:icon.ico(NSIS DisplayIcon 需要)、README 截图
│   └── screenshots/
├── .gitignore
├── Cargo.toml
├── Cargo.lock
├── LICENSE                 # MIT
├── README.md               # 重写
└── CHANGELOG.md            # 建议,Keep a Changelog 格式,从 0.1.0 起记
```

### 2.2 README 大纲(参照 stretchly / espanso)

1. **标题 + 一句话简介**(中英双语,如 "EyeFlow — context-aware eye-care reminder for Windows")
2. **Badges**:CI 状态(workflow badge)、License MIT、Latest release 版本、`platform-windows-blue`
3. **截图/GIF**:托盘菜单 + 提醒弹窗各一张(GIF 演示更佳,espanso/stretchly 均放动图;PNG 亦可)
4. **Features**:四状态机、全屏/空闲检测、加权随机提醒、5 种合成提示音、免打扰
5. **Download/Install**:GitHub Releases(Setup.exe / 便携 zip)为主入口;后续补 winget / scoop 命令
6. **Build from source**:前置(rustup stable + MSVC Build Tools)→ `cargo build --release` → 可选 NSIS 出安装包;产物约 3MB(strip+LTO+panic=abort)
7. **Configuration**:`%APPDATA%\eyeflow\config.toml` 各项说明;卸载行为说明(现 INSTALL.md 里的卸载清单移到这里)
8. **Roadmap / CHANGELOG 链接**
9. **License** 一行

CONTRIBUTING.md 对单人维护的小工具非必需(stretchly 有、espanso README 中无独立文件);若 MyGitHub 风格参照(GenericAgent、agentic-engineering-framework 都有),可加一份 5 行简版("先开 Issue 再提 PR")。

### 2.3 LICENSE 建议:MIT

- MIT 是小工具事实标准:一句"short and simple permissive license",允许商用/闭源衍生,唯一条件是保留版权声明,零摩擦最大化采用(来源:choosealicense.com)。
- GPL-3.0 仅当你想强制衍生品也开源时选(espanso 选了 GPL-3.0);stretchly 用更宽松的 BSD-2。护眼工具无核心库护城河诉求,GPL 只会降低被收录/分发的意愿。
- 与你 MyGitHub 现有习惯一致:GenericAgent、agentic-engineering-framework 均为 MIT。
- 文本:`Copyright (c) 2026 <你的名字>`;同时 Cargo.toml 写 `license = "MIT"`。

### 2.4 .gitignore

```gitignore
# Rust 构建产物
/target/
# 历史 target 目录(本地清理前防御)
/target6/
/target_fresh/
/target_fresh2/

# 编译产物与安装包(一律走 CI + Releases)
*.exe
*.msi
installer/*.exe

# 系统与编辑器
Thumbs.db
.DS_Store
.idea/
.vscode/
*.rs.bk
```

---

## ③ GitHub Actions workflow 要点

参照 ripgrep 官方 release.yml 的成熟结构(tag 触发 + 版本校验 + 独立发布 job),针对单平台 Windows 小工具简化:`dtolnay/rust-toolchain` 固定 stable、`Swatinem/rust-cache` 缓存、`--locked` 配合已提交的 Cargo.lock、choco 装 NSIS、产物 + SHA256 一并传 Release。

`.github/workflows/release.yml`(可直接使用):

```yaml
name: Release

on:
  push:
    tags: ["v[0-9]+.[0-9]+.[0-9]+"]

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: Verify tag matches Cargo.toml version
        shell: pwsh
        run: |
          $tag = "${{ github.ref_name }}" -replace '^v', ''
          $manifest = (Get-Content Cargo.toml -Raw) -match '(?m)^version\s*=\s*"([^"]+)"'
          $cargoVer = $Matches[1]
          if ($tag -ne $cargoVer) {
            throw "tag v$tag != Cargo.toml $cargoVer"
          }

      - name: Build release binary
        run: cargo build --release --locked

      - name: Install NSIS
        run: choco install nsis -y --no-progress

      - name: Build installer
        shell: cmd
        run: makensis /DPRODUCT_VERSION=${{ github.ref_name }} installer\installer.nsi
        # 需将 installer.nsi 的版本行改为:
        #   !ifndef PRODUCT_VERSION
        #     !define PRODUCT_VERSION "0.1.0"
        #   !endif
        # 并把 OutFile 改为 OutFile "..\EyeFlow-${PRODUCT_VERSION}-Setup.exe" 或统一产物目录

      - name: Make portable zip + SHA256
        shell: pwsh
        run: |
          $v = "${{ github.ref_name }}"
          Copy-Item target/release/eyeflow.exe "eyeflow-$v-x86_64-portable.exe"
          Compress-Archive -Path "eyeflow-$v-x86_64-portable.exe" -DestinationPath "eyeflow-$v-x86_64-portable.zip"
          (Get-FileHash "EyeFlow-$($v.TrimStart('v'))-Setup.exe" -Algorithm SHA256).Hash |
            Out-File "EyeFlow-$($v.TrimStart('v'))-Setup.exe.sha256" -Encoding ascii
          (Get-FileHash "eyeflow-$v-x86_64-portable.zip" -Algorithm SHA256).Hash |
            Out-File "eyeflow-$v-x86_64-portable.zip.sha256" -Encoding ascii

      - name: Upload build artifacts (debug/diagnostics)
        uses: actions/upload-artifact@v4
        with:
          name: eyeflow-${{ github.ref_name }}
          path: |
            *.exe
            *.zip
            *.sha256

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          draft: true
          generate_release_notes: true
          files: |
            *.exe
            *.zip
            *.sha256
```

要点清单:
- **tag 触发** `v*.*.*`(ripgrep 用纯数字 tag,这里建议带 `v` 前缀,更常见);
- **tag 与 Cargo.toml 版本一致性校验**(ripgrep 同款思路,防止忘改版本);
- **`--locked`** 严格按 Cargo.lock 构建,可复现;
- **rust-cache**:release 构建缓存(ripgrep 未用缓存,单平台小项目加上收益明显);
- **NSIS**:runner 不预装,`choco install nsis -y` 后 `makensis` 进 PATH;
- 先 `upload-artifact` 便于失败排查,再 `draft: true` 发布,人工检查后点 Publish(ripgrep 亦用 draft + `gh release upload`);
- 以后若需跨平台/加签名,再扩展 matrix 与 attest-build-provenance 步骤(见 ripgrep)。

---

## ④ 发布渠道路线图(低成本优先)

| 阶段 | 渠道 | 成本 | 说明 |
|---|---|---|---|
| 0(现在) | **GitHub Releases** | 零 | Setup.exe + 便携 zip + SHA256,tag 自动构建(③)。所有后续渠道都以 Releases 的固定 URL 为源 |
| 1(可选,半天) | **scoop 自有 bucket** | 极低 | 自建 GitHub 仓库放 JSON manifest(`bucket/eyeflow.json` 指向 Releases URL),用户 `scoop bucket add <你>/eyeflow-bucket` + `scoop install eyeflow`。无审核、纯静态文件。Scoop 本身即"command-line installer for Windows",官方支持第三方 bucket |
| 2(值得做) | **winget(winget-pkgs PR 模式)** | 低(免费、无公司要求) | fork `microsoft/winget-pkgs` → 用 `wingetcreate` 对着 Release URL 生成 manifest → 提 PR → 自动验证 + 微软人工审核 + CLA bot。**个人开发者可提交**,无需公司身份;审核数天到数周。要求安装包 URL 稳定,故 Releases 资产命名要固定(`EyeFlow-<ver>-Setup.exe`) |
| 不建议现在 | Microsoft Store / MSIX | 高 | MSIX 需要改造打包并购买代码签名证书(Store 分发亦要求签名),与"低成本"冲突 |
| 不建议现在 | Chocolatey 社区源 | 中 | 需维护审核账户,winget 已覆盖同等人群 |
| 后续可选 | 代码签名 | 中 | 未签名 exe 会触发 SmartScreen 警告(README 里写一句"更多信息→仍要运行"即可);开源项目可申请 SignPath 免费签名(espanso 即用此方案) |

---

## ⑤ E:\Projects\MyGitHub 现状与命名对齐

盘点结果(2026-09-08):

| 目录 | 状态 |
|---|---|
| `DailyWork` / `TestProject` | 空目录,无 .git |
| `GenericAgent` | Python 项目,有 .git,MIT(2025 lsdefine),含 CONTRIBUTING.md |
| `agentic-engineering-framework` | 有 MIT(2026 Yicheng Wei)+ CONTRIBUTING.md,**无 .git** |
| `pcb-parallel-check` | Qt/C++ 项目,有 .git,**无 LICENSE** |

观察与建议:
- 目录存在,可放正式仓库;命名风格混用(PascalCase:`GenericAgent`;kebab-case:`agentic-engineering-framework`、`pcb-parallel-check`)。
- **新仓库名用全小写 `eyeflow`**,与 kebab-case 多数派及 crates.io/GitHub 主流一致(crates.io 包名本身即小写)。
- LICENSE 跟随多数派选 **MIT**,版权行格式与 `agentic-engineering-framework` 一致(`Copyright (c) 2026 <你的名字>`)。
- 初始化顺序建议:GitHub 上建空 Public 仓库(勾选 MIT + .gitignore(Rust) + README 模板)→ 本地按①白名单复制文件 → `git init && git remote add origin ... && git push -u origin main`;打 `v0.1.0` tag 验证 workflow。
- 顺带:eyeflow 目录本身无 .git(git status 确认 fatal: not a git repository),迁移即全新历史,无历史包袱。

---

## ⑥ 来源列表

**本地来源**
- `E:\Workspaces\Hanako\eyeflow\{Cargo.toml, INSTALL.md, README.md, tasks.md, build-check.bat, build-release.cmd, install-rust.ps1, installer/installer.nsi, src/, docs/spec.md}`(逐文件读取)
- `E:\Projects\MyGitHub\{GenericAgent, agentic-engineering-framework, pcb-parallel-check, DailyWork, TestProject}`(目录与 LICENSE 抽查)

**网络来源(调研日期 2026-09-08)**
- A. stretchly README(README 结构/badge/截图/多渠道分发/贡献指引):https://raw.githubusercontent.com/hovancik/stretchly/master/README.md (仓库 https://github.com/hovancik/stretchly )
- B. espanso README(Rust 桌面工具 GPL-3.0、GIF 演示、SignPath 签名):https://raw.githubusercontent.com/federico-terzi/espanso/dev/README.md (仓库 https://github.com/federico-terzi/espanso )
- C. ripgrep release workflow(tag 触发/版本校验/draft release/gh release upload):https://github.com/BurntSushi/ripgrep/blob/master/.github/workflows/release.yml
- D. Cargo Book — Cargo.toml vs Cargo.lock("When in doubt, check Cargo.lock into VCS"):https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html
- E. Cargo FAQ — Why have Cargo.lock in version control(cargo new 默认跟踪;bin 提交收益):https://doc.rust-lang.org/cargo/faq.html
- F. Cargo Book — SemVer Compatibility(0.y.z 中 y 视为 major,leftmost-non-zero):https://doc.rust-lang.org/cargo/reference/semver.html
- G. SemVer 2.0.0 spec(0.y.z 为初始开发,任何内容可变):https://semver.org/
- H. winget 提交流程(fork → manifest → PR → 验证与人工审核):https://learn.microsoft.com/en-us/windows/package-manager/package/repository ;仓库:https://github.com/microsoft/winget-pkgs ;清单生成器:https://github.com/microsoft/winget-create
- I. Choose a License — MIT(小工具许可选择):https://choosealicense.com/licenses/mit/
- J. Scoop(命令行安装器;第三方 bucket:`scoop bucket add <name>` / bucket 目录):https://github.com/ScoopInstaller/Scoop
- K. Swatinem/rust-cache(Rust CI 缓存):https://github.com/Swatinem/rust-cache
- L. softprops/action-gh-release(发布 Action):https://github.com/softprops/action-gh-release
