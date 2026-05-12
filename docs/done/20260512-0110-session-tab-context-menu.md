# PanelChat session tab 右键菜单

## 需求

session tab 栏（上一轮新加）只能点切换；想 rename / pin / 删除得回到
session header 点 dropdown → 在长 list 里找对应行 → 点编辑 / pin / 删除。
两步路径，常用操作浪费一次"开 dropdown"。在 tab 上加右键菜单。

## 实现

`src/components/panel/PanelChat.tsx`：

- 新 state `sessionTabCtxMenu: { id, title, pinned, x, y } | null`
- 新 useEffect 监听 mousedown + Escape 关菜单（与既有 picker pattern 同）；
  setTimeout(0) 挂 mousedown 避免触发的同次 click 同步关
- tab 按钮加 `onContextMenu` 写菜单坐标 + tab tooltip 末尾加"· 右键改名 /
  pin / 删除"提示
- 菜单 JSX 在 component 根（与 ImageLightbox 同层），position:fixed + clamp
  视口边界，4 项内容：
  - 📌 pin 置顶 / 📍 取消 pin（调既有 `handleTogglePinned`）
  - ✏ 改名…（关菜单后 `setShowSessionList(true)` + `startRename(s)` 让
    用户看到 dropdown 里的 inline 输入）
  - 📋 复制标题（writeText + exportToast 短反馈）
  - 🗑 删除…（红字，关菜单后 `setShowSessionList(true)` + `handleDeleteClick`
    让用户看到 dropdown 那条 "确定？" 5s armed 按钮）

## 设计选择

- rename / delete 不在右键菜单里完成最终操作：那两个的"防误触"路径（inline
  input edit / armed-once 5s）已经在 dropdown 里实现得很到位，复制一遍不
  如让菜单负责"唤起"，dropdown 负责"完成"。展开 dropdown 后用户已知道位
  置 + 看到对应控件，符合操作肌肉记忆
- 不重写 outside-click 关闭逻辑：单独的 useEffect，与既有 picker 们独立。
  taskCtxMenu / priority / status picker 都在 PanelTasks 里，PanelChat 这
  里第一次有 ctx menu，所以自起一份小 effect

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 右键 tab → 浮窗在鼠标位置弹出（贴边自动 clamp）
  - 点 📌 pin 置顶 → invoke + reload，tab 自动跳到 pinned 区
  - 点 ✏ 改名… → 菜单关 + dropdown 展开 + 对应行变成编辑 input
  - 点 📋 复制标题 → toast"已复制标题：..."
  - 点 🗑 删除… → 菜单关 + dropdown 展开 + 对应行删除按钮显"确定？"红
    填充，5s 内再点真删
  - 外部 click / Esc → 菜单关
  - 左键点 tab 仍是切换 session（onClick 与 onContextMenu 不互相打扰）

## 不在本轮范围

- 没在 tab 上加内联 hover-only 小 icon（如 ✏ / 🗑）：tab 已经够小，加按
  钮会挤；右键菜单作为单一入口语义更清晰
- 没改 dropdown 行的既有按钮：右键是补充入口，老路径不动

## TODO 池剩余

- PanelTasks detail.md markdown 预览
- PanelDebug 工具风险 inline 调整
