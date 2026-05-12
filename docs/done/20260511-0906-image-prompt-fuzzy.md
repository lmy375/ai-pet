# /image prompt 历史模糊匹配

## 需求

`/image ` 历史菜单只在 arg 为空时显。用户敲 `/image dr` 想找"dragon over moon"
得清空 arg 再选 → 失去半截输入；理想是边敲边过滤。

## 设计

regex 从 `/^\/image\s*$/i` 扩到 `/^\/image(?:\s+(.*))?$/i`，捕获 arg。空 arg
显全部 5 条；非空 arg 用 substring 大小写不敏感过滤。0 匹配时菜单自动隐藏，
不打扰用户继续 compose 新 prompt。

键盘交互区分 Enter / Tab：
- Enter = pick + 直接发（一气呵成；过去要"选 → 按 Enter 填 → 再 Enter 发"两次）
- Tab = pick + 填回输入框（让用户继续编辑微调）
- 鼠标 click = 填回输入框（同 Tab；不冒昧地代用户发）

## 实现

`src/components/panel/PanelChat.tsx`：

- regex match 给 `imagePromptArg`，trigger 条件改为"匹配到 regex"而非"输入恰
  好是 `/image`"
- 拆 `allImagePrompts`（原 imagePromptHistory state）和 `imagePromptHistory`
  （useMemo 过滤后的子集），filter 走 `text.toLowerCase().includes(q)`
- arg 变化的 useEffect 把 selectedImagePromptIdx clamp 回 0
- 键盘 Enter 分支：`setInput("") + executeSlash({ kind:"image", prompt, n:1 })` 直接发
- 键盘 Tab 分支：`setInput("/image " + picked)` 填回继续编辑

`src/components/panel/ImagePromptHistoryMenu.tsx`：

- header 文案改成"↑↓ 选 · Enter 直发 · Tab 填回继续编辑"反映新交互

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - `/image ` 空 arg → 5 条全显
  - `/image dr` → 仅含 "dr" substring 的项；选 Enter → 直接发
  - `/image violentXYZ`（不在历史）→ 0 匹配 → 菜单隐藏，用户继续 typing 不被打扰
  - Tab 选 → 填回 `/image <picked>`，光标停在末尾，Esc 清空，方向键正常移光标
  - mouse click → 填回继续编辑

## 不在本轮范围

- 没做"模糊"评分排序：当前 substring 包含即过 + 保留历史时间序。如果用户反
  馈"想按相关度排"再加 fuzzysort 类库
- 没把 -n 参数与历史绑：用户输 `/image -n 4 dr` 时 regex 也匹配（`-n 4 dr`
  整体作 arg 过滤），筛得偏。可接受 —— 大多数复用是单图；要回归到精确 prompt
  匹配，让用户去掉 -n 后再选
