# 060 · PanelDebug 整改 — 20 action 一行 + 三层 tab + emoji 当组件

Debug 页截图证据：「应用」sub-tab 下 ~20 个 action 挤一排（刷新 / 清空 / logs 目录 / logs 路径 / uptime / .history dir / LLM tools / latency / 导出快照 / R0·K1·S3 / 02:46 倒计时 / cron / force consolidate / 抓快照 A / 立即开口 / 临时 prompt / 看上次 prompt / DevTools / 重置 stash / mute 15min / reload / stash JSON），全部以彩色背景 + emoji 作 button 标识 — 直接违反 059 的「emoji 不作组件」规范。叠加 3 层嵌套 filter（顶 sub-tab / 近 N 时间窗 / kind chip / 子 sub-tab），加 inline Telegram 启动错误 banner，信息密度极高。

需求：
- 沿用 051 中「调试↗ 默认隐藏」原则；本页仅 dev-mode 可见，但 dev 用户看到的仍按 059 生产级要求处理。
- 大 toolbar ~20 action 按职责分 4 个 collapsible section：
  - **状态查看**：刷新 / logs 目录 / logs 路径 / uptime / .history dir / latency / R-K-S 状态
  - **运行控制**：force consolidate / 立即开口 / 重置 stash / mute / reload / cron 配置
  - **抓取与导出**：抓快照 / 导出快照 MD / stash JSON
  - **prompt 调试**：LLM tools / 临时 prompt / 看上次 prompt / DevTools
- 所有 action 按 059 用统一 icon set 替换 emoji；button 文案纯文字（不 + emoji 前缀）；色彩仅区分语义级别（normal / destructive / warning）而非每按钮独立色。
- 多层 filter chip cluster 整合：顶 sub-tab 仅保留「应用 / 日志 / LLM 日志 / 统计」4 个；其下时间窗（近 1d/3d/7d/14d/30d）+ kind filter（开口/沉默/跳过）+ 子 sub-tab（宠物说/工具调用/反馈记录）合并为一个 filter bar，含义不重复。
- inline Telegram 启动错误 banner 改为统一 alert 组件（带 severity / dismiss / 操作链接 "查看原因"）；点击展开错误详情而非常驻完整 stack。
- 顶部 stat row（2 今日 / 2 本周 / 25 累计 / 1.3 日均 / 0.3 周日均 / 一前开口 / 19 天陪伴）整合为定义列表 / 单 inline 行，删除 emoji 前缀；非核心 metric 收进展开。
- 「最新事件」一行 mood / motion / 开口次数等 chip 改为结构化键值表，文案不混用 emoji。
- 另写一条工程问题 req（不在本需求 deliver）：`BOT_COMMANDS_TOO_MUCH` 暴露 TG 命令数已超 Telegram API 上限，需要重组命令族（把 /here_* /cat_* 等按家族 namespace 整理或合并），由独立 req 单独 track。
