# STATUS — 实时陪伴 AI 桌面宠物 项目盘点

写于 Iter 100 里程碑（2026-05-03）。回顾从 Iter 1 起 99 次迭代后，对照 IDEA.md 起点
那 5 条"与目标的差距"，看现状走到哪、还差什么。

## 起点（IDEA.md 的差距清单）

1. **完全被动**：宠物只在用户输入时才说话
2. **环境无感**：不知道用户在用什么 app、键鼠是否活跃、几点了
3. **无情绪/状态演化**：每次回复都是无状态的
4. **无节奏控制**：缺少"什么时候该说话、说几句、什么时候闭嘴"
5. **记忆只在工具调用时被动写入**：没有定期反思、整理记忆

## 当前状态（按差距条逐项核对）

### ① 主动发言（差距 1）→ 已闭合
- 后台 tick 引擎（Iter 1）按 `proactive.interval_seconds` 周期评估
- LLM 自主决定开口或返 `<silent>` 标记
- 手动触发命令 + 面板按钮（Iter 63-65）跳过 gate 用于测试
- 累计 99 个 iter 中约 25 个直接服务该轴

### ② 环境感知（差距 2）→ 大部分闭合
- `get_active_window`（Iter 2）+ `get_weather` / Iter 7a + `get_upcoming_events` / Iter 7b
- 键鼠输入空闲（Iter 3，input-idle gate）
- wake-from-sleep 检测（Iter 48）+ prompt 唤醒提示（Iter 49）
- macOS Focus / DnD（Iter 21-25）
- time-of-day 语义（Iter 43）
- per-turn 工具结果缓存（Iter 28）防 LLM 反复调
- 缺：macOS 系统通知 hook（NotificationCenter.db / Iter 7c deferred，隐私 + 权限阻碍）

### ③ 情绪 / 状态演化（差距 3）→ 已闭合 + 进化
- `ai_insights/current_mood` 记忆条目（Iter 4）+ 反应式聊天也接（Iter 11）
- `[motion: X]` 前缀让 LLM 选 Live2D 动作分组（Iter 8-10）
- mood-tag 命中率统计 + panel 显示（Iter 40-41）
- proactive 完成后 re-read mood emit 给前端（Iter 12-15）
- 抽出 read_mood_for_event 统一 helper（Iter 15-16）

### ④ 节奏控制（差距 4）→ 体系化闭合
- 多层 gate：disabled / quiet-hours / focus / cooldown / awaiting / idle / input-idle
  （Iter 5、20、21）
- table-driven gate 测试（Iter 19）
- gate 重构成 guard 列表 + 单一 sleep（Iter 18）
- cadence_hint 文字 + since_last_proactive_minutes 数字双轨（Iter 44 + 93）
- chatty_day_threshold 用户可调（Iter 75-77）
- proactive_rules 上下文动态加规则（Iter 51-54）
- 决策日志 ring buffer（Iter 38-39，Iter 78-79）让"为什么没说话"可见

### ⑤ 记忆系统（差距 5）→ 已闭合 + 强化
- 定期 consolidate（Iter 6）+ 手动触发（Iter 62）
- consolidate 引导 LLM 读 focus history（Iter 24）
- daily_plan 自动过期 sweep（Iter 67）
- reminder 绝对日期 + stale sweep（Iter 56-61）
- speech_history 持久化 + sidecar lifetime counter（Iter 45、71-73）

## 起点没有但浮现出来的能力

- **prompt 自我画像系统**：active_prompt_rules（Iter 84-86）让 panel 实时看到 prompt
  当下被多少规则塑造，加上 nature 分类（Iter 94-95）显示倾向，加上长跑 atomic
  累计（Iter 96）追"今日 prompt 60% 在克制"。
- **数据 → prompt 闭环**：env-awareness 计数低 → prompt 自动加纠偏规则（Iter 83）。
  这是 Iter 83 才真正做出来的反馈环——data shapes the prompt that produced it.
- **复合规则**：第一条积极 prompt（engagement-window / Iter 92）+ 第二条
  （long-idle-no-restraint / Iter 93）——突破"prompt 系统只能压制"的初始范式。
- **三层守护测试**：Iter 89/90/91 用 cargo test 守 backend label / frontend dict /
  proactive_rules match arm 三方对齐，让加新规则的协议可机器验证。
- **Panel 解构**：从 770 行单文件拆为 panelTypes + ChipStrip + StatsCard +
  ToneStrip + PanelDebug（Iter 97-99）。

## 体量

- **代码**：~14010 行（Rust + TS/TSX 合计）
- **测试**：184 个 cargo 单测（首次启动是 0），加 tsc 严格类型检查
- **持久化文件**：`~/.config/pet/{settings.toml, speech_history.log,
  speech_count.txt, speech_daily.json, focus_history.log, app.log, llm.log,
  memory/*.md, sessions/*.json}` 共 9 类
- **Tauri commands 数**：~40+
- **进程内 atomic 计数器组**：5（cache / mood_tag / llm_outcome / env_tool /
  prompt_tilt）

## 仍有的明显空白

1. **Live2D 表情**：当前 mood 只驱动 motion group（4 类粗分），未触及 expression
   切换 / 嘴型同步 / 视线追踪等更细的表情维度（IDEA.md Iter 8 范围其实已经
   到了"基础动作"，但富表达力还薄）。
2. **多窗口 panel 共存**：用户开多个 panel（debug / chat / settings）时数据轮询是
   独立的——同一份 ToneSnapshot 每秒拉 N 次。如果将来用户基础变大需要做共享
   store。
3. **隐私边界**：active_window 标题、calendar event 全文都进 prompt——LLM
   provider 看到。如果把宠物当真伙伴，应该有个"哪些场景不要发出去"的本地 filter。
4. **真正的"长期人格"**：宠物没有跨会话演化的人格——mood 在更新但 SOUL.md 是
   静态。"陪伴一年的宠物"和"刚装上的宠物"语气、记忆密度、情感状态应该有差，
   目前没有。

## 未来路线（粗）

按价值密度排（不是顺序）：

- **A. 长期人格演化**：把 SOUL.md 静态描述 + 累积 speech_history 拼成动态人格
  prompt，让宠物"使用越久越像那只宠物"。
- **B. 表情系统升级**：从 motion group 进到 expression 切换（眨眼 / 嘴角 /
  眉毛分轨），与 mood 字段更细对应。Live2D 模型本身支持，是前端 + mood 解析的活。
- **C. 隐私 filter**：在 prompt 构造层加可配置的 redaction（如某些 app 标题
  / calendar 主题不发出去），让用户能信任宠物背后的 LLM。
- **D. 记忆 surface**：把 `~/.config/pet/memory/*.md` 在 panel 里做可浏览的视图，
  让用户看到宠物"记住了什么"——增强信任也帮 debug。

## 关于"是真实伙伴吗" 的诚实评估

技术上：宠物有自主行为、环境感知、节奏控制、情绪状态、记忆累积、自反馈
prompt——5 条原始差距全部闭合。

体感上：还差**人格深度**和**表情丰富度**。当前宠物像一个"会主动观察你 + 偶尔说
合适的话 + 记得你聊过什么"的执行体；但没到"有自己的脾气、偏好、和你一起经历过
什么"的伙伴感。这个间隙是路线 A + B 要补的。

下一阶段如果继续投入 99 次迭代，最值得投在 A（人格演化）——它把已有的所有
infrastructure（mood / speech_history / memory / prompt 规则）真正绑在一起，让
"陪伴时长"产生差别。其他都是边际优化。
