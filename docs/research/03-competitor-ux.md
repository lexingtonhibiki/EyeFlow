# 竞品 UX 研究:什么设计真正让用户坚持用下来

> EyeFlow 竞品调研 / 2026-09 / 调研对象:Stretchly、Workrave、EyeLeo、SafeEyes、LookAway、CareUEyes、Iris、Windows 11/PowerToys
> 结论速览:**用户留下来靠"预告 + 可控的强制 + 可见的坚持记录";用户卸载靠"时机不对的打断"。EyeFlow 的差异点应押注在"全屏/心流智能静默 + Rust 轻量"上——这个组合在 Windows 免费市场目前是空白。**

---

## ① 竞品对比矩阵

| 竞品 | 平台/技术 | 定价 | 休息模型与默认参数 | 预告/渐暗 | 强制程度 | 跳过/推迟 | 声音 | 开机自启 | 统计 | 上下文感知 | 维护状态 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **Stretchly** | Win/Mac/Linux,Electron,开源 | 免费开源 | Mini 20s/10min + Long 5min/30min(每 2 个 Mini 后) | 有:前 10s(Mini)/30s(Long)通知 | 默认温和;strict mode 默认**关** | 可推迟 1 次(2min/5min)+ 跳过,Ctrl+X | 结束音 crystal-glass 开;开始音默认**静音** | 默认**关** | 无(仅 debug;社区长期要时间线统计) | 空闲 5min 暂停;DnD 监听;appExclusions 进程排除(仅配置文件) | 活跃(2026) |
| **Workrave** | Win/Linux,开源 | 免费 | Micro 30s/3min + Rest 10min/45min + 每日上限 | 有 | 可"block input / block input and screen";推迟次数可配;可 quiet mode | 有 postpone/skip,均可限制 | 有 | 有 | **完成/跳过/推迟次数**,按日/周/月 | 键鼠活动监测 | 活跃(1.11.1,2026-07) |
| **EyeLeo** | Windows,闭源免费 | 免费 | 短休息(带眼保健操)+ 长休息前通知 | 通知预告 | **强制**:休息期间屏幕阻挡;strict mode 不可跳过 | strict 下无 | 有 | 有 | 无 | 无 | **疑似停更**(官网 eyeleo.com 长期 Cloudflare 假页,镜像站列 v1.3.6) |
| **SafeEyes** | Linux(GTK/Python),开源 | 免费 | 短/长休息 + 休息练习 | 前/后通知 | 休息期间**禁用键盘** | 可配置 | 有 | 有 | 插件形式 | **smart pause**:系统空闲即暂停(X11 用 xprintidle,Wayland 用 pywayland);多屏 | 活跃 |
| **LookAway** | macOS 原生(Swift),商业;**Windows 标注 coming soon** | $19/$29 买断(1 年更新,续费 5 折),上架 Setapp | 基于 20-20-20;screen/blink/posture 三类提醒;Planned Breaks 定时排程 | **有:heads-up 预告 + "Starting break in 3" 倒数**;浮动倒计时跟随光标 | "firm enough to actually make me step back"(用户语);休息中不可随便略过,但开始前可 snooze +1/+5/+15min | 开始前 snooze / 立即开始 | 多种舒缓音可换 + 自定义背景/文案 | 有 | 使用统计+菜单栏实时状态(官网详述较弱) | **最强**:录屏/会议通话/视频播放/深度专注应用/全屏游戏五类自动 smart pause;v2.3 加 idle prediction | 活跃(v2.4.5) |
| **CareUEyes** | Windows,商业 | 付费(Pro)+ 免费版 | 定时休息提醒 | 有 | 弱(纯提醒) | 有 | 有 | 有 | 弱 | 无(强项是蓝光/亮度/聚焦压暗) | 活跃 |
| **Iris** | Win/Mac,商业 | 买断+订阅混杂 | 定时休息 + 蓝光 | 有 | 弱 | 有 | 有 | 有 | 弱 | 无 | 活跃但口碑下滑 |
| **Win11 内置** | 系统 | 免费 | Clock「专注会话」(番茄钟,非护眼);夜间模式只调色温 | 无 | 无 | - | - | - | 专注时长 | DnD 计划 | 内置 |
| **PowerToys** | 系统 | 免费 | **无护眼/休息模块**(Awake 只是防休眠) | - | - | - | - | - | - | - | 活跃 |

