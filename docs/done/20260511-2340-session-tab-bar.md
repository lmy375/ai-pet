# PanelChat 顶部 session 横排 tab 栏

## 需求

PanelChat 当前的 session 切换走点 title → 下拉里点行的两步路径。多于 3-4 个
session 时常用切换麻烦。加横排 tab 栏（pinned 优先 + 最近活跃，cap 8 + 当前
必显），让常用会话一键即达；dropdown 保留作"全部"溢出入口。

## 实现

`src/components/panel/PanelChat.tsx`：

- 新 `MAX_SESSION_TABS = 8` 常量 + `tabSessions: SessionMeta[]` useMemo：
  - 复用 dropdown 的排序：`[...sessionList].reverse()`（newest first）→
    pinned 先 / unpinned 后
  - slice MAX_SESSION_TABS
  - 当前 session 不在 top 8 时，把 current 提到首位，其余整体右移 1 位
    保 cap —— 让用户即便在很旧的 session 里也能"看到自己当前在哪条"
- 在 sessionBar 下方插 tab 栏 JSX：
  - 仅 `!searchMode && tabSessions.length >= 2` 时显（单条 tab 无意义）
  - flex row + `overflowX: auto` 让超出 panel 宽度时横向滚动
  - 每个 tab 是 button：
    - active：accent 色字 + 加粗 + 下边 2px accent solid（覆盖父
      borderBottom，与下面 search panel / dropdown 视觉对齐成"现在停在这条
      tab 上"）
    - inactive：muted 字 + 弱边框
    - pinned 加 📌 前缀
    - title 截到 12 字符 + …，hover tooltip 显完整 + 条数
    - 同 id 不重 invoke（active 行 cursor: default）
  - 末尾"⋯ +N"按钮：当 sessionList 数 > tabSessions 时显，点击打开既有
    dropdown 看全集；点击逻辑直接 `setShowSessionList(true)`

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 单 session 时：tab 栏不显（仅 dropdown 入口可用）
  - 2-8 个 session：全显为 tab，无 ⋯
  - 9+ 个：显前 8 个 tab + "⋯ +N" 按钮
  - 点 ⋯ → 打开 dropdown 看全集
  - 点 tab → loadSession 切换；当前 tab 蓝色加粗高亮
  - pinned session 永远在 tab 栏最前 + 加 📌
  - 切到极旧的 session（dropdown 里 9 位之后）→ 该 session 出现在 tab 栏
    首位作为当前活跃；其余 tab 显前 7 个
  - 横向滚动顺畅（macOS 触控板横滑 / 鼠标拖滚条）
  - search mode 开时 tab 栏隐（让位 search panel）

## 不在本轮范围

- 没让 tab 栏跨主题 tint（如根据 last activity 着色）：增加状态复杂度，与
  既有 dropdown 行的视觉一致优先
- 没做 tab 拖拽 reorder：pin 已足够"常用置顶"，拖拽再叠会与 pin 顺序冲突
- 没做"双击 tab 重命名 / 右键关闭"：rename / pin / delete 都在 dropdown
  里完整，tab 栏只做"快速切换"职能保持单一

## TODO 池剩余

- PanelTasks origin 过滤 chip（搁置等后端 origin 模型补全）
