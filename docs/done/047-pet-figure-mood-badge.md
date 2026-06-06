# 047 · pet figure mood 角标 — 让形象本体传达情绪

截图证据：宠物立绘是静态单一形象，所有 mood 信号都被压成文字 chip / PanelPersona section。GOAL「情绪价值 + UI 美观可爱」最直接的承载——宠物自己——反而最无表情。Live2D 不必上、立绘多版本不必出，最轻形态是 figure 右下角浮一个 mood emoji 角标。

需求：
- ChatMini pet figure 右下角浮一个 32px mood emoji 角标（😊 / 😴 / 🤔 / 😟 / 🥰 / 等），随 mood.rs 当前 mood 切换。
- 切换走 ≤ 300ms fade transition；不闪、不弹、不带 ripple。
- 严格视觉 cue：不带数字、不带 tooltip、不可点、不弹菜单 — 避免变 046 刚删掉的 dashboard chip。
- mood 映射表（mood-tag → emoji）做常量集中可扩展。
- 与 046 瘦身方向呼应：减信息密度的同时增情感密度，不堆 chip 改用 figure-borne cue。
- 没有 mood 数据（启动初 / mood.rs unavailable）→ 角标隐藏，不显默认 emoji。
- 与现有 figure 拖拽 / 右键菜单不冲突，角标位置在 figure 内坐标系。

---
实现笔记：
- 本刀仅 ship **backend slice**：UI 角标渲染 / fade / 位置布局是前端 ChatMini 改造，CLAUDE.md 要求 UI 改动需 dev server 浏览器测试——无可靠测试环境，留 frontend 工作给 user。
- `src-tauri/src/mood.rs` 新加：
  - `MOOD_EMOJI_TABLE: &[(&str, &str)]` 38 条中英 keyword → emoji 映射，按特异度排序（强负面 > 疲惫思考 > 强正面 > 平静 > 英文备用）
  - Pure `mood_to_emoji(text, motion)` case-insensitive 子串首条命中——混合「焦虑但还开心」取 😟（与 017 spirit 同：负面优先）
  - text + motion 都空 → None（spec「没有 mood 数据 → 角标隐藏」对应）；text 非空但无 keyword 命中 → 中性 fallback `🙂`（user 已显式 record，hide 反而违和）
  - `#[tauri::command] get_mood_emoji()` 异步壳：读 `read_current_mood_parsed()` → `mood_to_emoji()`；None 让前端隐藏
- 7 单测：空 input None / 负面优先 / 疲惫思考 / 正面 / 英文 case-insensitive / 无匹配中性 fallback / motion 槽参与扫描
- **缺口（frontend）**：
  1. ChatMini React 渲染右下 32px emoji 角标（`invoke('get_mood_emoji')` 拿 string | null）
  2. ≤ 300ms fade transition，不闪 / 不弹 / 不带 ripple
  3. 严格视觉 cue：不带数字 / tooltip / 不可点 / 不弹菜单
  4. None → 隐藏；Some → 显示
  5. 位置：figure 内坐标系右下，不冲突拖拽 / 右键菜单
  6. mood 切换时监听变化 event 后 re-invoke（或定期 poll）
