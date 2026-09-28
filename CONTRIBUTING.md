# 贡献指南

感谢关注 EyeFlow！本项目由个人维护，接受 Issue 与 Pull Request。

## 分支模型

| 分支 | 用途 | 规则 |
|---|---|---|
| `main` | **稳定版** | 只接受合并，永远对应一个可发布的 tag（`v*.*.*`）；打 tag 即触发 CI 自动构建安装包并发布 Release |
| `dev` | **集成分支** | 日常开发合入这里；`dev` 上的 CI 必须全绿，功能攒够后一次合并回 `main` 并打 tag |
| `feature/*` | **工作者分支** | 每个独立改动一条分支，从 `dev` 切出，完成后发 PR 合回 `dev`（单人直接 push `dev` 也可以，但 PR 留痕更好） |

日常流程：

```bat
git switch dev
git switch -c feature/my-change     rem 从 dev 切出工作分支
...开发 + cargo fmt + cargo clippy + cargo test...
git push -u origin feature/my-change
rem GitHub 上发 PR: feature/my-change -> dev
rem dev 验证无误后合并回 main 并打 tag 发布
```

## 提交约定

- 提交信息用英文或中文均可，但请说清“为什么”； releases 采用 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 格式记录在 `CHANGELOG.md`。
- 版本遵循 [SemVer](https://semver.org/)：`0.y.z` 阶段 y 位变更代表功能迭代。
- **提交前请确认**：GitHub 账号已开启 "Keep my email addresses private"，并使用 noreply 邮箱提交（`git config user.email "你的ID+lexingtonhibiki@users.noreply.github.com"`）。

## 开发环境

- Rust ≥ 1.95（MSVC 或 GNU 工具链均可；GNU 需 `windres`，MSVC 需 `rc.exe`）
- 提交前跑一遍：

```bat
cargo fmt
cargo clippy --bins -- -D warnings
cargo test
```

- **推前请再跑一遍 GNU 工具链**。CI 只跑 `windows-latest`（MSVC），不加 GNU 矩阵是有意为之
  （ADR-0007 判决七：`lessons.md` §3.3 那次事故的教训是「写显式断言」不是「矩阵翻倍」，
  为一个已根因消除的 bug 类别付永久 CI 成本是错配）。本机默认工具链就是 GNU，成本近零：

```bat
cargo +stable-x86_64-pc-windows-gnu test
```

- `EYEFLOW_DEMO=1` 可启动演示模式（60 秒后触发一次完整提醒流程），便于验收界面改动。
- `EYEFLOW_DEMO_TABS=1`（配合 `EYEFLOW_DEMO=1`）让设置窗每 2 秒自动翻一页签，
  用于录制 README 的 GIF：录制机上盖着别的窗口时点击送不进设置窗
  （真鼠标注入被上层窗口吃掉，`PostMessage` 的指针消息 winit 也不处理），
  自动翻页让录屏完全不依赖鼠标。`.shots/record_gif.py` 就是用它录的。

## Cargo features

| feature | 默认 | 作用 |
|---|---|---|
| `update-check` | **开**（v0.7.0 起） | 编译在线更新检查所需的 TLS + JSON 栈（`ureq` / `rustls` / `ring` / `serde_json`），实测 +1,139,200 B。v0.6 默认关掉它是为了体积，但那样下载版里「关于」页的更新开关根本不存在——把一个用户用得上的功能砍成"体积优化"是算错账，v0.7.0 改回默认开启 |

极限体积构建：`cargo build --release --no-default-features`（约 8.66 MiB）。
体积取舍的完整推导见 [ADR-0007](docs/adr/0007-v0.6-scope.md) 判决五与 [CHANGELOG](CHANGELOG.md) 的 v0.7.0。

## 发布流程（维护者）

1. 在 `dev` 上把 `CHANGELOG.md` 的 `Unreleased` 内容整理为 `[x.y.z]`；
2. 合并 `dev` → `main`；
3. `Cargo.toml` 版本号与 tag 一致后：`git tag vx.y.z && git push origin main vx.y.z`；
4. CI 自动构建 `EyeFlow-x.y.z-Setup.exe`（NSIS）+ 便携 zip + SHA256 并发布到 GitHub Releases。

### 发布附带的两件手工活

- **社交预览图**：GitHub **没有**设置它的 API，只能在仓库 *Settings → Social preview*
  手动上传 `assets/social-preview.png`（1280×640）。这张图由**本机的**
  `.shots/make_social_preview.py` 从真实录屏帧合成（`.shots/` 按 `.gitignore`
  约定是"本机验收产物、不入库"，所以脚本不在仓库里——要复现的话，它做的就是
  "Pillow 铺一个渐变底 + 贴图标 + 贴两个面板帧"）。
- **README 里的 GIF**：`assets/gifs/*.gif` 由**本机的** `.shots/record_gif.py` 录制，
  中英各一遍（设置窗那支靠 `EYEFLOW_DEMO_TABS=1` 自动翻页签，完整流程那支靠
  `EYEFLOW_DEMO=1` 的 60 秒演示周期 + 一次真实点击「现在开始」）。
  界面明显改动后应当重录——**README 里挂着旧 UI 的 GIF 比没有 GIF 更糟**。
