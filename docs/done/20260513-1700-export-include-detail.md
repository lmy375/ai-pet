# PanelTasks 导出 visible markdown 加 include detail toggle

## 需求

R98 的"📋 导出 MD"只拼 title / desc / meta 三段，不包含 detail.md 进
度笔记 —— 复盘 / 提 issue 时 detail 才是关键。补 toggle "✓ 含 detail"，
勾上后导出顺手拉每条任务的 detail.md 一并写入。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 新 state `exportIncludeDetail: boolean`，localStorage key
  `pet-tasks-export-include-detail`，跨重启持久
- `setExportIncludeDetailPersist` 同步写盘
- `handleExportAllVisibleAsMd` 改造：
  - toggle on：Promise.all 并发拉每条 detail（先看 detailMap 缓存，miss
    则 invoke task_get_detail，单条失败容忍走默认 noDetail format）
  - toggle off：原行为（仅 formatTaskAsMarkdown w/o detail）
- 导出时 status msg 区分含 detail 路径
- toggle UI：checkbox label 插在 "📋 导出 MD" 按钮前，title 说明耗时
  与 detail 含义
- 按钮 tooltip 根据 toggle 状态动态描述

`formatTaskAsMarkdown(t, detail)` 第二参已在既有 helper（line 213）支
持，无需改 helper。

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 默认 toggle off → 导出与 R98 一致
  - 勾选 toggle → 重启 panel 后仍勾选
  - 勾选后点 "📋 导出 MD" → 显 "正在拉 detail.md…" 进度提示 → 完成显
    "已导出 N 条（含 detail）"
  - 已 hover preview 过的任务（detailMap 命中）即时；未 hover 的需拉
  - 单条 detail fetch 失败 → 该条仅导 title/desc，不阻塞整批

## 不在本轮范围

- 没做"含 history"toggle（与 detail 同 axis）：history 段当前导出 helper
  不支持；要支持需扩 formatTaskAsMarkdown 签名
- 没做 export to file（仍剪贴板路径）：长队列含 detail 可能巨长，未来
  超出剪贴板限制可考虑文件路径
- 没做"显进度 bar"（Promise.all 期间）：< 50 条 < 1s 通常无感；> 100
  条可能眼花，但 scope 边际

## TODO 池剩余

- PanelChat "💾 保存为模板"
