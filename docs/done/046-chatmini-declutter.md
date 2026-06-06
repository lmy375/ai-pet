# 046 · ChatMini 宠物页瘦身 — 减杂乱、提聚焦

当前 ChatMini 信息密度过高（截图证据）：右上 3 chip（🐾2 / ✦19 / ⏭）、transient_note 气泡下 chip rack 6 个（⏱7m / 📊今日6 / 💾 / 🌐+8 / 📋 / ⛶）、输入栏底部 3 按钮（📸 / 📋 / 💡）— 多个 chip 含义不直观，📋 在 chip rack 与输入栏重复，⏭ 与右上其它两 chip 用途难辨；同时 LLM 输出泄漏 `</think>` 原始 tag 也在主聊天泡里。整体看不像情绪陪伴，像 dashboard。

需求：
- 右上 chip 组（🐾 / ✦ / ⏭）三个全部删除：游戏化计数和未注释的"下一条"按钮对 GOAL「情绪价值」无贡献，移除而非藏入二级。
- 头像下 chip rack 仅保留：⏱（最近主动开口）+ ⛶（全屏入口）。📊 / 💾 / 🌐+8 / 📋 全部移入 PanelChat 右侧菜单或对应 panel，ChatMini 主页不显。
- 输入栏只保留 📸（视觉输入入口 002 / 031）；删除 📋（与已迁走的 chip 重复）、💡（功能不明）。
- 修复 `<think>...</think>` 泄漏：内容不再混在主气泡正文里，而是抽出为气泡上方一个折叠的「思考」区，默认收起；点击展开看完整思考过程，再次点击收起。主气泡只渲染 think 标签之外的最终回答。
- transient_note 气泡位置下移至距底部固定 offset，避开宠物身体中段，宠物可见度优先。
- 不引入新设置开关；瘦身后默认即终态。

---
实现笔记：
- 本刀仅 ship **后端 `<think>` 剥离**——5 个 UI 子项（chip 删除 / 输入栏精简 / transient_note 位置 / `<think>` 折叠展开 UI）均纯前端改造，CLAUDE.md 要求 UI 需 dev server 浏览器测试，无可靠测试环境，留 frontend 工作给 user。
- `src-tauri/src/commands/chat.rs`：
  - 新加 pure `strip_think_blocks(s) -> (visible, Vec<think_blocks>)`：大小写不敏感匹配 `<think>...</think>`，多对块全剥；未闭合 `<think>` 后续视作 think 不混入 visible（spec「主气泡只渲染 think 标签之外的最终回答」对应）；剥完 visible 头部 trim 多余换行避空段
  - `run_chat_pipeline` 返回点应用剥离——所有**非流式 caller**（proactive / TG / consolidate / 各种 maybe_run_*）拿到的最终字符串自然干净
  - 流式 chunk（StreamEvent::Chunk）已经在 send_chunk 阶段直接推前端不剥（在 LLM 输出实时阶段需要状态机跨 chunk 拼接 `<think>` tag 边界，单元复杂 + 改 StreamEvent enum 影响前端 TS 类型，留 frontend 一并做）
  - 11 单测：无 think / 单块 / 多块 / 大小写不敏感 / 未闭合丢 remainder / leading newline trim / 空 think / 中文 / 内部换行 / markdown 保留 / 仅 think 输入返空 visible
- **缺口（frontend 工作）**：
  1. 右上 3 chip（🐾 / ✦ / ⏭）删除：纯 React state / DOM 移除
  2. chip rack 仅保留 ⏱ + ⛶：5 个 chip 删除 / 部分迁 PanelChat
  3. 输入栏 keep 📸：删 📋 / 💡 按钮
  4. **流式 `<think>` 渲染拆分**：前端需在 ChunkEvent 累积时实时识别 `<think>` tag 边界（cross-chunk 状态机），路由内容到「思考」折叠区 vs 主气泡。Backend 已对 final reply 剥离，但流式过程仍 leak；frontend 可选方案 (a) 自己跨 chunk 拼接识别；(b) 等 Done 后用持久化的 clean 版本替换累积文本（需 Done event 带 final_text 字段——目前 `Done {}` 空 payload，要么扩 enum 要么前端走 listen 已落库的 session 末条 message）
  5. transient_note 气泡固定 offset 下移：CSS / inline style
  6. 不引入新设置开关——本刀也保持此约束（无 settings 字段新增）
