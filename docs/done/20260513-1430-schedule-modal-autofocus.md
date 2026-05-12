# 改 schedule modal kind 切换后自动 focus

## 需求

iter #232 给 schedule modal 加了 kind 切换下拉。切换后用户得手动点
date / time 输入框开始改 —— 多一次点击。补 autofocus：

- kind=every → focus time（only field）
- kind=once / deadline → focus date（first field to fill）

## 实现

`src/components/panel/PanelMemory.tsx`：

- 新 ref `editScheduleDateRef` + `editScheduleTimeRef`，attach 到对应
  input
- 新 useEffect 监听 `editScheduleDraft?.kind` 和 `editScheduleDraft?.title`
  （title 变化代表新 task open，需要重 focus）
- setTimeout 0 等 React commit（date 在 kind=every 时 conditional 撤掉，
  立即 focus 拿 null）
- kind=every → focus time；else → focus date

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 点 [every: 09:00] 任务的 ✏️ → modal 打开 → time 输入框焦点
  - 切到 once → date 输入框焦点
  - 切到 deadline → date 输入框焦点
  - 切回 every → time 输入框焦点
  - 关 modal 再打开 → 同步重 focus
  - 不影响保存路径

## 不在本轮范围

- 没做"自动 select date input 全文"（避免每次 focus 时光标位置不变）：
  date input 通常用户用 picker 不需要 select；time input 数字键盘
  操作即可
- 没做"Enter 提交"快捷键：modal save 按钮可点；keyboard submit 用户
  期望不强

## TODO 池剩余

- PanelMemory butler_tasks item "⏰ 下次触发：X 后"
- PanelChat "↩️ 快速 follow-up" 下拉
- PanelDebug "📥 全部 stash JSON" 按钮
- PanelTasks priority badge 右键菜单
