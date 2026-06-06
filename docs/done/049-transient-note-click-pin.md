# 049 · transient_note bubble 点击 pin — 桌面侧 /here_pin 对偶

截图证据：transient_note bubble 显示在 ChatMini pet figure 下方（"用户刚回来不久，还在Slack上忙工作..."），但目前仅显示、不可点。user 觉得内容值得长期记下来时只能切到 TG 用 `/here_pin`，桌面就在眼前的 bubble 反而无入口。

需求：
- transient_note bubble 加 click 行为：点击弹一个轻浮层「记一下这条? ［记下］ ［编辑后记下］ ［算了］」。
- 「记下」→ 落 PanelMemory 一条常规 entry，cat 由 LLM best-match，原 transient_note 淡出（生命周期结束 / 不等自然过期）。
- 「编辑后记下」→ 浮层切 inline 编辑 input，确认后落 entry；取消回浮层初态。
- 「算了」→ 关浮层，transient_note 保留至自然过期，不影响现有 lifecycle。
- 与 TG `/here_pin` 完全对偶：同一通路落库、同 source 字段标记、不重复落两条。
- 不引入新 panel；浮层走与 030 reminder snooze 浮层同一组件，避免新 UI 元件。
- pin 失败（LLM cat 选不出 / 写库异常）→ 浮层降级显示「记不下，再试一次」按钮，不静默吞错。

---
实现笔记：
- 本刀 ship **backend Tauri command**——前端 click 行为 / 浮层（共用 030 snooze 浮层组件）/ inline 编辑 input / 复用 reminder snooze 浮层视觉 都是纯 React 改造，留 frontend 工作给 user。
- `src-tauri/src/proactive.rs` 新加：
  - `#[tauri::command] pin_transient_note(edited: Option<String>, category: Option<String>) -> Result<String, String>`：edited None → 用当前 transient_note；edited Some(s) 空白 → Err；category None / 非白名单 → fallback 「ai_insights」；memory_edit 成功才清 transient_note（避免「清掉但没落库」）；返新 entry title 给前端展示「✓ 已记下『xxx』」
  - **不重复落两条**（spec 硬约束）：memory_edit「create」遇同 title 已存在 → 视作 idempotent 成功，仍清 transient_note 返同 title（识别 error 文本「已存在 / duplicate / exists」关键词）
  - Pure `format_transient_pin_title(text)`：首 30 char + `…` 若超；按 char 计而非 byte（中文安全）；空 / 全空白 → 兜底 `"transient note"`
  - Pure `format_transient_pin_description(text)`：前缀 `[source: transient_note_pin]` marker（spec「同 source 字段标记」对应）
  - `TRANSIENT_PIN_ALLOWED_CATEGORIES` 白名单 (ai_insights / user_profile / general / todo)——非命中值 fallback 防 LLM / 前端拼错 cat 名 panic
- 7 单测：title 短文本 verbatim / 长截断 + … / trim 空白 / 空兜底 / 中文 char 计算（不切 UTF-8 byte）/ description marker 前缀 / description 内部空白 trim
- **Frontend gaps**（user 接力点）：
  1. ChatMini transient_note bubble click handler 弹浮层「[记下][编辑后记下][算了]」三按钮
  2. 浮层共用 030 reminder snooze 浮层组件（spec「避免新 UI 元件」）
  3. 「记下」→ `invoke('pin_transient_note', { edited: null, category: null })`
  4. 「编辑后记下」→ inline input → `invoke('pin_transient_note', { edited: <user text>, category: null })`
  5. 「算了」→ 关浮层不调 backend；transient_note 保留自然过期
  6. 成功（Ok title） → 浮层 fade out + bubble fade out（spec「原 transient_note 淡出 / 生命周期结束」）
  7. 失败（Err msg） → 浮层显示「记不下，再试一次」按钮（spec 反指令「不静默吞错」）
- **LLM best-match cat 缺口**：spec「cat 由 LLM best-match」未做——backend 默认走「ai_insights」（pet 学到的"用户处于何状态"自然归到此 cat）。前端可加单独 LLM 调用先 classify cat 再传给 `category` 参数，或后续单独刀加 backend 路径。
