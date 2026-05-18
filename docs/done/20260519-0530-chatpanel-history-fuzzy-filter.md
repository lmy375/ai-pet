# ChatPanel input 历史 popover 加 fuzzy filter（iter #537）

## Background

ChatPanel 既有 💡 sent history popover（line 711+）— 显最近 5 条输入。
但 owner sentHistory 累积到 20+ 条后想找特定 prompt（「上次写周报怎么
开头的」/「那次 deploy 的 prompt」）只能：
1. 关 popover
2. 在 textarea 内 ↑↑↑↑↑... 翻历史（顺序遍历不能跳）

或滚 popover（仅 top 5 显，看不到第 6 条往后的）。

本 iter 加 fuzzy filter input — 输 keyword 实时 case-insensitive 子串
过滤，让长 history 中找 prompt 一步完成。

## Changes

### `src/components/ChatPanel.tsx`

#### 新 state

```tsx
const [historyFilterQuery, setHistoryFilterQuery] = useState("");
const historyFilterInputRef = useRef<HTMLInputElement>(null);
```

#### popover open / close 生命周期

- popover 关 → 清 query（避免下次打开 stale state）
- popover 开 → setTimeout focus filter input（让 owner 立即可输）

```tsx
useEffect(() => {
  if (!historyPopoverOpen) {
    if (historyFilterQuery) setHistoryFilterQuery("");
    return;
  }
  window.setTimeout(() => historyFilterInputRef.current?.focus(), 0);
  // ... 既有 close + Esc handlers
}, [historyPopoverOpen]);
```

#### Popover 渲染重构

```tsx
{(() => {
  const q = historyFilterQuery.trim().toLowerCase();
  const filtered = q
    ? sentHistory.map((entry, originalIdx) => ({ entry, originalIdx }))
        .filter((x) => x.entry.toLowerCase().includes(q))
    : sentHistory.map((entry, originalIdx) => ({ entry, originalIdx }));
  // 空 query default top 5；filter 命中 top 10 让 sample 略大
  const cap = q ? 10 : 5;
  const shown = filtered.slice(0, cap);
  return (
    <>
      <input
        ref={historyFilterInputRef}
        type="text"
        value={historyFilterQuery}
        placeholder="搜历史 prompt（kw 子串）…"
        onChange={(e) => setHistoryFilterQuery(e.target.value)}
        onKeyDown={(e) => { if (e.key === "Enter") e.preventDefault(); }}
        style={{ ... }}
      />
      <div style={{ ...header... }}>
        {q
          ? shown.length === 0
            ? `🔍 无匹配「${q}」`
            : `🔍 命中 ${filtered.length} 条「${q}」（显 ${shown.length}）`
          : `💡 最近 ${shown.length} 条输入`}
      </div>
    </>
  );
})()}
{(/* 再 IIFE 一次拿 shown for map */).map(({ entry, originalIdx }) => (
  <button
    key={originalIdx}  // key 用 originalIdx 保 filter 视图下 unique
    onClick={() => {
      setInput(entry);
      historyCursorRef.current = originalIdx;  // 仍按真实历史 index
      // ...
    }}
  >...</button>
))}
```

关键：

- **`historyCursorRef.current = originalIdx`** 而非 filter render
  index — 让 owner 后续在 textarea 内按 ↑↓ 翻历史时仍按真实历史顺序
- **key 用 originalIdx** — 唯一稳定
- **filter active 时 cap 10**：empty 时仍 default 5（与既有 UX 一致）；
  filter 时 fuzzy 命中通常稀少，cap 10 让命中数都看到

## Key design decisions

- **case-insensitive substring**：与 /find / /search_today 等其它
  fuzzy 入口同协议
- **Enter preventDefault**：filter input 内 Enter 不该触发外层 onSend
  / 不该关 popover — 让 owner 输完 kw 后用 click / Esc 收尾
- **Esc 仍关 popover**：filter input 内 Esc 默认会冒泡到外层 keydown
  listener → 关 popover；不需在 input 单独处理
- **auto-focus filter input**：popover 开后 setTimeout 立即聚焦 — owner
  最常见 next action 就是输 kw
- **清 query on close**：避免 stale state（owner 上次输 "周报"，下次
  打开看到空清单会困惑）
- **`(() => { ... })()` IIFE 重算一次**：filter 计算在 header IIFE +
  map IIFE 内分别执行 — 双倍 cost 但只在 render 时 O(N) 扫，N typical
  < 50；保 render 代码扁平 + 不引外层闭包变量
- **`historyCursorRef.current = originalIdx`**：保 ↑↓ navigation 一
  致 — filter close 后 owner 按 ↑ 应该从「真实历史 N 条」位置开始，
  不是「filter view 的 N 条」位置
- **不写 unit test**：纯 React state + filter substring + render；逻
  辑 trivial（既有 sentHistory popover 同 click→setInput 路径 production
  验证）。GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.31s)
- 后端无改动 — 纯前端 filter
- 手测：
  - 💡 chip 点开 → popover 顶部看到「搜历史 prompt」filter input + 自动
    focus
  - 输 "周报" → header 显「🔍 命中 N 条「周报」」+ 仅显含此 kw 的
    entries
  - 清 input → 回 default top 5
  - 选某条 click → input 灌入 + popover 关 + filter query 清掉
  - 关 popover（Esc / click outside）→ 下次打开 filter query 已清空

## Future iters (out of scope)

- 「↑↓ 在 popover 内 navigate matches」+ Enter pick：当前需 click 或
  关 popover 后 textarea 内 ↑；后续 keyboard 化
- 「filter 高亮命中字符」（黄底）— 与 /find 命中 hi-light 同协议；当
  前命中 entry 整条显，命中位置不强调
- 「持久 filter history」— 当前 query 仅 session 内；persisted query 后
  续按需
