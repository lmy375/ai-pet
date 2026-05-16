# PanelMemory item hover tooltip 显示 created / updated 时间

## 背景

PanelMemory 任意 item 行的 hover 500ms 浮 detail.md 预览 tooltip 之前只显 `📄 detail_path` 头 + detail body 前 600 字。

但 owner 经常想知道 item 年龄 / 最近改动时刻 —— 比如：
- "这条 user_profile 记忆是不是过时了？" → 看 updated_at
- "ai_insights 里这条反思是几月写的？" → 看 created_at
- "我刚才改了吗？" → 看 updated_at

既有体验：要查时间必须点 "✎ 编辑" 才看到字段；或扫 yaml 文件。本 iter 把"📅 创建 X 前 · 🔄 更新 Y 前" 加到 hover tooltip 头，免一次点击。

之前 TODO 描述"右键菜单加📅 显示创建时间"，但 PanelMemory items 当前没有 right-click 菜单基础设施。改成扩展既有 hover preview tooltip 更轻量、对用户也更顺手（hover 自然，右键得有意识）。

## 改动

### `src/components/panel/PanelMemory.tsx`

#### 1. previewActive gate 从 `+ previewText 非空` 放宽到只 `previewActive`

之前：`{previewActive && previewText && previewText.length > 0 && (...)}` —— detail.md 为空时 tooltip 不显，owner 无从查时间。

之后：`{previewActive && (...)}` —— 外壳总渲，内部预览段独立 gate。

#### 2. tooltip 头加 📅 / 🔄 时间行

```tsx
{(() => {
  const nowMs = Date.now();
  const createdMs = item.created_at ? Date.parse(item.created_at) : NaN;
  const updatedMs = item.updated_at ? Date.parse(item.updated_at) : NaN;
  const fmt = (ms: number) => {
    const age = nowMs - ms;
    return age < 60_000 ? "刚刚" : formatRelativeAgeBuckets(age);
  };
  const parts: string[] = [];
  if (!Number.isNaN(createdMs)) parts.push(`📅 创建 ${fmt(createdMs)}`);
  if (!Number.isNaN(updatedMs) &&
      (Number.isNaN(createdMs) || Math.abs(updatedMs - createdMs) > 60_000)) {
    parts.push(`🔄 更新 ${fmt(updatedMs)}`);
  }
  if (parts.length === 0) return null;
  return (
    <div title={`created_at: ${item.created_at || "（缺）"}\nupdated_at: ${item.updated_at || "（缺）"}`}>
      {parts.join(" · ")}
    </div>
  );
})()}
```

- 复用既有 `formatRelativeAgeBuckets` helper（PanelTasks / PanelChat 同算法）
- created_at vs updated_at ≤ 60s 视为同一动作 → 仅显 📅 一段（防"创建 5 天前 · 更新 5 天前"重复噪音）
- 解析失败 / 字段为空 → 跳过对应段；都失败 → 整行不渲（不显空行）
- inner `title=...` 附完整 ISO 串让 owner 想看精确时刻也行

#### 3. detail.md 空时的 fallback 文案

```tsx
{previewText && previewText.length > 0 ? previewText : (
  <div style={{ ...italic muted ... }}>（detail.md 无内容 / 未写过）</div>
)}
```

让 owner hover 空 detail.md item 时仍看到"占位说明"，配合上面新加的时间行就形成完整 hover info card。

## 关键设计

- **改 gate 而不是另建 tooltip**：复用既有 hover preview pipeline（startPreviewHover / endPreviewHover / previewActive / previewText 缓存）→ 不引新 React state / event listener。
- **创建 vs 更新 ≤ 60s 视为同一动作**：刚创建的 item create_at ≈ updated_at（同一 IO 一次写入），重复显两段噪音；> 60s 才算"被改过"，更新段独立显。
- **`Number.isNaN` 双门**：created_at / updated_at 字段缺失（旧 yaml 数据 / DB migration 漏字段）/ malformed ISO 串都自然降级，不抛 error 不空 div。
- **改用 hover 而非右键菜单**：原 TODO 说"右键菜单加📅 显示创建时间"。但 PanelMemory items 当前没有 ctxMenu 基础设施，加一套全 menu 框架（state + render + click-outside-close + ESC handler）比扩 hover tooltip 大 10x。Hover 也比右键更顺手（owner 视线移过去就触发，不必先有"我想知道时间"的明确意图）。
- **既有 tooltip 内部插入 + 不破坏布局**：新行用相同 fontSize 10 + muted color 不抢眼；marginBottom 2px 与既有 📄 path 行间留 vertical rhythm 一致。
- **inner title attr 给精确时刻**：相对时间是 "ambient" 信号；想精确 owner hover 时间行可看 native tooltip 显完整 ISO 串。

## 不做

- **不引入 right-click menu 框架到 PanelMemory items**：本 iter 范围外；hover tooltip 已足够。
- **不显完整 ISO 串在主行**：太长 / 不必要；inner title 自带。
- **不加 "创建" 段 click-to-edit**：要双击 title 改名 / 双击 description 改内容，与时间字段语义无关。
- **不写测试**：纯 Date.parse + formatRelativeAgeBuckets（已被多 caller 验证过的 helper）+ inline 渲染。视觉验证（hover 一条 user_profile item → tooltip 应显时间行 + 现有 detail content）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.17s
- 改动 ~70 行（gate 调整 + 时间行 IIFE 50 + fallback 文案 12 + 注释 8）。既有 startPreviewHover / endPreviewHover / previewActive / previewText 缓存路径完全不动；item row 本身布局 / 操作按钮 / 排序 / pinned chip 一切不变。

## TODO 状态

剩 1 条留池：
- ChatMini bubble 底 "⏱ N 分前" hover chip

## 后续

- 加 right-click context menu 框架到 PanelMemory items（参考 PanelTasks 既有 taskCtxMenu 模板），扩展 "📅 显完整时间"、"🔗 复制 detail.md path"、"📋 复制 description" 等 entries 一站式。
- 老 item（> 90 天没动过）顶上加 "🕰 久未更新" 灰 chip 与既有 latestTs / churn sparkline / 闲置 hint 形成多层时间信号。
- detail.md 编辑器底部 status bar 加 "📅 N 天前创建 · 🔄 X 分钟前最近改" 一行，编辑时就能 ambient 看到时间。
