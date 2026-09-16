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
cargo clippy
cargo test
```

- `EYEFLOW_DEMO=1` 可启动演示模式（60 秒后触发一次完整提醒流程），便于验收界面改动。

## 发布流程（维护者）

1. 在 `dev` 上把 `CHANGELOG.md` 的 `Unreleased` 内容整理为 `[x.y.z]`；
2. 合并 `dev` → `main`；
3. `Cargo.toml` 版本号与 tag 一致后：`git tag vx.y.z && git push origin main vx.y.z`；
4. CI 自动构建 `EyeFlow-x.y.z-Setup.exe`（NSIS）+ 便携 zip + SHA256 并发布到 GitHub Releases。
