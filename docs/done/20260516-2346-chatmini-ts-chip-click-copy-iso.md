# ChatMini bubble 顶 ts chip click 复制完整 ISO timestamp

## 背景

ChatMini bubble 顶有 hover-only `[HH:MM]` 时间 chip + hover 显完整 ISO 文本的 title attr。owner debug 时常想"把这条消息的精确时刻 copy 到 bug report / chat / 日志"。但目前只能 hover 看 title tooltip 然后手动敲 ISO 字符串。

把顶 ts chip 改为可点 → click 复制 `m.ts`（raw ISO）到剪贴板 + 1.5s ✓ 反馈。

## 改动

### `src/components/ChatMini.tsx`

#### 1. 新 `tsCopyIdx` state（同 bubbleCopyIdx 模式）

```ts
const [tsCopyIdx, setTsCopyIdx] = useState<number | null>(null);
```

#### 2. ts chip 改 IIFE 含 click handler

```tsx
{hasValidTime && !hiddenTimestampIdx.has(idx) && (() => {
  const isTsCopied = tsCopyIdx === idx;
  return (
    <span
      className="pet-mini-row-time"
      title={`${formatFullTimestamp(m.ts)} · 点击复制完整 ISO timestamp 到剪贴板`}
      onClick={(e) => {
        e.stopPropagation();
        if (!m.ts) return;
        navigator.clipboard.writeText(m.ts).then(() => {
          setTsCopyIdx(idx);
          window.setTimeout(() => setTsCopyIdx((cur) => cur === idx ? null : cur), 1500);
        }).catch(err => console.error("ts chip copy failed:", err));
      }}
      style={{
        ...原 style + cursor: "pointer",
        color: isTsCopied ? "tint-green-fg" : "muted",
        fontWeight: isTsCopied ? 600 : undefined,
      }}
    >
      {isTsCopied ? "✓ " : ""}{timeLabel}
    </span>
  );
})()}
```

## 关键设计

- **复制 m.ts raw ISO**：而非 formatFullTimestamp 渲染串 —— 让 owner 拿到机器可解析的标准串（"2026-05-16T14:32:18+08:00"）。
- **e.stopPropagation 防 bubble click 路径误触**：iter #208 加了 ⌘+click 复制 bubble；ts chip 自身有 click，避免冒泡到 bubble。
- **1.5s ✓ 反馈**：与既有 bubbleCopyIdx 同 timeout 时长 / 同 pattern，视觉一致。
- **tint-green-fg 短暂上色**：与 ☑ checklist 全勾完同绿色 family。
- **不写新 toast**：✓ 字符变化 + 上色已足够 ack；toast 多余。
- **cursor: pointer**：让 chip 视觉 affordance 可点。
- **title attr 补 "点击复制" hint**：owner hover 自然发现可点交互。

## 不做

- **不复制 formatted display 文本**：raw ISO 更通用；owner 想要"格式化时间"已可在 title tooltip 看到再手抄。
- **不绑 ⌘+click 复制相对时间**：底部既有 ⏱ N 分前 chip (iter #195)；ts chip 与底相对 chip 各自单点击复制不同语义即可。本 iter 只做顶 chip 复制 ISO。
- **不写测试**：纯 click handler + navigator.clipboard.writeText API；视觉验证（hover ts chip → click → 短暂 ✓ + 剪贴板含 ISO）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.19s
- 改动 ~50 行（tsCopyIdx state 3 + ts chip IIFE 改写 35 + 注释）。既有顶 chip hover-reveal CSS / formatFullTimestamp / hiddenTimestampIdx 折叠 / 底 ⏱ 相对 chip 路径完全不动。

## TODO 状态

剩 5 条留池：
- PanelTasks 行 💤 snooze chip click 弹 snooze presets popup
- detail.md 编辑器 toolbar "📋 复制选中段 → 新 task"
- PanelMemory items hover preview "🔗 复制 detail.md path" 按钮
- PanelTasks "+ 新建" chip 显未读 / 错误任务计数
- pet 区右键加「📡 ping LLM 测延迟」

## 后续

- ⌘+click ts chip 复制本地 readable 时间格式（"2026-05-16 14:32 周五"），与单 click ISO 对偶。
- ts chip click 同款体验给底 ⏱ 相对 chip（点击复制 "5 分前" / 或同样 ISO）。
- PanelChat 消息 ts 列也加 click → 复制 ISO，让面板侧 debug 同样顺手。
