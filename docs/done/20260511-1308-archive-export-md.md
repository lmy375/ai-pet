# PanelTasks 历史归档导出 markdown

## 需求

consolidate 会把 30 天前已结束的 butler_tasks 自动挪到 task_archive 类目。
用户打开归档面板能逐条看，但缺一键"把全部归档拼成 markdown 复制出来"的功
能 —— 适合做年度 / 半年回顾贴到周报 / 公众号 / Notion。

## 实现

`src/components/panel/PanelTasks.tsx`：

新 callback `handleExportArchiveAsMd`：

- 按 archive item title 前缀 `YYYY-MM-DD_` 解析日期，按 `YYYY-MM` 分组
- 月份按 desc 排（最新月份在前；同月内复用既有 `archiveItems` 已 sort 过的
  `updated_at desc`）
- 每月段：`## YYYY-MM (N 条)` 标题 + 每条 `- **YYYY-MM-DD** title`，description
  非空时附 sub-bullet 保留 `[done] / [result: ...] / #tag` 全标记
- 整体 header：`# 任务归档 (N 条 · 当前时间)`
- 写剪贴板成功 → `bulkResultMsg`（既有 toast 通道，4s 自清）

归档头部加 `📋 导出 MD (N)` 按钮，与 "刷新" 同行，archiveLoaded 才显（避免
未加载就给空导出按钮）。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 点开归档 → 加载几十 ms → 头部出现"📋 导出 MD (12)"按钮
  - 点 → 4s toast "已导出 12 条归档到剪贴板（按月份分组）"
  - 粘到笔记看到：
    ```
    # 任务归档 (12 条 · 2026/5/11 上午 1:08)
    
    ## 2026-04 (5 条)
    
    - **2026-04-15** 整理 downloads
      - [archived: 2026-04-15] [task pri=2] 整理 [done]
    - **2026-04-10** 周复盘
      - ...
    
    ## 2026-03 (7 条)
    ...
    ```
  - 0 条归档 → "归档为空，无可导出条目" toast
  - title 不匹配 `YYYY-MM-DD_` → 归到"未归档日期"段（防御）

## 不在本轮范围

- 没做"按年分组 / 按 tag 分组"切换：用户最常想的是"过去一个月做了啥" → 月
  份维度直觉对齐。要按 tag 重组放后续，目前可手动 PostgreSQL grep
- 没把 raw_description / detail_md 一起拉：detail_md 在 file，N 次 IO 串行
  导出可能秒级；当前只用 archive 自身的 description（带 marker 文本）已够回
  顾用

## TODO 池剩余

- PanelChat 全部 session 打包成 snapshot（最后一条）