关键出处:Stretchly 官方文档与源码默认值、Workrave 官网/FAQ/翻译源、SafeEyes README、LookAway 官网与定价页、Trustpilot/Reddit、Microsoft Learn(见文末来源列表)。

---

## ② 用户"爱 / 恨"清单(带来源)

### 爱的理由(让用户坚持)

1. **休息前的预告/渐暗,给人收尾时间** —— Stretchly 默认提前 10s/30s 通知;LookAway 的核心口号就是 "gives you a heads-up beforehand"+"Starting break in 3",预告被开发者称为"不打断心流地告知"。(stretchly/about;lookaway.com;HN#48668132)
2. **全屏遮罩 + 大倒计时 = 仪式感与"被允许休息"** —— Stretchly 默认 85% 屏宽的遮罩窗;LookAway 用户:"firm enough to actually make me step back, reset my eyes, and come back feeling better"。PerkPilot 60 天评测的结论是"仪式感就是产品本身"。(App Store 评价;perkpilot.io/review/lookaway)
3. **休息内容:护眼贴士/眼保健操** —— Stretchly 每次随机给一条 break idea;EyeLeo 的眼保健操(带小豹子演示)+ 屏幕阻挡是其最大记忆点,PCWorld/MakeUseOf 均以此推荐。(hovancik.net/stretchly/about;igorkozarchuk.github.io/eyeleotest;pcworld.com)
4. **可换声音,且开始音默认静音** —— Stretchly 默认只有结束提示音(crystal-glass),开始音 silence,还可设音量;LookAway 提供舒缓音集+自选音频。(Stretchly defaultSettings.js;lookaway.com)
5. **有限而非无限的跳过/推迟** —— Stretchly 一次休息只允许推迟 1 次(且只在前 30% 时段内);LookAway 的 snooze 只有 +1/+5/+15 三档。有限选项既保留控制感又防止"无限逃逸"。(stretchly/about;lookaway.com)
6. **跳过/推迟被记录成"欠账"并可视化** —— Workrave 统计"完成/推迟/跳过次数"按日周月展示,是老用户留存的核心;Stretchly 社区仍在高票请求时间线统计(issue #266, 15 评论)和 Break Health Mode(跳过越多屏幕边缘"血条"越红)。(workrave.org;stretchly issues #266)
7. **上下文感知(休息时机选得对)** —— LookAway 五类 smart pause(录屏/会议/视频/深度专注/全屏游戏)+ idle prediction 被用户认可为"less annoying",其 Show HN 标题就是"a break reminder that knows when not to interrupt"。(lookaway.com;HN#48668132)
8. **买断制 + 原生性能** —— LookAway $19 买断 "worth every penny";对比下 Electron 的 Stretchly 在 M1 上被报"能耗过高"(#1278)。(lookaway.com/pricing;stretchly#1278)
9. **离开即暂停(不强算离席时间)** —— Stretchly 空闲 5 分钟自动暂停、锁屏/睡眠暂停、DnD 模式联动,默认全开。(stretchly/about)

### 恨的理由(让用户卸载)

1. **时机不对的打断:全屏时弹窗把游戏/演示最小化** —— Stretchly issue #355"Full screen applications disable Stretchly"2019 年开至今(32 评论)未做自动静默,这是最高热的功能类 issue。(github.com/hovancik/stretchly/issues/355)
2. **太频繁/参数激进** —— Workrave 默认 3 分钟一 microbreak,官方 FAQ 自己承认"设置因人而异,只能自己试";Stretchly 社区要求固定时间表(#1638)、打字时延后(#1007)都源于默认节奏不合身。(workrave.org/faq;issues #1638/#1007)
3. **强制过头** —— Workrave 可 block input and screen,EyeLeo strict mode 不可跳过;Stretchly 用户要求"strict 模式下仍显示托盘菜单"说明强制期间失控感强。官方 FAQ 拒绝"休息时继续看屏幕"功能时语气强硬,是双刃剑。(Workrave po 源;eyeleotest;workrave.org/faq)
4. **Electron 原罪:内存/能耗/自启失效** —— Stretchly #1278(M1 能耗)、#1259(开机托盘不显示)、微软商店版自启直接被禁用。(stretchly known issues)
5. **锁屏/睡眠后计时错乱** —— Stretchly #1724"Breaks Reset After a Certain Period of Suspend/Lock"(18 评论),说明"离开重置"的边界处理是普遍痛点。(stretchly#1724)
6. **通知太吵/抢焦点** —— Stretchly Windows 版有"升级后永远处于 DnD 导致不弹窗"、浏览器在休息后无响应等 known issues;用户还要求暂停 DnD 检查(macOS #1549)。(stretchly known issues/#1549)
7. **提醒叠加过多** —— LookAway 被 ClemStation 点名"提醒过载风险"(护眼+眨眼+坐姿三路提醒)与"明显比免费替代贵"。(clemstation.com)
8. **商业信任崩塌** —— Iris Trustpilot 代表性投诉:"我买了终身会员,今天却提示过期要再付费";CareUEyes 在 r/software 被问"安全吗",Pro 调光失效,Chrome 生态里同名扩展曾被标记有害;r/webdev 有人专门求"不是 Iris 或 CareUEyes"的方案。(trustpilot.com/review/iristech.co;reddit)
9. **停更即弃用** —— EyeLeo 官网长期打不开(Cloudflare 假页),用户被迫迁移到 Stretchly/LookAway;"死了的护眼软件"本身就是需求入口。(eyeleo.com 现状;fileeagle.com 镜像)

---

## ③ 对 EyeFlow 的功能取舍与差异化建议

### 现状判断

- **"全屏游戏智能静默"有人做吗?** LookAway 做了(全屏游戏/深度专注/会议/录屏/视频五类),但它是 **macOS 付费原生应用,Windows 版"coming soon"**。Stretchly 在 Windows 上六年(issue #355,2019 年至今)都没做全屏检测,只有需要手写 JSON 的进程排除;SafeEyes 只做了空闲检测;Workrave 完全没有。**Windows + 免费/lightweight + 全屏与心流静默 = 市场空白,EyeFlow 的定位正好落在这里。**
- **LookAway smart pause vs EyeFlow 的区别**:LookAway 检测的是"应用语义"(哪个 app 在录屏/开会/游戏),靠 macOS 私有 API 和启发式(idle 拉伸少、应用切换率低 = deep focus)。EyeFlow 在 Windows 上可以用更直接的系统信号(前台窗口全屏态、后台窗口枚举、输入节奏),且可以把"游戏模式=只响提示音不弹窗"做成显式场景而非隐藏行为——这是 LookAway 没有的"分级降级"设计。
- **切入人群**:被 EyeLeo 停更和 Electron 系臃肿劝退的 Windows 老用户,是被验证过的迁移流量(Reddit/榜单推荐位全被 Stretchly 占据但它是 Electron)。

### MVP 必须有的 5 个

1. **双层休息 + 渐暗预告**:Mini(如 20s/10min)+ Long(5min/30min),休息前 10~30s 屏幕边缘渐暗/角标预告 + "3-2-1"倒数。预告是被所有成功产品验证的第一交互(stretchly/about;lookaway.com)。
2. **全屏/心流智能静默(核心卖点)**:检测全屏前台窗口(游戏/视频/演示)→ 自动降级为"仅托盘变色 + 静音提示音,不弹窗";退出全屏后补一次轻提示。默认开启,可在托盘一键开关。
3. **离开即暂停**:空闲 N 分钟暂停计时、锁屏/睡眠/解锁重置计时。必须把 #1724 这类边界(短暂锁屏 vs 长离开)处理对,这是竞品的公开伤疤。
4. **有限跳过 + 本地统计**:每次休息只可推迟 1 次(+2min/+5min);统计"今日完成/跳过/推迟次数 + 连续坚持天数",托盘 tooltip 可见。跳过计数既是留存钩子也是"欠账可视化"。
5. **温和的休息界面 + 可换声音**:全屏遮罩 + 大倒计时 + 一条护眼贴士;开始音默认静音、结束音轻柔可换;托盘菜单含"暂停 30m/1h/到明天"。

### 明确不要的 3 个

1. **蓝光过滤/亮度调节**:与 Windows 夜间模式、f.lux、CareUEyes/Iris 正面重叠,红海、需要显示驱动权限、且把产品从"提醒工具"拖向"系统美化工具"(用户对这两家商业厂的信任问题见 ②-8)。
2. **强制锁键盘/不可跳过的默认独裁模式**:Workrave 的 block input 和 EyeLeo 的 strict mode 是高级选项而非默认,因为"失控感"是卸载诱因;EyeFlow 至多提供可选 strict,默认必须温和。
3. **插件系统/帐号云同步/订阅付费墙**:SafeEyes 的插件体系拉高了复杂度却非留存动因;Iris 的订阅化直接引发口碑崩塌。EyeFlow 应保持单文件、本地数据、一次性买断或免费的极简心智。

### 差异化一句话

**"会看情况的休息提醒":Windows 上第一个默认懂全屏游戏、懂心流、懂离开的护眼托盘,Rust 单进程、无 Electron、内存个位数 MB。**

---

## ④ 来源列表

**官方/文档**
- Stretchly 官方文档(默认参数、推迟/跳过、strict、声音、known issues):https://hovancik.net/stretchly/about/
- Stretchly 默认值源码(openAtLogin:false、开始音静音等):https://github.com/hovancik/stretchly/blob/trunk/app/utils/defaultSettings.js
- Stretchly 仓库:https://github.com/hovancik/stretchly
- Workrave 官网:https://www.workrave.org/ ;FAQ:https://www.workrave.org/faq/
- SafeEyes README(特性、smart pause 依赖):https://github.com/slgobinath/SafeEyes ;官网:https://slgobinath.github.io/safeeyes/
- LookAway 官网(智能暂停清单、预告、snooze、倒计时):https://lookaway.com/ ;定价:https://lookaway.com/pricing
- CareUEyes 官网:https://care-eyes.com/
- EyeLeo 官方页(开发者镜像):https://igorkozarchuk.github.io/eyeleotest/ ;现官网 http://eyeleo.com/(Cloudflare 假页,佐证停更);镜像版本页:https://www.fileeagle.com/software/2469/EyeLeo
- Microsoft 专注会话:https://support.microsoft.com/en-us/windows/experience/focus-stay-on-task-without-distractions-in-windows
- PowerToys 模块列表(无护眼模块,Awake 仅防休眠):https://learn.microsoft.com/en-us/windows/powertoys/

**Issues / 用户声音**
- Stretchly #355 全屏问题(2019 至今 open):https://github.com/hovancik/stretchly/issues/355
- Stretchly #266 历史统计请求:https://github.com/hovancik/stretchly/issues/266 ;#1724 锁屏重置:https://github.com/hovancik/stretchly/issues/1724 ;#1278 M1 能耗:https://github.com/hovancik/stretchly/issues/1278 ;#969 摄像头暂停:https://github.com/hovancik/stretchly/issues/969 ;#1007 打字延后:https://github.com/hovancik/stretchly/issues/1007 ;#1638 固定休息时间:https://github.com/hovancik/stretchly/issues/1638 ;#667 每日上限:https://github.com/hovancik/stretchly/issues/667
- Workrave 强制文案源(block input 等):https://github.com/rcaelers/workrave/blob/main/po/workrave.pot
- LookAway Show HN("knows when not to interrupt",深度专注检测讨论):https://news.ycombinator.com/item?id=48668132
- LookAway App Store 评价("firm enough…"):https://apps.apple.com/us/app/lookaway-break-reminder/id6747192301
- PerkPilot 60 天评测:https://perkpilot.io/review/lookaway ;ClemStation 批评(贵、提醒叠加):https://clemstation.com/blog/20260323-best-eye-strain-apps-mac-2026
- Iris Trustpilot(终身会员失效投诉):https://www.trustpilot.com/review/iristech.co
- CareUEyes Reddit 质疑:https://www.reddit.com/r/software/comments/1pzpm3h/eye_protection_software_is_careueyes_safe_to_use/
- EyeLeo 评价:PCWorld https://www.pcworld.com/article/464329/eyeleo.html ;MakeUseOf https://www.makeuseof.com/tag/eyeleo-prevent-eye-strain-pc/
