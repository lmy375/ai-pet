# PanelChat ⌘K task 引用选择器

## 需求

聊天中常要让宠物针对某条具体任务说话 / 行动，但目前用户得手敲任务
标题，宠物侧也得在 description 全文里模糊匹配，错位易发生。给一个
⌘K 召唤的轻量选择器：列当前 butler_tasks 队列，搜 + 键盘 nav + 选
中插入 ref token。

## 实现

`src/components/panel/PanelChat.tsx` 单文件：

### state + helpers

- `composeTextareaRef`：主 compose textarea 的 DOM ref（之前没拉，
  ⌘K 插入要读光标位置 + 恢复焦点）
- `taskPickerOpen / Tasks / Query / SelectedIdx` 四个 state
- `taskPickerInputRef`：picker 内 search input 的 ref
- `openTaskPicker`：异步 invoke `task_list`（既有后端命令），缓存到
  `taskPickerTasks` —— picker session 内不刷新，关再开重拉
- `insertTaskRef(title)`：从 textarea ref 读 selectionStart/End，把
  `「title」 `（全角直角引号 + 尾空格）splice 到光标位；setTimeout 0
  恢复光标到插入末尾 + focus

### 快捷键

`handleInputKeyDown` 加最前的拦截分支（优先级高于 image 历史菜单 /
slash 菜单 / Enter）：
- `(metaKey || ctrlKey) && !shiftKey && !altKey && key.toLowerCase() === "k"`
- `e.preventDefault()` 吃浏览器默认；调 `openTaskPicker()`

### UI

modal 走 fixed overlay 居中 480px × 70vh max，三段布局：
- 头：📎 引用任务 标题 + 全宽 search input（autoFocus）
- 体：filtered 列表，flex column scroll；每行 = status pill + title
- 脚：底部状态栏 "filtered / total · 插入格式：「任务标题」"

filter：纯前端 `title.toLowerCase().includes(query.toLowerCase())`，
空 query 列全部。selectedIdx 自动 clamp 到 `[0, filtered.length-1]`。

键盘行为（绑在 search input 的 onKeyDown）：
- ↑/↓：移动 selectedIdx，preventDefault 防输入光标乱跳
- Enter：插入当前选中 task → close
- Esc：close

鼠标行为：hover 行同步 selectedIdx（与键盘 nav 互不打断）；click 直接
插入 + close。backdrop click（外层 div onClick）= close + 恢复焦点
到 chat textarea。

### 插入 ref 形式

选 `「整理 Downloads」`（直角引号 + 全角，与 ASCII 不冲突）。设计原
因：
- 宠物侧 prompt 处理可用 regex `「[^」]+」` 抓任务 ref token
- 不与 markdown 的反引号 / 直角括号 / @-提到 冲突
- 对人眼视觉显眼，"特别引用"语义清晰
- 中文输入法本就常用，不需要 IME 切换

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 在 chat textarea focus 状态 ⌘K → modal 弹开，task 列出
  - 敲字过滤；Tab/Enter 顺序自然
  - 选中 Enter → `「任务标题」 ` 插入到 chat textarea 光标位 → modal 关
  - 焦点回 chat textarea，光标在插入末尾，用户可继续敲消息
  - 无 task 时 modal 显"（没有任务可引用）"，Esc 关
  - 多次 ⌘K 反复打开，每次重新 invoke 拉最新 task list
  - 模态打开时 chat 的 Enter / Shift+Enter / Esc 不会误触 send / clear —
    全部走 picker 内部 input 的 onKeyDown，没冒泡到 chat textarea

## 不在本轮范围

- 没做"模糊匹配"（typo-tolerant）：includes() 已经覆盖 95% 场景；
  fuzzy match 库（fuse.js 等）成本不值
- 没做"最近引用过的任务"置顶：picker 内 task 已按后端队列顺序（pending →
  done / archive），用户主动找通常按字面搜
- 没做 ⌘K 在 textarea 外（如 session list focus 时）拦截：scope 限
  textarea 内更安全，避免与系统 / 其它快捷键冲突
- 没做后端"看到 ref token 时怎么处理"：宠物侧 prompt 工程是单独
  iteration —— 本 UI 已经让用户能产出 ref token，后端处理逻辑后续接
- 没做 picker 内点击 detail 预览：picker 是"挑一条快进引用"，不是
  阅读视图；要看 detail 直接到 PanelTasks

## TODO 池剩余

空。下一轮需自主提需求。
