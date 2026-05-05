# 任务详情面板复制按钮 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务详情面板复制按钮：详情段「完整描述」/「进度笔记」当前是只读 pre 块；加 hover 显示的「复制」让用户能把描述 / 笔记一键存到外部笔记。

## 目标

任务详情 accordion 里的 3 个段（完整描述 / 进度笔记 / 事件时间线）当前都是
只读 pre + 列表展示。本轮在 hover 时把「完整描述」和「进度笔记」段的 label
旁边显出一个小「复制」按钮，点击 → `navigator.clipboard.writeText` + 1.5s
"已复制"绿色反馈。

## 非目标

- 事件时间线不加复制按钮 —— 它是多行 list，单一 copy 语义不清；用户想复制
  个别行可以浏览器原生选中。
- 编辑模式下的进度笔记 textarea 已有原生选中-复制路径，不重复加按钮。
- 不写 README —— 任务详情视觉补强。

## 设计

### CSS hover-only 显隐

PanelTasks 当前全 inline style，本轮第一次引入 `<style>` 块，class
`.pet-detail-section`：

```css
.pet-detail-section .pet-detail-copy-btn { opacity: 0; transition: opacity 120ms; }
.pet-detail-section:hover .pet-detail-copy-btn { opacity: 0.85; }
.pet-detail-section .pet-detail-copy-btn:hover { opacity: 1; color: #0ea5e9; border-color: #7dd3fc; }
```

与 PanelChat 既有 `.pet-chat-row .pet-copy-btn` 同模式（hover 整段渐显，再
hover 按钮自身强化）。

### 状态 / handler

```ts
const [copiedDetailKey, setCopiedDetailKey] = useState<string | null>(null);
async function handleCopyDetail(key: string, text: string) {
  try {
    await navigator.clipboard.writeText(text);
    setCopiedDetailKey(key);
    window.setTimeout(() => {
      setCopiedDetailKey((prev) => (prev === key ? null : prev));
    }, 1500);
  } catch (e) {
    console.error("clipboard write failed:", e);
  }
}
```

key 用 `${task.title}-rawDesc` / `${task.title}-detailMd` 区分多任务多段（虽然
同时只有一条 expanded，但显式键避免未来扩展时 collide）。

### 应用

把 detailSection 的 label 行包成 flex `<div>`：

```tsx
<div className="pet-detail-section" style={s.detailSection}>
  <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
    <span style={s.detailLabel}>完整描述</span>
    <button className="pet-detail-copy-btn"
      onClick={() => handleCopyDetail(`${t.title}-rawDesc`, detail.raw_description)}
      style={...}>
      {copiedDetailKey === `${t.title}-rawDesc` ? "已复制" : "复制"}
    </button>
  </div>
  <div style={s.rawDescBox}>{detail.raw_description || "（空）"}</div>
</div>
```

进度笔记同理（仅 view 模式下渲染按钮；编辑模式 textarea 不需要按钮）。

### 测试

无后端改动；纯 UI 微调，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `<style>` 注入 + state + handleCopyDetail |
| **M2** | 完整描述 / 进度笔记 view 模式接入 copy 按钮 |
| **M3** | tsc + build + cleanup |

## 复用清单

- PanelChat 既有 `.pet-chat-row .pet-copy-btn` CSS hover 模式
- `navigator.clipboard.writeText` 在 PanelChat / PanelDebug 已用过

## 进度日志

- 2026-05-05 27:00 — 创建本文档；准备 M1。
- 2026-05-05 27:15 — 完成实现：
  - **M1**：`PanelTasks.tsx` 引入第一处 `<style>` 块（CSS hover-only 显隐 `.pet-detail-section .pet-detail-copy-btn`，与 PanelChat 既有 `.pet-chat-row .pet-copy-btn` 同模式：hover 整段渐显 + hover 按钮自身强化）；新增 `copiedDetailKey: string | null` 状态 + `handleCopyDetail(sectionKey, text)` async handler（clipboard.writeText + 1.5s 反馈）。
  - **M2**：完整描述段 / 进度笔记段（仅 view 模式 + content 非空）outer div 加 `className="pet-detail-section"`；标题旁内联 hover-only 复制按钮，已复制时绿字 + 强制 opacity=1 覆盖默认 hover-only 显示。sectionKey 用 `${title}-rawDesc` / `${title}-detailMd` 区分多任务多段。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 任务详情视觉补强。
  - **设计取舍**：CSS-driven hover 而非 React state-on-hover（后者 setState 抖动）；时间线段不加复制按钮（多行 list 单一 copy 语义不清）；编辑模式下进度笔记 textarea 已有原生选中-复制路径，不重复加按钮。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；CSS hover 模式与 PanelChat 既有副本同源，由 tsc 保证。
