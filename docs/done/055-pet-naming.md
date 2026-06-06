# 055 · pet 取名 — 从 generic「小宠物」到「我的 X」

截图证据：窗口标题「Pet - Panel」、pet 自我介绍「主人你的小宠物呀」— pet 没有 user 命名的专属身份。GOAL「自我进化 / 情绪价值」最基础的身份承载缺位，pet 永远是 generic AI 助手而非「我的小宠物 X」。

需求：
- PanelSettings / PanelPersona 中加「宠物名字」字段，初始未设；设定前 pet 自然回避（用户问"你叫什么"时回"我还没有名字，主人想叫我什么呀？"）。
- 名字落 settings 持久化；所有 LLM prompt 头部注入「pet name = X」，pet 在 self-reference / 自我介绍 / 029 self_note / 021 mood 周报 / 034 surprise 等场景统一使用此名。
- 桌面与 TG 共用同一 settings name（单 store，不引入多端 sync）。
- 用户首次设名 / 改名后，pet 在下一次自然 utterance 中自发提一句「从今天起我叫 X 啦」一次性确认，不强制每次启动 echo。
- 删名（清空字段）→ 回到"还没有名字"形态；不留旧名残影在 prompt。
- 047 mood 角标 / 054 idle 动画不依赖名字（互相独立）；与现有 PanelPersona「当下心情」/「常用工具」/「沟通偏好」并列新「身份」section。
- 不引入"宠物姓"/"昵称"/"全名"多字段；只保留一个 name 简化心智。

---
实现笔记：
- AppSettings 新加 `pet_name: String`（与既有 `user_name` 平行；空 → 未取名）
- `commands/chat.rs::format_persona_layer(days, persona, mood_trend, user_name, pet_name)` 签名加 `pet_name`：
  - 非空 → "你的名字是「X」——self-reference / 自我介绍时用这个名字"
  - 空 → "你还没有名字——主人还没给你取。被问到「你叫什么」时柔和回避并邀请主人取名（例：「我还没有名字，主人想叫我什么呀？」），不要自己编一个" 反指令
  - 顺序：pet_name → user_name → 陪伴 → persona → mood_trend → 尾导
  - `build_persona_layer_async` 直读 settings 拿 user_name + pet_name
- `proactive/prompt_assembler.rs::PromptInputs` 加 `pet_name: &'a str`；`build_proactive_prompt` 在 companionship 后注入同款 framing
- `proactive.rs::run_proactive_turn` 构造 PromptInputs 时同步取 pet_name
- 桌面 + TG 共用 settings（spec「单 store」自然满足）
- 既有 6 chat.rs 测试加第 5 arg + 4 新 GOAL 055 测试（非空注入 / 空邀请 + 反指令 / pet 先 user 后 / 全空白视未设）；3 个 proactive prompt 测试；blank_inputs_still_safe block_count 从 3 → 4（含常驻邀请行）
- **缺口**：
  1. **首次设名 / 改名 one-shot 确认**（spec「pet 在下一次自然 utterance 中自发提一句『从今天起我叫 X 啦』」）未做——需 sidecar 跟踪「已 announce 过的 name」+ 改名后一次性 confirm prompt + 落 self_note
  2. **PanelSettings / PanelPersona「身份」section UI** 留 user 接力（无可靠 UI 测试环境）。当前可手编 settings.yaml `pet_name: 小冬` 立即生效
  3. **029 self_note / 034 surprise 用 pet name**：通过 persona layer 间接影响——LLM 看到「你的名字是 X」自然采用；无 backend 强 enforce
  4. **TG 路径单测**：spec「桌面+TG 共享 single store」自然满足（共用 settings），未单测 TG run_chat_turn 走同 layer
