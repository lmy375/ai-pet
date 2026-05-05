# 决策日志"复制当前过滤"批量按钮（Iter R90）

> 对应需求（来自 docs/TODO.md）：
> 决策日志"复制当前过滤"：filter 行尾加按钮，把 filteredDecisions 按 `[ts] kind reason` 多行格式一键复制，贴 issue / 笔记分析多条事件不再要逐条点。

## 目标

PanelDebug 决策日志现在每行支持"复制"单条 `[ts] kind reason`（hover 出现），
但调试一组关联事件（比如"近 10 分钟所有 Skip"或"全部 LlmError"）时，得逐条
点 N 次再贴笔记，效率低。

加 1 个"批量复制"按钮：把当前过滤后（kind + 时间窗 + reason 三层 AND
之后）的 N 条决策按"每行一条"格式打包到剪贴板。复制顺序遵守
`decisionsNewestFirst` 显示序，让粘贴出去的列表与屏幕看到的一致。

## 非目标

- 不做"复制为 markdown 表格" / "复制为 JSON" 等多格式选项 —— 单一 plain
  text 行格式与现有"复制单条"一致，简单粘贴到 issue / 终端 / Slack 都
  友好；多格式 dropdown 增加心智负担
- 不做 deselect / 多选 checkbox —— 与既有"按过滤批量"语义一致；多选
  checkbox 是另一个交互维度（用于"挑出几条非连续的"）暂不需要
- 不写文件导出 —— 剪贴板足够。文件导出适合 ≥ 100 条的场景，决策 ring
  buffer CAPACITY=16 用不上

## 设计

### 位置

filter chip 行尾的现有控件序列：

```
[chips] [search input] [✕] (auto)→ [↑/↓ 最新在顶/底] [count: N/M · buffer N/16]
```

新按钮插在 sort 切换之后、count 之前：

```
... [↑/↓ sort] [📋 复制 N] [count]
```

视觉上紧贴 sort（同为 secondary action），不占 marginLeft auto 的 push 流。

### 行为

```ts
onClick: async () => {
  if (filteredDecisions.length === 0) return;
  const ordered = decisionsNewestFirst
    ? [...filteredDecisions].reverse()
    : filteredDecisions;
  const text = ordered
    .map((d) => `[${d.timestamp}] ${d.kind} ${d.reason}`)
    .join("\n");
  try {
    await navigator.clipboard.writeText(text);
    setCopyMsg(`已复制 ${ordered.length} 条`);
    setTimeout(() => setCopyMsg(""), 2000);
  } catch (err) {
    setCopyMsg(`复制失败: ${err}`);
  }
}
```

格式与单行复制完全一致（line 1638 附近）：`[ts] kind reason`，原始 reason
（不本地化）—— 贴 issue / 终端 grep 都用 ASCII 形式更易处理。

### 视觉

- `filteredDecisions.length === 0` 时按钮 disabled（cursor: default、bg/fg 走
  pet-color-bg/muted）；非 0 时正常 hover-able
- title 提示具体动作："把当前过滤后的 N 条决策按 `[ts] kind reason`
  多行格式复制到剪贴板"
- 复制成功 reuse 顶部 `copyMsg`（已是"近期复制状态"的统一通道）；2 秒自
  清空，与单行复制 1.5s 时长基本一致

### 测试

无单测；手测：
- 默认全选 → "📋 复制 16" → 粘贴出 16 行
- 切到"近 10m" 限定到 3 条 → "📋 复制 3" → 粘贴出 3 行
- 切 `decisionsNewestFirst` → 复制顺序翻转
- filter 命中 0 → 按钮 disabled，点击无副作用
- 不影响既有单行复制 / clear / sort 按钮

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 加按钮 + handler |
| **M2** | tsc + build |

## 复用清单

- 既有 `copyMsg` state + 自清空 timer 模式
- 既有 `filteredDecisions` useMemo（kind + 时间窗 + reason 三层 AND）
- 既有 `decisionsNewestFirst` 渲染序

## 进度日志

- 2026-05-08 18:00 — 创建本文档；准备 M1。
- 2026-05-08 18:08 — M1 完成。filter 行尾在"↑/↓ 最新在底"按钮之后、count span 之前插入"📋 复制 N"按钮：filteredDecisions.length === 0 时 disabled（bg=bg、color=muted）；非 0 时 hover-able。复制顺序遵守 decisionsNewestFirst（reverse 后再 join `[ts] kind reason\n`），让粘贴出去的列表与屏幕一致。复制成功 reuse 顶部 copyMsg "已复制 N 条"，2s 自清空。
- 2026-05-08 18:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (499 modules, 965ms)。归档至 done。
