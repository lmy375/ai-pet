# detail.md 编辑器底 status bar 加 📅 创建 / 🔄 更新 时间段

## 背景

iter #194 给 PanelMemory item hover tooltip 加了 "📅 创建 X 前 · 🔄 更新 Y 前"。detail.md 编辑器底 status bar 已有 "● 未保存 / 行 N / ☑ checklist / 字数 / dirty 警示" 多段 chip，缺一段时间信号 —— owner 编辑长 task detail 时也常想知道"这条任务多老了 / 上次改动什么时候"。

补一段时间 chip 到 status bar，与 PanelMemory 同源信号。

## 改动

### `src/components/panel/PanelTasks.tsx`

在 detail editor 底部 status bar，"行 N / 共 M" chip 之后、☑ checklist 进度 chip 之前插入：

```tsx
{(() => {
  const nowMs = Date.now();
  const cMs = t.created_at ? Date.parse(t.created_at) : NaN;
  const uMs = t.updated_at ? Date.parse(t.updated_at) : NaN;
  const fmt = (ms: number) => {
    const age = nowMs - ms;
    return age < 60_000 ? "刚刚" : formatRelativeAgeBuckets(age);
  };
  const parts: string[] = [];
  if (!Number.isNaN(cMs)) parts.push(`📅 ${fmt(cMs)}创建`);
  if (!Number.isNaN(uMs) && (Number.isNaN(cMs) || Math.abs(uMs - cMs) > 60_000)) {
    parts.push(`🔄 ${fmt(uMs)}改`);
  }
  if (parts.length === 0) return null;
  return (
    <span style={{ fontSize: 10, color: muted, fontFamily: monospace }}
          title={`created_at: ${t.created_at || "（缺）"}\nupdated_at: ${t.updated_at || "（缺）"}`}>
      {parts.join(" · ")}
    </span>
  );
})()}
```

显示形态举例：
- 新建后即编辑：`📅 刚刚创建`
- 创建 2 天前 + 5 分前最后改：`📅 2 天前创建 · 🔄 5 分钟前改`
- 创建 1 周前 + 未再改：`📅 7 天前创建`（updated_at ≈ created_at，省第二段）

## 关键设计

- **复用 `formatRelativeAgeBuckets`** 与 PanelMemory item hover / 任务 row "🕰 N 天前" chip 同 helper（src/utils/formatRelativeAge.ts），跨 panel 单位 / 桶分一致。
- **created vs updated ≤ 60s 合并**：刚创建任务 created_at ≈ updated_at；重复显两段噪音。> 60s 才算"被改过"，🔄 段独立显。
- **解析失败跳整段**：旧数据缺字段 / malformed ISO 都返 NaN → parts 空 → null 渲染。不抛 error 不空 div。
- **inner title attr 显完整 ISO**：相对时间是 ambient；想精确 owner hover chip 看 native tooltip 显原始 created_at / updated_at 串。
- **位于行号与 checklist 进度之间**：status bar 阅读顺序 ● 未保存 / 行号 / ⏱时间 / ☑ checklist / 字数 / 警示 —— 时间贴近行号一侧（编辑导航感同源），与 checklist / 字数等"内容统计"分组。
- **覆盖 view-mode preview 与 edit**：与既有 ☑ / 字数 chip 同生命周期 —— preview / split / edit 三态都显，让任意视图模式下 owner 都能读到时间。

## 不做

- **不在 ChatMini 同样加**：ChatMini bubble 已有顶 [HH:MM] + 底 ⏱ 相对 chip 双时间信号（iter #195）。detail.md editor 与 ChatMini 视觉场景不同，不重叠。
- **不实时 ticking 漂移**：与 PanelMemory item hover 同样 ── created_at 是历史 fixed 不变；updated_at 在 owner 改完 save 时由后端更新，自然触发 setDetailMap 重渲。不需 setInterval 强制刷。
- **不为 created_at = updated_at 显 "未改动过" 文字 hint**：状态栏字符空间紧；省略足以让 owner 意会（看到只有 📅 一段 = 没改过）。
- **不写测试**：纯字符串 + Date.parse + formatRelativeAgeBuckets（被多 caller 验证过）+ inline 渲染。视觉验证（开任一含 detail 的 task → 编辑器底 status bar 看到时间段）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.16s
- 改动 ~55 行（IIFE 计算 + chip 渲染 + 注释）。既有 ● 未保存 / 行号 / ☑ / 字数 / 警示 chip / dirty 检测 / autosave / setDetailMap 路径完全不动。

## TODO 状态

剩 5 条留池：
- PanelTasks 行右键加「🔇 Toggle silent」
- PanelMemory butler_tasks section header 加 silent/snooze 计数 chip
- 桌面 pet collapse tab hover 1s 浮 ambient mini card
- detail.md 编辑器底 status bar 加 字数统计 chip
- butler_task `[snooze: ...]` 自然短串预设

## 后续

- 时间 chip 加 hover 触发"打开 task created_at 那天的整周 timeline"（与 ChatMini cross-session 搜索同入口）—— 编辑期间不离 panel 也能 contextual scope。
- 时间段 chip click → 弹小 dialog 让 owner 一键 reset updated_at 重新计时（罕见 use case：长寿任务想"重新计时" idle 标记）。
- 跨日跳过 30 天后改"📅 N 月前"（与 PanelMemory idle hint 30d → "Nmo+" 同模板）。
