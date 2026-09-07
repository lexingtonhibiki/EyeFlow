---
status: accepted
date: 2026-09-08
---

# 提醒形态默认温和（预告 + 不抢焦点的休息界面 + 可跳过），全屏遮罩仅作可选严格模式；不做蓝光过滤与强制锁定

调研显示所有阳性证据（20-20-20 RCT、18 项 RCT 的 Meta 分析、依从性研究）都来自**可忽略的非强制提醒软件**，“强制遮罩 vs 可忽略通知”没有任何头对头研究，而强制形态是竞品被卸载的主要诱因之一（Workrave block input、EyeLeo strict mode）。竞品中被用户反复肯定的交互是：**休息前预告**（Stretchly 提前 10/30 秒、LookAway “Starting in 3”）、**有限次数的延后**（Stretchly 每次休息只能推迟一次）、**可见的坚持记录**（Workrave 完成/跳过/延后统计）。因此 v0.2 的默认提醒链路是：预告（可延后 5 分钟一次 / 跳过 / 立即开始）→ 置顶但不抢焦点的休息界面（倒计时 + 一条权威贴士 + 完成/跳过）→ 结算进本地统计。全屏遮罩作为“严格模式”开关提供，默认关闭。**明确不做**：蓝光过滤 / 色温调节（Cochrane 17 项 RCT 显示对视疲劳无效，AAO 不推荐，且与 Windows 夜间模式重叠）、默认锁键盘、账号云同步、订阅。

## Sources

- Cochrane 2023 蓝光滤镜综述: https://www.cochranelibrary.com/cdsr/doi/10.1002/14651858.CD013244.pub2/full
- AAO, Should You Worry About Blue Light: https://www.aao.org/eye-health/tips-prevention/should-you-worry-about-blue-light
- Leppe-Zamora et al. 2025 Meta 分析（提示类干预有效）: https://pubmed.ncbi.nlm.nih.gov/40514667/
- Stretchly 官方文档（预告、推迟一次）: https://hovancik.net/stretchly/about/
- LookAway（预告、有限 snooze、智能暂停）: https://lookaway.com/
- Workrave（统计作为留存核心）: https://www.workrave.org/
