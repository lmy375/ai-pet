# PanelTasks 行加「📋 ref」hover chip（iter #503）

## Background

detail.md / ChatPanel input / TG /quick 等都支持 `「<title>」` token
语法引用某 task — 在 detail.md 内会被渲染成可点 chip 跳回该 task，
在 chat / TG 是结构化引用。但既有路径要复制 task ref token：

- 手敲：要打全角 `「`、记完整 title、打全角 `」` — 三步易错
- 走 ⌘K palette insertRef mode：要先开编辑器 → ⌘K → 切 mode →
  输 query → ↑↓ → Enter — 五步
- 找现成 ref token paste：scope 局限

本 iter 加 row hover chip — 鼠标停 task row 500ms → 看到「📋 ref」chip
→ 点击直接得到 `「<title>」` 到剪贴板。一步。

## Changes

### `src/components/panel/PanelTasks.tsx`

紧贴 `📜 复制 raw` chip 之后插：

```tsx
{taskPreviewHoverTitle === t.title && t.title.length > 0 && (
  <button
    onClick={async (e) => {
      e.stopPropagation();
      const token = `「${t.title}」`;
      try {
        await navigator.clipboard.writeText(token);
        setBulkResultMsg(`📋 已复制 ref：${token}`);
      } catch (err) {
        setBulkResultMsg(`复制 ref 失败：${err}`);
      }
      window.setTimeout(() => setBulkResultMsg(""), 2500);
    }}
    title={`复制 task ref token「${t.title}」到剪贴板 — 粘到 detail.md / chat / 别的 task description 引用本 task；token 在 detail.md 渲染成可点 chip 跳回。`}
    style={{ ...common chip style... }}
  >
    📋 ref
  </button>
)}
```

### Gates

- **`taskPreviewHoverTitle === t.title`**：500ms hover state（与
  📂 / ↗ / 📊 / ↘ / ⏭ / 🔁 / 📅 / 📜 / ⏰ 同节奏）
- **`t.title.length > 0`**：极端兜底，防空 title 复制 `「」` 无用 token
- **无 `!isFinished(t.status)` gate**：done / cancelled task ref 仍有
  audit / 历史引用价值，与 📜 raw chip 同 finished-allowed 设计

## Key design decisions

- **`「」` 而非 `[[...]]`**：与既有 task ref 协议（`renderDetailTextWithLinkCards`
  + ⌘K palette insertRef mode 都用 `「」`）一致；改语法会破渲染
- **`setBulkResultMsg` 2.5s toast 显具体 token**：与既有 📜 raw / 🔁
  schedule chip 同 feedback pattern — owner 即时验证复制内容
- **chip 字 `📋 ref` 而非 `📋 复制 ref`**：与 📜 raw / 🔁 schedule 等
  既有 chip 文本紧凑度协调 — short label + tooltip 详
- **不写 unit test**：纯 clipboard write + 字符串拼接；逻辑 trivial
  （既有 📜 raw chip 同 algorithm + 既有 `「」` token 协议已经
  production 验证）。GOAL.md "meaningful tests only" 规则下不引装饰性
  测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.29s)
- 后端无改动 — 纯前端 hover chip
- 手测：
  - PanelTasks 任意 row hover 500ms → chip 「📋 ref」浮起
  - click → toast 「📋 已复制 ref：「<title>」」
  - 粘到 detail.md 编辑器 → ref chip 渲染（既有 renderDetailTextWith
    LinkCards 路径处理）
  - 粘到 ChatPanel input → 文本「<title>」+ ref 形式
  - 粘到 TG /quick 写 task → 新 task body 含完整 token

## Future iters (out of scope)

- 「⌘⇧Click」复制 `[[butler_tasks/<title>]]` PanelMemory 风 token —
  与既有 detail.md `[[<cat>/<title>]]` 渲染协议双轨；当前 task ref 单
  协议已够 80% 场景
- ChatMini bubble 右键加「📋 ref」菜单项 — 移动入口对偶；当前 row
  hover chip 已覆盖桌面端流
