# detail.md 编辑器加「⌘⇧I 插完整 ISO 8601 时间戳」快捷键（iter #520）

## Background

既有 ⌘⇧D 插 `MM-DD HH:MM`（紧凑短戳，progress note 场景）。但缺
**精度版** — owner 在 cross-tool 引用 / 日志相关性 / timestamp diff
计算 / 录决策时刻能精到秒等场景需要完整 ISO 8601 含 tz 时间戳。

本 iter 加 **⌘⇧I** — 插 `YYYY-MM-DDThh:mm:ss+TZ:00` 格式（与
JSON.stringify(new Date()) 行为兼容，含 local timezone offset 显式）。

## Changes

### `src/components/panel/PanelTasks.tsx`

新 `handleDetailIsoTimestamp` callback（紧贴 `handleDetailDateStamp`
之后）：

```tsx
const handleDetailIsoTimestamp = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (!e.shiftKey || e.altKey) return false;
    if (e.key.toLowerCase() !== "i") return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    e.preventDefault();
    const now = new Date();
    const y = now.getFullYear();
    const mo = String(now.getMonth() + 1).padStart(2, "0");
    const d = String(now.getDate()).padStart(2, "0");
    const hh = String(now.getHours()).padStart(2, "0");
    const mm = String(now.getMinutes()).padStart(2, "0");
    const ss = String(now.getSeconds()).padStart(2, "0");
    // tz offset：getTimezoneOffset 返「UTC - local」分钟数 — 与 ISO
    // 显示方向相反。东八区 -480 → 显 `+08:00`。
    const offMin = -now.getTimezoneOffset();
    const sign = offMin >= 0 ? "+" : "-";
    const abs = Math.abs(offMin);
    const offH = String(Math.floor(abs / 60)).padStart(2, "0");
    const offM = String(abs % 60).padStart(2, "0");
    const stamp = `${y}-${mo}-${d}T${hh}:${mm}:${ss}${sign}${offH}:${offM}`;
    insertMarkdownAtCursor("wrap", stamp, "");
    return true;
  },
  [insertMarkdownAtCursor],
);
```

#### 接入 onKeyDown 链

两个 textarea（split 模式 + edit-only 模式）都在 `⌘⇧D` 之后立即接入：

```tsx
if (handleDetailDateStamp(e)) return;
// ⌘⇧I 插完整 ISO 8601 时间戳（含秒 + tz offset）— 与 ⌘⇧D MM-DD HH:MM
// 紧凑版互补。
if (handleDetailIsoTimestamp(e)) return;
```

#### Keyboard help modal 新一行

紧贴 `⌘⌥L` 之后：

```tsx
["⌘⇧I", "插完整 ISO 8601 时间戳（YYYY-MM-DDThh:mm:ss±tz）— 精度版 ⌘⇧D"],
```

## Key design decisions

- **modifier ⌘⇧I**：⌘I 已是 italic；行业 IDE 「Insert ISO Timestamp」
  常见绑 ⌘⇧I — owner 心智匹配。preventDefault 吃 browser 默认（部分
  OS / browser 是「页面信息」/「inspect」— Tauri webview 已禁，但兜
  底）
- **手算 tz offset 而非用 `toISOString()`**：toISOString 永远返 UTC `Z`
  尾巴 — 让 paste 后看不到「这是本地时刻吗」直接信号；含 `+08:00` /
  `-05:00` offset 明确 owner 当前 tz 上下文 — 与既有 created_at /
  updated_at 字段 ISO 协议一致
- **与 ⌘⇧D 精度互补不替代**：紧凑 `MM-DD HH:MM` 仍是 progress note 主
  场景（同年内 year 上下文 obvious）；完整 ISO 是 cross-tool / 跨年
  / 高精度场景 — 两 shortcut 共存让 owner 按需选
- **`insertMarkdownAtCursor("wrap", stamp, "")`**：复用既有插入 helper
  — 与 ⌘⇧D / 工具栏 📅 button 同 cursor / undo / dirty-flag 协议
- **不写 unit test**：纯 String.padStart + Date 字段算术；逻辑 trivial
  + 单元化（既有 ⌘⇧D 同 helper production 验证）。GOAL.md "meaningful
  tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.28s)
- 后端无改动 — 纯前端 keyboard shortcut
- 手测：
  - detail.md 编辑器内 ⌘⇧I → 插 `2026-05-19T01:35:42+08:00`（或 owner
    本地 tz）
  - ⌘⇧D 仍正常插 `MM-DD HH:MM`（两 shortcut 共存）
  - ⌘/ 键盘帮助 modal 看到新「⌘⇧I」行
  - 跨 split / edit-only 模式都触发

## Future iters (out of scope)

- 「⌘⇧⌥I 插 UTC ISO」（含 `Z` 尾）— 给 SRE / 跨时区团队场景；当前
  local + offset 已是 ISO 合规
- 「插当前 unix epoch ms」（`1747626900000`）— 不同精度需求；后续 propose
