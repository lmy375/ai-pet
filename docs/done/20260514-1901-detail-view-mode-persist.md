# 任务详情视图模式 localStorage 跨 session 持久化

## 背景

TODO 上 auto-proposed 一条："任务详情 edit/split/preview 视图模式 localStorage 跨 session 持久化：当前每次切任务都重置为『edit』，偏好 split 的用户得反复点。"

`detailViewMode` 是 detail.md 编辑器的三态切换（edit / split / preview）。重 detail.md 用户通常稳定偏好一种模式：

- 写日记 / 长 markdown：稳定 split（左输入 + 右渲染）
- 跨 review / 不改：稳定 preview
- 默认快速写：edit

但前一版用 `useState<DetailViewMode>("edit")` 每次组件 mount / 切任务 / 重启 panel 都 reset 到 edit。偏好 split 的用户每次都得点两次切换，体感不一致。

把它接进 localStorage 让"上次用哪个就这次用哪个"。其它面板偏好（如 detailMaxWidth / pinnedFilter / showFinishedDateSort 等）早已走 localStorage 同模式 —— 本 iter 是补齐遗漏。

## 改动

### `src/components/panel/PanelTasks.tsx`

```ts
type DetailViewMode = "edit" | "split" | "preview";

const [detailViewMode, setDetailViewMode] = useState<DetailViewMode>(() => {
  try {
    const raw = window.localStorage.getItem("pet-task-detail-view-mode");
    if (raw === "edit" || raw === "split" || raw === "preview") return raw;
  } catch {}
  return "edit";
});

useEffect(() => {
  try {
    window.localStorage.setItem("pet-task-detail-view-mode", detailViewMode);
  } catch (e) {
    console.error("detailViewMode localStorage save failed:", e);
  }
}, [detailViewMode]);
```

## 关键设计

- **严格 enum 校验 (`raw === "edit" | "split" | "preview"`) 而非 type cast**：localStorage 可能存了别的（用户手动改 / 老版本误写 / 跨 origin 污染）；严格比较后 fallback "edit" 保证 state 永远合法的 union 成员。
- **lazy initializer + useEffect 写回**：与既有 `detailMaxWidth` / `pinnedFilter` 等"跨 session 持久"模式完全对偶 —— 进来读一次、变化时写一次，无 race / 无 N+1 写盘。
- **fallback "edit"**：新用户 + 老用户首次升级时 key 不存在 → 走默认。"edit" 是最多数人初始预期（直接进编辑），不打扰新用户。
- **私密模式 / 容量满静默 fallback**：try/catch 包 localStorage 操作 —— Tauri WKWebView 上 localStorage 实际可用，但跨 origin / iframe 边界仍可能抛。失败不阻塞用户切模式，只是不持久。
- **不写 settings.yaml 系统设置**：模式是"小颗粒前端偏好"，与"主题 / SOUL.md" 等结构化设置不同；走 localStorage 与其它前端 chip filter / 列宽偏好同层级，settings.yaml 保结构化不被淹。

## 不做

- **不在「设置」面板暴露**：偏好太局部（仅 detail.md 编辑器），不值得专门一栏。owner 直接在编辑器切就行。
- **不写测试**：纯 useState lazy initializer + useEffect localStorage 写盘，逻辑 10 行；既有同模式偏好（detailMaxWidth / pinnedFilter / dueFilter）都无单测。
- **不区分跨任务持久 vs 单任务持久**：当前所有任务共享一份偏好。若用户需要"每个任务自己的视图模式"再扩 key 加 task title suffix —— 但 detail.md 工作流没有任何信号要求这种 split，YAGNI。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.22s
- 改动 ~20 行（lazy initializer + useEffect + comment）；既有 setDetailViewMode 调用点 / 三态切换 button / 渲染分支不变。

## TODO 状态

6 条候选 auto-proposed 已完成 2 条，余 4 条留池：
- session 下拉按月份分组折叠
- detail.md 打开自动滚到最新 `- [x]` 行
- 桌面 ChatPanel ⌘K 任务 ref picker
- TG /pinned 命令

## 后续

- 偏好 split / preview 但当前任务 detail.md 很短时自动 fallback edit —— 减少用户"我偏好 split 但这条只有 2 行没意义"的认知负担。复杂度上去，先观察用户反馈。
- "进入编辑总走 X 模式" 选项可在「设置」面板上线让 owner 显式知道 / 重置，但与 localStorage 路径冲突时合并语义需想清楚。
