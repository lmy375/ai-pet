# ChatPanel 输入框加「📜 复制全 input」按钮（iter #490）

## Background

ChatPanel input 已有：
- 📋 paste-as-plain（read clipboard → normalize → insert at cursor）
- 💡 sent history popover（recent 5 input 历史）

但缺反向 — **复制当前 input 内容**。owner 在写长 prompt 时担心：
- 误删（⌘A → ⌫ / accidentally ⌘Z 过多）
- 失焦丢失（窗口失焦后 timer 清空 input 等极少 edge case）
- 暂存草稿 audit / 切去回复别人后回来发现 input 已变

本 iter 加 📜 button — 一键复制当前 input 到剪贴板。

## Changes

### `src/components/ChatPanel.tsx`

紧贴既有 📋 paste-as-plain 按钮（right: 36）之后插，position right: 64：

```tsx
{input.trim().length > 0 && (
  <button
    onMouseDown={(e) => e.stopPropagation()}
    onClick={async (e) => {
      e.stopPropagation();
      try {
        await navigator.clipboard.writeText(input);
        console.log(`📜 已复制全 input（${input.length} 字）`);
      } catch (err) {
        console.error("copy input failed:", err);
      }
    }}
    title={`复制当前 input（${input.length} 字）到剪贴板 — 防误删 / 防失焦丢失 / 暂存草稿 audit。`}
    aria-label="copy full input to clipboard"
    style={{ position: "absolute", top: 6, right: 64, ... }}
  >
    📜
  </button>
)}
```

### Position layout 三 chip 各居其位

| Button | Right | When |
|---|---|---|
| 📜 copy-input | 64 | input non-empty |
| 📋 paste-as-plain | 36 | always |
| 💡 history | 8 | sentHistory non-empty |

各 22×22 圆形 + 6px gap，三按钮 vertically aligned top: 6。当 input 为
空时 📜 隐藏；sentHistory 为空时 💡 隐藏 — 但 📋 永显，让 paste 入口
始终可达。

## Key design decisions

- **`input.trim().length > 0` gate**：空 input 时 chip 隐藏避免「📜 已
  复制 0 字」无意义 click；trim 防纯空白也算 non-empty
- **`console.log` 而非 setMessage / toast**：ChatPanel 无 PanelDebug 那
  种 toast 通道；console feedback 已足够（剪贴板内容粘到任何地方都能
  验证成功）。owner 关心的是 "now I can ⌘V 回粘"，不需要视觉确认
- **不复用 sentHistory 添加路径**：sentHistory 是 onSend 后才 push；本
  chip 是 "send 前先备份草稿" 入口，不应混入 send-cycle 历史
- **right: 64 位置**：📋（right: 36）左侧 28px，让两按钮 + 💡（right:
  8）三个 22px 圆形 + 间距各居其位。横向布局简单清晰
- **不引 ⌘C / 不抢键盘事件**：textarea 原生 ⌘C 仅复制 selection；本
  chip 是「复制全文」语义，单击按钮 explicit
- **不写 unit test**：纯 clipboard write + state read；逻辑 trivial
  （既有 paste-as-plain 同 clipboard API pattern production 验证）。
  GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
- 后端无改动 — 纯前端 button
- 手测：ChatPanel input 输入「写一段长 prompt…」→ 右上 chip 行看到
  「📜 📋 💡」三按钮（input 非空 + sentHistory 非空时）→ 点 📜 → 切到
  其它编辑器粘贴 → 看到完整 input 文本；input 清空时 📜 隐藏（仅
  📋 显示）
