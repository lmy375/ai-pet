# PanelDebugLogs 📜 复制 markdown 按钮（iter #442）

## Background

PanelDebug 既有 📸 抓快照 A 按钮 — dump 当前全状态（task counts / mood
overrides / tone snapshot / mute 剩余 / 决策日志计数等）— owner 写
bug report 时用。但 snapshot 是"现在这台机器状态如何"的截面 — 时间线
信号（最近 N 秒发生了啥 ERROR / WARN）走不进去。

LogStore（in-memory ring）含 1000 条最近日志 — 已有 PanelDebugLogs tab
渲成黑底 console 视图 + level + keyword 过滤。但 owner 想"把这段日志贴
到 GitHub issue / Notion"时要手动选中复制 + 自己排版表格。

本 iter 加「📜 复制 markdown」按钮 — 把当前 filteredLogs（应用了
level + keyword 过滤后）拼成 3 列 markdown 表（time / level / message）
一键复制；与 snapshot 互补 — 那个是截面，本按钮是时间线。

## Changes

### `src/components/panel/PanelDebugLogs.tsx`

#### 1. `handleCopyMarkdown` 闭包

输入 `filteredLogs`（已 level + keyword AND 过滤）；输出：

```markdown
## 应用日志 · N 条 [/ M total]
- filter: level=ERROR,WARN · kw=「foo」   (仅在 filter 激活时显)

| time | level | message |
| --- | --- | --- |
| MM-DD HH:MM:SS | ERROR | ... |
| ... |
```

- 时间：line.slice(0, 14) — 与既有 console 视图同 slice 协议
  （PanelDebugLogs.tsx:348）
- level：检测 rest 前缀 `ERROR` / `WARN` / 否则 `INFO`（与既有过滤逻辑
  同识别协议）
- message：去 level token + trim — pipe `|` 转义为 `\|` 防表格断裂
- header 计数：`N 条`（filteredLogs.length），filter 激活时加 ` / M total`
- header filter 标注：仅在 level filter 或 keyword 激活时插一行 `- filter: ...`
  让粘到 issue 的人一眼看到「这是过滤后子集」

空 `filteredLogs` → friendly `console.log` 不空写剪贴板。

#### 2. 按钮 UI

紧贴现有「清空」按钮，在 toolbar 内：

```tsx
<button onClick={handleCopyMarkdown} disabled={filteredLogs.length === 0}
        title={`把当前可见 ${filteredLogs.length} 条日志拼成 markdown
                表复制 …`}>
  📜 复制 markdown
</button>
```

- disabled 状态：`filteredLogs.length === 0` 时灰显 + opacity 0.5 +
  not-allowed cursor + title 改 "暂无可复制日志"
- title 解释清楚 markdown 表格式 + 与「📸 抓快照」互补关系
- 文案「📜 复制 markdown」与既有「📋 复制 logs 路径」「📋 复制 snapshot」
  「📋 复制 diff」一致 emoji + 动词模板

## Key design decisions

- **复制 filteredLogs（非全量 logs）**：owner 在 tab 里已用 level chips +
  keyword 过滤拣出关心子集 — 复制时再走全量等于丢弃 owner 已做的过滤。
  WYSIWYG 是更直觉的「复制可见」语义。filter 激活时 header 含 `/ M total`
  + filter chip 文案让粘到 issue 的人知道这是过滤子集
- **markdown 表非 raw lines**：raw 文本贴到 issue 渲不出结构；markdown
  3-col 表则在 GitHub / Notion / Bear / Obsidian 都渲成漂亮表格。pipe
  转义 `\|` 防 message 里含 pipe 撑断表格
- **不引 Tauri clipboard plugin**：navigator.clipboard.writeText 在 Tauri
  webview 中可用（与既有 PanelDebug `navigator.clipboard.writeText` 路径
  同源 — line 2320 复制 logs 路径用的也是同 API）
- **不显复制后 toast**：与既有 PanelDebug 其它复制按钮（snapshot / diff /
  logs path）同 — 仅 console.log 不弹 toast，靠 button 短暂闪烁 +
  clipboard 内容做反馈。pet 桌面没有统一 toast 通道
- **disabled 不藏 button**：空状态下 button 灰显 + tooltip 提示"暂无可复
  制" — 比"动态藏"更稳：button 位置稳定，owner 知道它在哪
- **不为 button 引 unit test**：纯 string-build + clipboard 副作用，逻辑
  在 `tsc` + `vite build` 通过即可保证；行为靠手测（点 button → 看
  console.log → 粘到 markdown 编辑器渲表格）。GOAL.md「meaningful tests」
  规则下，这个 button 不该引入装饰性测试
- **不动 PanelDebugLogs 现有 polling / followTail / scroll 逻辑**：本
  按钮纯增量；filteredLogs 已是 useMemo 缓存，复制不引新计算

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
- 后端无改动 — 复用既有 `get_logs` Tauri 命令读 LogStore
- 手测：点 📜 button → 切到 markdown 编辑器粘贴 → 看 3 列表渲出
