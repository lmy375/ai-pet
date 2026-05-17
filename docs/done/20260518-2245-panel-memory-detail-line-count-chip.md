# PanelMemory item hover preview 加「📊 行数」chip（iter #508）

## Background

PanelMemory item hover preview popover 已显 `📄 <relative path>`（iter
#501 改可点复制 abs）+ preview 截断文本。owner 想 "这条 detail.md 有
多长" 时只能：
1. 展开 item
2. 看顶部状态信息显字数
3. 心算字数 → 行数

或外部打开 → 数行。本 iter 在 hover preview 内加一个 「📊 N 行」chip
让 owner 即时拿到行数信号。

## Changes

### `src/components/panel/PanelMemory.tsx`

紧贴可点 `📄 <path>` 之后插入 inline chip：

```tsx
{previewText && previewText.length > 0 && (() => {
  const nlCount = (previewText.match(/\n/g) || []).length;
  const truncated = previewText.endsWith("…");
  const lines = nlCount + 1;
  if (lines < 20) return null;
  const label = truncated ? `📊 ≥${lines} 行` : `📊 ${lines} 行`;
  return (
    <div
      title={truncated
        ? `detail.md 至少 ${lines} 行（hover preview cap 600 字...）`
        : `detail.md ${lines} 行 — 长 doc 时考虑 ⌘⇧P heading palette ...`
      }
      style={{
        fontSize: 10,
        color: "var(--pet-color-muted)",
        marginBottom: 4,
        fontFamily: "'SF Mono', 'Menlo', monospace",
        userSelect: "none",
      }}
    >
      {label}
    </div>
  );
})()}
```

## Key design decisions

- **复用 previewCache 不引新 IPC**：行数计算从已 hover-trigger 加载的
  `previewCache[detail_path]` 来 — 零额外 IPC，instant render
- **「≥ N 行」前缀当 truncated**：`memory_read_detail` 在 ≥ 600 字时
  截断 + 加 `…` 尾。本 chip 检测到 `…` 时给下限暗示，避免误导
- **`lines < 20` gate**：短 doc（≤ 19 行）行数显示无 audit 价值；门槛
  20 是「需要 ⌘⇧P 跳转 / consolidate 判断」起点的经验值
- **位置 inline 而非独立 chip 行**：hover preview popover 空间紧凑，
  与 `📄 path` 同 fontSize/color/family — 视觉一致 metadata 行
- **tooltip 含 cross-reference**：truncated 文案点 hover cap = 600
  字、未截断文案点 ⌘⇧P heading palette 入口 — 让 owner 知道"行数大"
  之后有哪些工具可用
- **`previewText.match(/\n/g)`**：标准 JS regex global match — 命中
  数即 `\n` 数；行数 = `\n` + 1（最后一行无 `\n` 兜尾）
- **不写 unit test**：纯 regex count + 阈值；逻辑 trivial（既有
  detailSizes / detailMap pattern production 验证）。GOAL.md
  "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.28s)
- 后端无改动 — 复用 memory_read_detail 既有 IPC
- 手测：
  - 短 detail.md（≤19 行）→ hover preview 内无「📊」chip（gate 生效）
  - 中等长（20-60 行）→ 显「📊 N 行」灰字
  - 超长（≥ 600 字触发截断）→ 显「📊 ≥N 行」+ tooltip 解释截断
  - hover preview 关闭 → chip 一起消失（在 popover 内 scoped）

## Future iters (out of scope)

- 「📊 字数 + 行数」二合一 chip — 当前字数信号已分散在 sortByCharCount
  / 顶部状态行 / 📂 reveal 等多处，独立行数 chip 不引重复
- click chip 跳到 detail.md editor — 当前 ⌘⇧P heading palette + 双击
  title 已覆盖入口
