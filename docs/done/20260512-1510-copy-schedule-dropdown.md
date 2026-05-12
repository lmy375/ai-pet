# PanelMemory butler_tasks 新建 modal "从现有任务复制 schedule" 下拉

## 需求

新建 butler_tasks 时，模板按钮组（🔁 every / 📅 once / ⏳ deadline）
只插入占位文本如 `[every: 09:00] `，用户还得手敲具体时分。已有任
务里那些已经验证过的 schedule（"我已经知道每天 9:00 适合我"）应
该能一键复制。加下拉"📥 复制现有 schedule…"，列出 butler_tasks 已
有项目的标题 + 前缀，选中即插入光标位。

## 实现

`src/components/panel/PanelMemory.tsx`，编辑 modal 的 schedule 工具
栏行：

- toolbar 行 style 加 `flexWrap: "wrap"` + `alignItems: "center"`：
  下拉横向放不下时自动换行不挤掉模板按钮
- 在 3 个 SCHEDULE_TEMPLATES 按钮后追加一个 IIFE 渲染 `<select>`：
  - 从 `index.categories.butler_tasks.items` 提取所有含 schedule 前缀
    的项目：用正则 `^(\[(?:every|once|deadline):[^\]]+\])` 抓前缀字
    符串，再用既有 `parseButlerSchedule` 校验完整语法
  - `candidates.length === 0` 时不渲染下拉（全新用户没任何 butler_tasks
    时 dropdown 空选择器是噪声）
  - option label 形如 `整理 Downloads — [every: 09:00]`，value 是
    带尾空格的前缀（与 SCHEDULE_TEMPLATES 同形：`[every: 09:00] `）
  - onChange 调既有 `insertTemplate(v)` —— 与按钮走同一插入路径
    （cursor 替换 / 选区替换 / focus 回 textarea）
  - 选完立刻 `e.currentTarget.value = ""` 把下拉重置到 placeholder
    option，让用户能重复选同一条不被卡 stale state
- 不动 SCHEDULE_TEMPLATES 模板按钮 / 不动 parseButlerSchedule 实现 /
  不加新 state

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 全新用户（butler_tasks 空）打开新建 modal → 仅 3 模板按钮，无下拉
  - 已有一条 `[every: 09:00] 写日记` → 下拉显该 item 一条
  - 已有多条 → 下拉列全（不去重；标题相同前缀不同是常见，列出帮用户区分）
  - 选 `整理 Downloads — [every: 18:00]` → textarea 光标位插入 `[every: 18:00] `
  - 下拉自动重置到 placeholder；再选可重复插入
  - description 含 `[error: ...]` 但无 schedule 前缀的 item 不进 dropdown
    （parseButlerSchedule 必须真正命中 every/once/deadline 才入选）
  - 编辑现有 item 也能用 dropdown（不限 isNew）—— 想替换 schedule 时
    一样有用

## 不在本轮范围

- 没做"复制 description 整段（含 topic 部分）" —— 复制 topic 意味着
  用户在同一任务有两份内容，反而困惑；本轮仅复制前缀语法
- 没做"复制后是否清空已有前缀" —— 当前是插入光标位（与模板按钮同源），
  用户自己决定是否选中既有前缀再覆盖
- 没去重 prefix（多个任务同 `[every: 09:00]` 在下拉里列两次）—— 用户
  按标题区分，重复列出不会出错
- 没做下拉项排序（按 updated_at / 字典序）—— index 已按 backend 顺序
  返回；< 10 条用户能扫；规模大后再考虑

## TODO 池剩余

- PanelTasks header "今日 due" quick filter chip
- PanelChat ⌘K task 引用选择器
