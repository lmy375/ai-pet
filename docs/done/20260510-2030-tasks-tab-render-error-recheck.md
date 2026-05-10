# PanelTasks 任务页渲染出错二次审阅

> 对应需求（来自 docs/TODO.md）：
> PanelTasks 任务页依然渲染出错，修复。

## 现状

`TabErrorBoundary`（commit e054374）已就位，渲染出错时会用红框展示错误
信息 + 可重试按钮。用户报"依然渲染出错"说明 boundary 仍在触发。

## 本轮做的事

1. 跑通 tsc / vite build / cargo check / cargo test (892 passed) —— 静态层无错。
2. 全文通读 PanelTasks.tsx + useTaskKeyboardNav.ts，重点找：
   - 未守卫的属性访问（t.body / t.tags / t.updated_at 等）
   - Date.parse 出错路径
   - hooks ordering / 条件 hook 调用
   - useMemo / useEffect deps
   - 最近改动（handleMarkDone 接入 hook）
3. 委派 Explore subagent 同样审阅，归并三组高 / 中信任候选，未发现明
   确可重现的 throw 点。
4. 后端 build_task_view 返回的 TaskView 字段全是 String / 已知枚举，
   不会发出 null/undefined 进字段。

## 仍未定位

唯一一致的可能性：用户本地数据里有某条 butler_task 形态特殊（updated_at
/ created_at 缺失或非 RFC3339）。但 backend 由 memory_edit 写盘，只产出
合规 ISO，需要历史脏数据才能进入这个分支。

## 等待用户提供

Boundary 红框里展示的 `error.message` + stack 第一行。一行就够定位到具
体源码位置。下一轮拿到信息后再针对性修。

## 不打算的改动

不做"防御式覆盖一切" —— 那会污染原本明确合约的代码（CLAUDE.md：don't
add error handling for scenarios that can't happen, trust internal code
guarantees）。需要看到实际错误再针对性修。
