---
status: accepted
date: 2026-09-08
---

# 开机自启使用 HKCU\Run：安装时默认开启，应用内提供开关

护眼提醒的收益依赖持续使用——20-20-20 RCT 显示停用一周后获益即消失，因此一个不开机自启的提醒工具接近无用。Microsoft Learn 认可的桌面自启机制只有 Run/RunOnce 注册表键和“启动”文件夹，二者都受任务管理器“启动应用”统一管控，用户可一键禁用；任务计划不在该列表中，与“用户始终可控”原则相悖，不采用。微软 UX 指南倾向 opt-in，而本项目选择**安装即开启**（与 Workrave 一致，与 Stretchly 默认关闭不同），折中方式是：在设置界面首屏提供“开机自启”开关，并为 exe 补齐版本资源与图标，使任务管理器启动项显示友好名称而非裸文件名。便携运行（未经安装器）时默认不写入自启。

## Sources

- Run and RunOnce Registry Keys: https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys
- Startup apps（任务管理器管控范围）: https://learn.microsoft.com/en-us/windows/win32/w8cookbook/startup-apps
- Talens-Estarelles et al. 2023（停用一周获益消失）: https://pubmed.ncbi.nlm.nih.gov/35963776/
