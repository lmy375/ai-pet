# PanelDebug 调试上下文快照加 mute 段（iter #427）

## Background

PanelDebug 已有「📋 导出快照 MD」按钮（line 2324-2330）+
`buildDebugMarkdownSnapshot` useCallback 聚合：环境 / 任务状态 /
工具缓存 / 心情 motion / proactive 出口 / env 工具 / prompt tilt /
tone snapshot / 待审核 / 待提醒 / 工具风险偏好 / 宠物最近说 — 已
覆盖 TODO 文案 4 项中 3 项（tone / mood 衍生 / recent_speeches）。

唯一缺：**mute state**。owner 反馈 bug「pet 不主动开口」时这是关
键排查信号 — 是不是被 mute 了。本 iter 加 ## mute 段补齐。

## Changes

### `src/components/panel/PanelDebug.tsx`（buildDebugMarkdownSnapshot 内）

```ts
// 紧贴 tone snapshot block 之后插入：
{
  let muteMins = 0;
  if (muteUntil) {
    const t = Date.parse(muteUntil);
    if (Number.isFinite(t)) {
      const diff = t - Date.now();
      if (diff > 0) muteMins = Math.ceil(diff / 60_000);
    }
  }
  lines.push("", `## mute`);
  if (muteMins > 0) {
    lines.push(`- 状态: muted (剩 ${muteMins} 分钟)`);
    if (muteUntil) lines.push(`- until: ${muteUntil}`);
  } else {
    lines.push(`- 状态: 未静音`);
  }
}
```

deps array 加 `muteUntil`（已早于本 useCallback 声明，可直接 closure）。

## Key design decisions

- **inline 算 muteMins 而非 reference muteRemainingMins useMemo**：
  TDZ — muteRemainingMins useMemo 在本 useCallback 之后才声明，引
  用会在 render 时跑 deps array 时炸 ReferenceError（render 函数
  自上而下顺序求值；deps 在 useCallback 调用时就 eval，此时尚未
  到 muteRemainingMins 声明行）。inline 重算 4 行避坑
- **「未静音」也写入 snapshot**：owner reproduce bug 时排除假设
  也是有效信号；显式记 < no-info 强 — debug log 应明确而非省略
- **block scope `{ ... }`**：避免 `let muteMins = 0;` 污染外层
  block 变量空间；与既有 lines.push 段落风格相符
- **deps 加 muteUntil only**：muteMins 是 closure 内派生不需 dep；
  仅 muteUntil 是 useCallback 真正闭包捕获的外部 ref

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.41s)
- 后端无改动 — 复用既有 get_mute_until invoke + buildDebugMarkdown
  Snapshot 聚合 pipeline
