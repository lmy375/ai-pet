# PanelDebug 决策日志加 collapse toggle（Iter R146）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug 决策日志段加 collapse toggle：现是 always-on 区块；与 tool history / feedback section 不一致（那两段有 ▾ / ▸ 折叠按钮）。加同款 collapse，让 debug 时长 buffer 不抢屏。

## 目标

PanelDebug 内三个长 buffer 类区块对比：
- 工具调用历史（line 1942）：✅ collapse via `showToolHistory`，default `false`
- 反馈历史（line 2185 附近）：✅ collapse via `showFeedbackHistory`，default `false`
- 决策日志（line 1452）：❌ always-on 区块，最 16 条 + filter chips + 排序按钮，
  在 16 条满 buffer 时整段约 200px maxHeight 内滚动。debug 长会话场景下用户
  经常想"暂时折起来看下面别的状态"，目前必须靠浏览器 zoom 解决。

加同款 ▾/▸ toggle，统一交互。

## 非目标

- 不改 default 状态：决策日志是 debug 主信号，default `true`（展开）。tool /
  feedback 默认折叠是因为它们更次要，决策日志反过来。
- 不动 16 条 buffer 上限 / filter chips / 排序按钮 / 清空按钮（这些都在 header
  外或 header 内右侧，不动；wrap 的只是 chip 行 + 决策行 list）。
- 不动 maxHeight 200px 滚动行为（折叠时整段消失，不需要滚动）。

## 设计

### 状态

```tsx
// R146: 决策日志 collapse；default true（展开）—— 这是 debug 主信号
//   而非次要 buffer，与 tool/feedback 默认折叠不同。
const [showDecisions, setShowDecisions] = useState(true);
```

加在 line 127 (`showToolHistory`) 附近以保持状态分组连贯。

### Header 改造

当前 header 第一行 (line 1465-1468)：

```tsx
<span>
  最近 {decisions.length} 次主动开口判断（最新在
  {decisionsNewestFirst ? "顶部" : "底部"}）
</span>
```

改为可点击：包一层 `onClick` 切换 `showDecisions`，加 `cursor: pointer`，末尾追
加 ▾/▸ chevron。注意：title 段右侧已有 proactiveStatus span 与「清空」按钮 —
点击事件不能冒泡到清空按钮（清空已有 setTimeout armed 机制，stopPropagation
清空按钮的 onClick 即可，但更简单的做法是把 toggle 绑在 title span 自己上而非
整个 header div，这样清空按钮天然不受影响）。

```tsx
<span
  onClick={() => setShowDecisions((s) => !s)}
  style={{ cursor: "pointer", userSelect: "none" }}
  title={showDecisions ? "点击折叠决策日志" : "点击展开决策日志"}
>
  最近 {decisions.length} 次主动开口判断（最新在
  {decisionsNewestFirst ? "顶部" : "底部"}）
  {" "}
  {showDecisions ? "▾" : "▸"}
</span>
```

### Body 包裹

filter chips (line 1526) 到 decision list 末 (line 1903) 整段包入：

```tsx
{showDecisions && (
  <>
    {/* 现有 filter chip 行 */}
    {/* 现有 decision list */}
  </>
)}
```

注意：折叠时 proactiveStatus / 清空按钮仍可见（它们在 header 同一行，不在
wrap 范围内）。这是 **故意** —— 用户折叠后还能从 status 看到 last action，并
能直接清空。

### 折叠时 maxHeight 浪费

容器外层 `maxHeight: 200px` + `overflowY: auto`（line 1460-1461）折叠时仍
保留 padding 8px + header 一行 ≈ 28px 总高，远不到 200px，所以 maxHeight
不需要条件化。容器 padding 不变，与 tool / feedback 折叠状态高度一致。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 加 `showDecisions` state + header span onClick + body wrap |
| **M2** | tsc + build |

## 复用清单

- `showToolHistory` / `showFeedbackHistory` pattern (R4 / R6)
- ▾ / ▸ chevron 文案约定（与 line 1976 / 2185 一致）

## 进度日志

- 2026-05-11 03:00 — 创建本文档；准备 M1。
- 2026-05-11 03:20 — M1 完成：showDecisions state + header span ▾/▸ 切换 +
  body wrap；M2 tsc + build 通过。归档。
