# ChatMini bubble 加「📊 字数」hover chip（iter #541）

## Background

ChatMini bubble hover 已有两 ambient chip：
- 顶 `⏱ HH:MM` ts chip — 点复制 ISO timestamp
- 底 `⏱ N 分前` rel chip — 点复制相对时间

但缺**字数信号** — owner 看 long pet reply 想 audit「这段多长」/
「复制前预估字数」时只能心数 / 复制后看剪贴板长度。

本 iter 加顶 `📊 N` hover chip（与既有 ts chip 同 row 但位置在对侧 —
user 左顶 / assistant 右顶 — 让两顶 chip 不挤一边）。

## Changes

### `src/components/ChatMini.tsx`

#### CSS hover-reveal 类

紧贴 `.pet-mini-row-rel` 之后加：

```css
.pet-mini-row .pet-mini-row-chars {
  opacity: 0;
  transition: opacity 120ms ease-out;
}
.pet-mini-row:hover .pet-mini-row-chars {
  opacity: 0.5;
}
```

与既有 ts / rel 同 hover-reveal 模式，opacity 0.5 表 ambient 优先级
（比 ts 顶 0.55 略低，比 rel 底 0.5 同）。

#### JSX chip 紧贴 rel chip 之后

```tsx
{text && (() => {
  const chars = Array.from(text).length;
  if (chars === 0) return null;
  return (
    <span
      className="pet-mini-row-chars"
      title={`本 bubble 字数 ${chars} 字（Unicode code points）— 点击复制「${chars} chars」`}
      onClick={(e) => {
        e.stopPropagation();
        navigator.clipboard.writeText(`${chars} chars`)
          .catch((err) => console.error("chars chip copy failed:", err));
      }}
      style={{
        position: "absolute",
        top: -12,
        // 对侧 — user 左 / assistant 右（与 ts chip 反向）
        [m.role === "user" ? "left" : "right"]: 8,
        fontSize: 9,
        color: "var(--pet-color-muted)",
        fontFamily: "'SF Mono', 'Menlo', monospace",
        ...same chip base style as ts/rel...
      }}
    >
      📊 {chars}
    </span>
  );
})()}
```

## Key design decisions

- **位置在 bubble 顶对侧**：既有 ts chip 是 user 右顶 / assistant 左顶；
  本 chip 反向（user 左顶 / assistant 右顶）— 让两顶 chip 不挤一边，
  视觉对称
- **`Array.from(text).length` 计 Unicode code points**：直接 `text.length`
  会算 UTF-16 code units — emoji / 中文超出 BMP 时会 double-count。
  Array.from 迭代器拿 code points 准确（与 `[...text].length` 等价）
- **`text` truthy gate**：纯图 bubble / 空 content 没有字数概念，skip 渲
  染。复用既有 `text` variable（已在 bubble row scope 内可用）
- **0 chars short-circuit**：极端兜底防 chip 显「📊 0」无意义
- **click 复制「N chars」line**：与 ts / rel chip click 复制 pattern
  一致；line 简洁 — owner 想要 details 走 tooltip
- **hover-reveal opacity 0.5**：与 rel chip 同 ambient 信号 — 低优先
  级，不打扰主 chat 流
- **不写 unit test**：纯 conditional render + Array.from length；逻辑
  trivial（既有 ts / rel chip 同 render pattern production 验证）。
  GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.36s)
- 后端无改动 — 纯前端 hover chip
- 手测：
  - hover assistant bubble → 顶右看到「📊 N」chip
  - hover user bubble → 顶左看到「📊 N」chip（对侧 ts chip 在右）
  - 纯图 bubble hover → 无字数 chip（text gate 验）
  - click → 剪贴板含「N chars」
  - 含 emoji / 中文 bubble 字数与 Array.from count 一致（spec 而非
    UTF-16 code units）

## Future iters (out of scope)

- 「字数告警」≥ 阈值染红 — 与 ChatPanel 长 input chip 同 axis；后续
  按需
- 「token 估算」chip — 与 PanelDebug LLM token chip 同 heuristic 4
  chars/token；ChatMini 内当前不需精确
- 「按 \n 计行数」chip — bubble 多行场景较少，按需
