# 048 · 输入框 placeholder 随 pet mood 切换 — 从首屏就传达主动性

截图证据：ChatMini 输入框 placeholder 固定写「今天感觉怎么样？」每次启动都一样，看似 pet 在问其实是死字符串。GOAL「主动陪伴 + 情绪价值」要求 pet 自己的心情就从首屏传达：mood 焦虑时 placeholder 是「怎么了？聊聊」，愉悦时「今天怎么样？」，想念时「好久不见，怎么样了？」。

需求：
- 输入框 placeholder 改为按 mood.rs 当前 mood 从映射表选一句；mood 切换时 placeholder 跟随更新（不在用户已输入字符时改，避免抢焦点状态）。
- 候选句每个 mood 准备 3-5 条，每次启动 / mood 切换时随机选一条（避免固定句固化为"标签"）。
- 候选句不与 016 morning_briefing / 008 welcome_back utterance 重叠（语气更轻，"placeholder 级"非"开口级"）。
- 候选句表做常量集中，不接 LLM 调用（输入框 placeholder 高频 render，零延迟）。
- mood 数据不可用 / 启动初 → 退回固定句「聊点什么吧」，不显空 placeholder。
- 047 figure mood 角标与 048 placeholder 协同更新但互不依赖（任一独立可关）。

---
实现笔记：
- 本刀 ship **backend slice**——前端 ChatMini placeholder 切换 / salt 计算 / 抢焦点防护是纯 React 改造，留 frontend 工作给 user。
- `src-tauri/src/mood.rs` 新加：
  - `PLACEHOLDER_BUCKETS: &[(&str, &[&str])]` 6 桶（negative / tired_thoughtful / positive / calm / default / no_data）每桶 3-5 条 placeholder 短句，常量集中可扩展。语气统一 placeholder 级（轻 / 邀请式 / 非"开口级"），与 016 / 008 utterance 不冲突
  - Pure `placeholder_bucket(text, motion)` case-insensitive 子串扫描：**负面信号优先**（与 `mood_to_emoji` spirit 同——「焦虑但还开心」→ negative）；都不命中走 default；空走 no_data
  - Pure `placeholder_candidates(bucket)` 返桶候选列表（unknown bucket → 空 slice）
  - Pure `pick_placeholder(bucket, salt)`：salt mod len 索引——**确定性**让前端同 salt 永远返同一句（前端用启动 ts + mood-change counter 当 salt，单 mood 段内 placeholder 稳定不跳字符；不引 thread_rng 避免 RNG 开销 + 让测试可预期）
  - `#[tauri::command] get_input_placeholder(salt: Option<u64>)` 异步壳：mood 不可用 → no_data 桶（spec「退回固定句『聊点什么吧』」），no_data 仅 1 条永远返同一句
- 11 单测：bucket 空 / 负面优先 / 疲惫思考 / 正面 / calm / 未匹配 default / 全桶非空 / unknown bucket / 同 salt 确定性 / 不同 salt 可异 / no_data 固定句
- **Frontend gaps**（user 接力点）：
  1. ChatMini React 输入框 `invoke('get_input_placeholder', { salt })` 拿 String 设 placeholder 属性
  2. salt 策略：`Date.now()`（启动时刻）+ mood-change counter（监听 mood-changed event 递增），保单 mood 段内稳定
  3. **抢焦点防护**（spec 硬约束）：用户已输入字符时不 re-invoke，`value.length === 0` 才更新 placeholder
  4. 与 047 角标互不依赖：两条独立 Tauri command，任一关闭不影响对方
