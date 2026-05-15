# PanelTasks 搜索框历史 datalist

## 背景

PanelMemory 搜索框有 datalist + localStorage history（最近 5 条 keyword，handleSearch 成功后入栈）。PanelTasks 搜索框是 live filter（无 search 按钮），缺这套自动补全。

加 search history：Enter 时（"我用这条 query 用得满意"的显式信号）入栈；datalist 浮在 input 下提供自动补全。

## 改动

`src/components/panel/PanelTasks.tsx`：

- 加 state `taskSearchHistory` (cap 5)，初始化从 localStorage `pet-tasks-search-history` 读
- `pushTaskSearchHistory(kw)`：trim + dedup move-to-front + cap 5 + 写盘
- 主搜索 input 加 `list="pet-tasks-search-history"`
- `onKeyDown` 加 Enter 分支：非空 query Enter → pushTaskSearchHistory
- 渲染 `<datalist>`（仅 history 非空时）
- placeholder 多一句 `· Enter 入历史` 让用户知道有这个能力

## 不做

- 不在 blur / debounce 时入栈：避免半敲完的 query 污染历史
- 不动归档搜索（archiveQuery）：那是临时回看场景，历史 5 条多半噪音 > 信号
- 不写测试：纯 localStorage 字符串处理 + native datalist；项目无 vitest

## 验收

- `npx tsc --noEmit` ✅
- 任务面板搜索框敲完 query 按 Enter → 后续聚焦看到 5 条 history 下拉
- Esc 清 query 行为不变；live filter 行为不变（onChange 仍即时）
- 关闭面板 / 重启 app → history 仍在

## 完成

- [x] PanelTasks.tsx: state + pusher + datalist + onKeyDown Enter
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
