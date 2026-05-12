# PanelTasks tag 颜色自定义

## 需求

当前 tag chip 全走灰底 muted 字（`#f1f5f9 / #475569`），多 tag 任务里"哪条
属于哪类"靠用户读字。给用户加调色板，让 #urgent 红、#read 蓝、#weekend 绿
等，多任务列表一眼看归属。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 新 `TAG_COLOR_OPTIONS` 常量：7 项（default + blue / green / purple /
  orange / yellow / red），与既有 6 对 `--pet-tint-*-{bg,fg}` CSS var 一
  一对应，主题切换自动跟随
- 新 state `tagColors: Record<tagName, colorKey>`，初始从 localStorage
  `pet-tag-colors` 懒加载
- `setTagColor(tag, key)` 写新 map + JSON 同步进 localStorage；
  `colorKey === "default"` → 从 map 删除（cleanup，避免长期累积空值）
- `getTagTintStyle(tag)` 算 `{ background, color }` 用 CSS var 字符串模板，
  default / 无配色 → 返空对象（base style 接管）
- 新 state `tagColorPicker: { tag, x, y } | null`
- 既有"outside-click + Esc 关 picker"effect 把 tagColorPicker 纳进来
  （与 priority / status / taskCtxMenu 同模式）
- 两处 tag chip render 加 `onContextMenu`（preventDefault + setTagColorPicker
  写坐标）和 `{...getTagTintStyle(tag)}` 样式合并：
  - 顶部 tag filter row：selected（已筛选）态保 accent 蓝优先，unselected
    才叠 tint —— "已选"语义比"用户偏好色"重要
  - 每个 task card 的 tag chip：tag click stopPropagation 防误触行展开；
    所有状态都叠 tint
- 调色板 JSX 与 taskCtxMenu 同模式渲染在 component 根末尾，position:fixed
  坐标 + clamp 越界；7 个 24px 圆按钮（active 加 2px 实色边 + ✓ / default
  虚边 + ○）

## 不动后端

tag 颜色是 UI 偏好，不该写进 task 描述里的 `#tag` 字面量（那是给 LLM /
parse 用的 marker，加颜色后缀污染语义）。localStorage 与 memory pin 同模
式，per-machine。task 改名 / 删 tag 后 colors map 里残留无副作用，下次重
开 panel 想清还可以再右键改 default。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 顶部 tag filter row 任一 tag chip 右键 → 弹小调色板（7 圆）
  - 选"红" → chip 立刻变红底深红字；同 tag 的 task card 内 chip 也变红
  - 重启 panel → 颜色还在
  - 选"默认" → 回灰底（map 删除该 key）
  - selected 态（已点 chip 加 ✓）→ 蓝底优先；取消 selected 后看自定义色生效
  - 主题切深色 → 红色自动用 `--pet-tint-red-bg` 暗模式版（深棕底 + 浅粉字）
  - 右键越界（贴右下角）→ 调色板自动往内挪不溢出

## 不在本轮范围

- 没做"自定义 hex 输入"：6 色覆盖语义已足够（红 = 紧迫 / 蓝 = 阅读 / 绿 =
  完成 / 橙 = 提醒 / 紫 = 学习 / 黄 = 待思考），advanced 用户随取一档即用
- 没做"批量同名 tag 跨任务都改色"：颜色 map by tag name，本来就是全局生
  效，单条改一次到处看；不必再做"批量"按钮
- 没在 PanelMemory 的 #tag 区跟一套：那里 tag 语义与 task 重叠但渲染独立，
  后续若用户反馈"想统一"再做一个共享 colorMap

## TODO 池剩余

- PanelDebug LLM 日志增量加载
- /image 历史 prompt 菜单显缩略图
- PanelChat assistant 消息三键 reaction
