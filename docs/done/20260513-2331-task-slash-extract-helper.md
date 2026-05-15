# 抽 `taskSlashHelpers` 共享 fuzzy 解析逻辑

## 背景

前三轮分别加了 PanelChat 的 `/done` `/cancel` `/retry`，三个 case body 现在 ~145 行近完全雷同：拉 `task_list` → fuzzy 匹配（exact / substring）→ 0/1/多分支处理 → 反馈文案。

差异只 4 处：status 过滤谓词、backend 命令名 + extra args、成功反馈图标 + 动词、未找到 / 多命中文案后缀（retry 有"Error 任务"字样）。

第 3 份雷同时抽 helper 是 GOAL.md "代码越少越好" 偏好的合理触发点。

## 改动

### 新增 `src/components/panel/taskSlashHelpers.ts`

纯 TS 模块，无 React / Tauri 依赖（match + format 都是纯函数；fetch 由 invoke 注入）：

```ts
export type TaskResolution =
  | { kind: "found"; title: string }
  | { kind: "none" }
  | { kind: "multi"; candidates: string[] };

/// 在 titles 列表上对 query 做 fuzzy 匹配。优先 exact，其次 case-insensitive
/// substring。多个 substring 命中时返回全候选。caller 负责 status 预过滤。
export function matchTaskByQuery(query: string, titles: string[]): TaskResolution;

/// 多命中候选列表渲染。最多 5 条 `· title`，> 5 条时追加 `…还有 N 条`。
/// `domainHint` 用于"匹配到 N 条<X>任务"中的 X（"" 或 "Error "）。
export function formatMultiHitMessage(query: string, candidates: string[], domainHint: string): string;
```

### `src/components/panel/PanelChat.tsx`

`executeSlash` 三个 case 改为：

```ts
case "done": {
  try {
    const resp = await invoke<{ tasks: Array<{ title: string; status: string }> }>("task_list");
    const titles = resp.tasks.map((t) => t.title);
    const res = matchTaskByQuery(action.query, titles);
    if (res.kind === "none") {
      pushLocalAssistantNote(`⚠️ 没找到匹配 "${action.query.trim()}" 的任务。/tasks 看完整列表。`);
      break;
    }
    if (res.kind === "multi") {
      pushLocalAssistantNote(formatMultiHitMessage(action.query.trim(), res.candidates, ""));
      break;
    }
    await invoke<void>("task_mark_done", { title: res.title, result: null });
    pushLocalAssistantNote(`✓ 已标 done：${res.title}`);
  } catch (e) {
    pushLocalAssistantNote(`/done 失败：${e}`);
  }
  break;
}
```

cancel / retry 同模板；retry 在 `matchTaskByQuery` 之前先 `filter(t => t.status === "error")`，0 命中时文案多一句"Error 任务（/retry 仅作用于 Error 状态…）"。

case body 从 ~45 行各自缩到 ~17 行；3 个 case 总行数从 ~135 → ~55，加上 helpers 模块 ~30 行 → 净减 ~50 行。

### 不做

- 不抽 `dispatchTaskCommand` 一站式 helper：参数 6+（filter + invokeName + extraArgs + successPrefix + notFoundMsg + failurePrefix）后调用方更绕。stop at fetch+match 这一层。
- 不在 helpers 里 import `invoke` —— 让 caller 注入 fetch 结果，helpers 保持纯函数（可单测）。
- 不写单测（前端无 vitest，且 helpers 是从已经过实战的 case body 抽出 —— 行为等价回归由 tsc + 手动验收覆盖）。

## 验收

- `npx tsc --noEmit` ✅
- `/done <部分标题>` / `/cancel <部分标题>` / `/retry <部分 Error 标题>` 行为与上 3 轮完全一致：唯一命中 → 成功反馈；0 命中 → ⚠️；多命中 → 候选列表
- retry 0 命中文案仍含"Error 任务（…仅作用于 Error 状态…）"

## 完成

- [x] taskSlashHelpers.ts 新增
- [x] PanelChat.tsx 三 case 改用 helpers
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
