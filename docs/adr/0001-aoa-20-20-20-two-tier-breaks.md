---
status: accepted
date: 2026-09-08
---

# 默认节奏采用 AOA 20-20-20 双层休息，取代 10~18 分钟 / 25 秒

v0.1 的默认是“10~18 分钟加权随机一次、休息 25 秒”，且只有一层短休息。调研（docs/research/02-science-evidence.md）表明：唯一同时有机构指南背书和随机对照试验直接检验的节奏是 **AOA 的 20-20-20**（每 20 分钟看 20 英尺外 20 秒），AOA 同页还要求**连续用屏 2 小时后休息 15 分钟**；自发微休息的平均时长是 27.4 秒且提前结束会恢复不足。因此 v0.2 默认改为：**短休息 15~25 分钟加权随机（峰值 20 分钟）、持续 30 秒；新增长休息：连续用屏累计 2 小时后提示休息 15 分钟**。所有值仍可配置。

## Considered Options

- 保留 10~18 分钟：只有肌肉骨骼不适方面的证据（Balci 2003 的 15 分钟 + 微休息方案），对眼疲劳没有优势，且更密的打断会降低依从性。
- 每 30 分钟休息 5 分钟：Balci 2003 中眼疲劳最低的方案，但无机构指南背书，作为长休息的可选档保留。

## Consequences

- 配置结构新增长休息相关字段，旧 `config.toml` 通过默认值兼容。
- 休息界面文案必须包含“看 6 米（20 英尺）外”和“多眨几次眼”（AAO：用屏时眨眼从每分钟约 15 次降到 5~7 次），避免任何“防近视 / 防眼损伤”类表述（AAO 定位数字视疲劳为暂时性）。

## Sources

- AOA, Computer Vision Syndrome: https://www.aoa.org/healthy-eyes/eye-and-vision-conditions/computer-vision-syndrome
- AAO, Computers, Digital Devices and Eye Strain: https://www.aao.org/eye-health/tips-prevention/computer-usage
- Talens-Estarelles et al. 2023 (20-20-20 RCT): https://pubmed.ncbi.nlm.nih.gov/35963776/
- Henning et al. 1989 (微休息 27.4 s): https://pubmed.ncbi.nlm.nih.gov/2806221/
- Balci & Aghazadeh 2003 (工作/休息方案对照): https://pubmed.ncbi.nlm.nih.gov/12745696/
