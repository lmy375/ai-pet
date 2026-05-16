# Pet — 桌面 AI 宠物管家

一个常驻桌面的实时陪伴型 AI 宠物。它既是会主动找你聊天的情绪伙伴，也是能动手帮你处理事务的「宠物管家」。

> 产品定位与边界详见 [`docs/GOAL.md`](docs/GOAL.md)；当前需求池见 [`docs/TODO.md`](docs/TODO.md)。

## 产品介绍

- **形象**：基于 Live2D 的透明无边框桌面窗口，永远悬浮在屏幕一角。
- **大脑**：兼容 OpenAI Chat Completions 协议的任意模型。
- **手脚**：通过内置工具与 MCP 协议连接本地能力（文件、Shell、日历、天气、记忆库等），可被 AI 自主调用以完成任务。
- **多入口**：桌面气泡 / 面板窗口，可选 Telegram Bot 转发，让你在手机上也能继续与宠物对话。

## 产品功能

### 1. 被动聊天 — 桌面随时对话
- 点击桌面宠物即可呼出聊天，输入文字与之交流。
- 支持流式输出、口型同步、情绪驱动的动作切换。
- **双击宠物触发 happy 动作**：Live2D 区双击触发 Tap motion group（happy / 活泼），600ms 冷却防连点刷动画；`settings.motion_mapping` 仍生效让自定义模型用户也能命中 happy 等价组名。子区域浮标（任务 pill / MoodWidget / 收起按钮 / sparkle）的点击不会被误识别。
- **MoodWidget 双击展开 7 天浮窗**：桌面心情徽章双击浮出"最近 7 天心情" sparkline —— 每天一个 dot，色取自当日 top motion（💗 Tap 粉 / ✨ Flick 黄 / 💢 Flick3 橙 / 💤 Idle 灰），opacity & 尺寸跟当日采样数缩放，hover 单 dot 显日期 + motions 分布。首次打开 lazy fetch `get_mood_daily_motions(days=7)`；点窗外 / Esc 关。让用户不必切到 Panel/Persona tab 也能回看一周心情走势。
- **桌面迷你聊天列表**：宠物窗口顶部展示最近 20 条 user / assistant 消息，与 Panel Chat 同款气泡样式（user 右对齐 accent，assistant 左对齐 card）；每次宠物开口（流式或主动）自动滚到底；最新一条 assistant 仍带 ✕ / 👍 反馈按钮，沿用 R1b dismissed / Liked 反馈语义。
- **桌面静默淡出**：聊天列表 60 秒无新消息 / streaming / tool 活动后整段淡到 45% 透明度，让 Live2D 宠物在桌面成视觉焦点；鼠标进入或移动立即 600ms 渐回满。嫌烦的用户可在 localStorage 写 `pet-chatmini-idle-fade = "off"` 关闭。
- **时间戳自适应折叠**：当某条 mini chat 消息的前一条与后一条都是"同 role + 时差 < 60s"时，中间 ts 标签自动隐藏 —— 密集聊天只保留首末时间戳，消息列表不再被时间戳切碎；hover bubble 自身 title attr 仍能拿到完整时间。
- **跨日分隔条**：mini chat 消息时间戳跨日时浮一条居中分割线 + 日期标签（今天 / 昨天 / 本年 MM-DD / 跨年 YYYY-MM-DD）。回滚长历史时一眼分辨"哪条是今天 / 昨天 / 前几天"。第一条有有效 ts 也显，让对话起点也有锚。ts 缺失静默跳。
- **桌面 pet 窗口 Esc 收起快捷键**：在 pet 窗口任意位置按 Esc 即触发 collapse() 把宠物滑到桌边只露 tab —— 替代手点右上角小 ▶| 按钮。让位条件覆盖：已收起 / 右键菜单开着 / 输入控件聚焦时不抢键（让既有 textarea / picker / ctx menu 的 Esc 行为优先）。mouse-enter 左侧 tab 仍能召回宠物，与点 ▶| 按钮行为完全一致。
- **桌面 pet 窗口 ⌘O 打开面板快捷键**：与 Esc 收起对偶 —— Esc 走 / ⌘O 来。在 pet 窗口任意位置按 ⌘O / Ctrl+O 即触发 `openPanel()` 把面板召出来，替代鼠标点 mini chat ⛶ / 输入栏 💬 / 右键菜单「📋 打开面板」三条路径之一。即便 pet 已 collapse 也允许触发（`invoke("open_panel")` 与 hidden 状态正交）。输入控件聚焦时让位，`!shift && !alt` 严格 modifier 避免与浏览器扩展冲突，preventDefault 吃掉浏览器默认"打开文件对话框"。 mini chat ⛶ / 右键菜单两处 tooltip 同步加 ⌘O hint 让 owner hover 即得知。
- **桌面 pet 右键聚合菜单**：在 Live2D 主区右键弹出聚合入口 —— 📋 打开面板 / 📂 打开数据目录（复用 `open_pet_data_dir` 同后端，直奔 `~/.config/pet/`）/ ☀️ 🌙 切 light / dark 主题（直接读 storage 翻转 + applyTheme 即时生效，CSS var 自动 propagate 无需 React 重渲）/ 😴 mute 30 / 60 分 / ☀️ 解除 mute（与 `/sleep` slash 同后端 `set_mute_minutes`）/ 🔄 重启窗口（muted gray 视觉独立）。setTimeout 0 + outside mousedown / Esc close 防"刚开就关"，clamp 视口边界保不溢出。子控件（pill / chip / 按钮）右键不抢菜单，让它们自身行为不被覆盖。
- **桌面 ChatMini 右键菜单**：在 mini chat 任意 bubble 右键弹出聚合入口 —— 「📋 复制本条」「⌚ 复制 · 含时间戳」「💭 针对这条再问」（仅 assistant）「⛶ 在 Panel 中打开聊天」「⛶ 在 Panel 中定位本条」（仅 text 非空）。把原本散在双击 / hover 浮按钮里的动作合并到一个发现入口；最后一条扩展跨窗口 deeplink 协议（`chatMatch.excerpt` 字段，TTL 10s），PanelChat 收到后反向扫 items 找最近 substring 命中、scrollIntoView + 1.5s 高亮 —— 让"想从 mini chat 跳到 panel 看上下文"是 1 步操作。菜单按 viewport 边界自动夹紧、点外面 / Esc 自动关。
- **@ 提及任务 ref**：聊天输入框敲 `@` 触发任务 ref picker（与 ⌘K 互补，IM 风格"@提到"直觉），fuzzy 搜任务标题，选中后插入 `「title」` 引用 token —— LLM 看到后会按 ref 语义解析，且 hover 仍能看到任务状态。**⌘K 全局热键**：Panel window 内 ⌘K / Ctrl+K 在任意位置（消息区 / 侧栏 / chip 区）都能弹任务 ref picker，不必先点 textarea。当焦点在任意输入控件（textarea / input / contentEditable）时让位给该控件自己的 ⌘K handler（textarea 走既有 onKeyDown 路径不变）。
- **双击编辑 user 消息并重新生成**：在 PanelChat 双击历史 user bubble 进 inline textarea 编辑；`Enter` 提交后丢弃后续所有 items 与 messagesRef 对应位之后的内容，再以新内容触发流式回复（IM 风），`Shift+Enter` 换行、`Esc` 取消。流式中 / 含图片的消息不进入编辑（语义边界）；任务 ref token 双击仍走原跳转语义不冲突。
- **PanelChat 上下文 token 警示 banner**：PanelChat 60 秒轮 `get_active_session_context_stats`（与 ChatMini / DebugApp 同源），当 LLM context 累计 tokens > 4000 时输入栏顶部贴顶 yellow tint banner 「💭 上下文 ~N tok（已超 4000，建议 /reset 让宠物注意力回到当前话题）」+ 内嵌 `/reset` 按钮。点击调统一抽出的 `handleResetLlmContext` 钩子（与既有 `/reset` slash 命令同 path）—— PanelChat 是软 reset，保留可见 items 仅清 messagesRef，无需 armed 二次确认；流式中按钮 disabled 防 race。让 panel 长 session 的 owner 在敲下一条之前感知 prompt 在膨胀。
- **桌面 mini chat 上下文 token 提示 chip**：App.tsx 60 秒轮 `get_active_session_context_stats`（与 DebugApp 统计 tab 同源），当 LLM context 累计 tokens > 4000 时 ChatMini 顶部浮 yellow tint chip 「💭 上下文 ~N tok（已超 4000，建议 /reset）」+ 内嵌 `/reset` 按钮。armed-confirm 二次确认（首点变红 "再点确认 (3s)" + 3s 自清）防止误触丢历史。点击 reset 走新 `resetContext()` 钩子清掉 messages + items + saveSession 立即落盘，系统提示词保留。让 context 控制成为 ambient 反馈而不必切到 DebugApp 才能感知。
- **`/reset` 软清空 LLM 上下文**：与 TG `/reset` 对偶 —— 桌面 PanelChat 敲 `/reset` 把 messagesRef 砍到 system-only（保留可见 items / bubble 历史不动）。语义与 `/clear` 互补：clear 是"硬清空全部消息让 session 看着像新建"，reset 是"我想跟宠物开新话题但又不想丢可见历史"。流式中拒绝以防截断 race；写盘后重启 / reload 仍保持干净 LLM 状态。**DebugApp 统计 tab 顶部"当前会话 LLM 上下文"卡片**实时显示当前 session 累积的 messages / chars / tokens（排除 system），与 `/reset` 配合让用户感知"上下文是否该清"。tokens > 4000 时卡片转 yellow tint + 提示 "考虑敲 /reset 清掉以省 token"。
- **`/repeat` 重发上一条 user 消息**：PanelChat 敲 `/repeat` 直接 send 最近 user item 的同款 content + images（若有），让"宠物回得不满意，再试一次"或"刚才网络半截，再跑一遍" 1 步搞定 —— 比 ⌥↑ + Enter 少一步操作。不丢历史 / 不复用 LLM 缓存（fresh turn）；流式中拒绝以防 race。
- **`⌥↑` 键盘召回到编辑模式**：PanelChat 输入框空 + 不在历史浏览态时按 `Alt+↑`，倒序找到最近一条 user bubble 直接进入 inline 编辑（不经 send-history 循环、不动剪贴板）。送回纯键盘党 IM 体感的同时不抢原 `↑`（仍是 send-history cycle），按 ⌥ 修饰键明确"我要的是编辑而非召回历史"。
- **输入 token 估算 chip**：PanelChat 输入框非空时左上角浮 `~N tok` 小 chip，给"我打了多长"的直觉感知（CJK ~1 token/字 + 其它 ~4 字/token；hover tooltip 说明口径 + 实际字数）。与右侧的"↑ 历史"提示错开布局不互挡。
- **复制消息带元数据**：消息上的复制按钮 ⇧/Shift+点击会在 payload 顶部加 `[session 标题 · YYYY-MM-DD HH:MM]\n` 前缀（与 ⌥+点击保留 markdown 可叠加），适合外部归档 / share。
- **`/done` / `/cancel` / `/retry` / `/snooze` / `/unsnooze` / `/pin` / `/unpin` 桌面任务管理**：聊天输入框 `/done <标题>` 标完成、`/cancel <标题>` 取消、`/retry <标题>` 重试 Error 任务、`/snooze <标题> [30m / 2h / tonight / tomorrow / monday]` 暂停（缺省 30m）、`/unsnooze <标题>` 解除暂停、`/pin <标题>` 钉住关键任务、`/unpin <标题>` 取消钉住（均支持子串模糊匹配），免切到「任务」tab；与 TG bot 同名命令体感对齐，0/多命中时给候选清单引导精确化。snooze 边界与桌面右键 Snooze 子菜单完全一致（今晚 18:00 已过自动跳明晚 / 周一永远跳下周一）。`/pin` 双语义：无参数仍是"切换当前会话钉住"的既有 alias、带 title 时钉任务，由是否带 title 消歧。
- **`/stats` 桌面任务状态速览**：聊天输入框 `/stats` 一行汇总待办 / 逾期 / 今日完成 / 出错 / 今日取消计数（含所有任务，不只本会话）；与 TG bot 同名命令对称，全 0 时显"今日很安静 ✨"。
- **`/today` 桌面今日叙事**：与 `/stats` 互补 —— `/today` 直接列出今日到期 + 今日已完成的任务**标题清单**（不是数字），快速回答"今天该干嘛 / 搞定了啥"。每段 cap 5 + 溢出提示。
- **今日会话 chip**：聊天面板顶部显示今日活跃会话数 + 累计消息数（📅 N · M），让 owner 一眼看到当天活跃度（会话级近似 —— per-message timestamp 暂未持久化）。
- **系统反馈视觉降噪**：slash 命令的执行回执（`/help` 命令清单、`/done`/`/stats` 反馈等）以小字 + 虚线 + 半透明的 subdued bubble 渲染，与真 LLM 回复区分；markdown 导出会话时也自动过滤掉这些系统消息。
- **极简桌面入口**：宠物窗口底栏只保留"输入框 + 💬 打开聊天面板"按钮；多 tab 设置 / 任务 / 记忆全部走面板，桌面层不再堆功能按钮。
- **输入框 placeholder 轮播**：input 空 + 非流式时每 30 秒在 5 句 placeholder 之间循环（"说点什么…（可粘贴 / 拖入图片）" → "今天感觉怎么样？" → "想聊点啥？" → "需要帮忙做什么？" → "随便聊聊，我陪着 🐾"），让放置态桌面少点"待机寡淡感"。首句保留功能性提示让新用户能学到能力；后续 conversational 风。用户开始打字立即停止轮换，发出去后回到上次的 idx 继续。
- **桌面状态 pill（胡萝卜 + 大棒）**：宠物 Live2D 区左上角实时显示任务状态（60s 轮询后端 `task_stats`）—— 现支持 3 段动态拼接：🔴 N（逾期）/ ✓ M（今日完成）/ 💤 K（pending 中处 `[snooze: …]` 暂停期的任务）。1 段时附中文后缀（"逾期 / 今日完成 / 暂停"）让初见也能读懂；多段以 `· ` 串接。tint 按紧迫度优先：红 > 绿 > 蓝（snoozed-only）。点击跨窗口 deeplink 跳「任务」tab（含逾期跳 overdue filter）。
- **今日主动 🐾 chip**：桌面 Live2D 区右上角 `right: 76px` 位置（紧邻 ✦ 陪伴 chip 左侧）显「🐾 N」chip，10 分钟轮 `get_today_speech_count` 看宠物今天主动找你几次（不含 user-initiated 回复）。count > 0 才显 —— 早上还没开口 / 兜底未抓到都不渲染避免噪音。opacity 0.6 → 1 hover，点击跳「人格」tab 看完整 speech 统计（今日 / 本周 / 累计 + 小时分布）。
- **陪伴天数 ✦ chip**：Live2D 区右上角紧贴收起按钮的小 chip 实时显「✦ N」（10min 轮 `get_companionship_days`），鼠标 hover 时 opacity 0.6 → 1 唤起，点击 deeplink 跳「人格」tab。比"切到 Panel/Persona 才能看陪伴天数"的来回少一步。
- **任务完成 sparkle 庆祝**：`task_stats.done_today` 单调 +1 时 Live2D 区飘 6 颗 ✨ / ⭐ / 🌟 粒子，按弧线 fan out + scale + fade，1.5s 内涟漪式涌现完成。首次观测仅作 baseline 不点燃（避免开窗时已是 N 直接误触）；午夜回到 0 也不触发。`prefers-reduced-motion` 下整段不渲染。
- **`↑` / `↓` 召回历史消息（跨窗口共享）**：桌面 pet 窗输入框与面板大聊天框共用同一份 shell 风发送历史（cap 20 · dedup · localStorage 持久）—— 在 pet 窗发的话回到面板按 `↑` 也能拉回，反之亦然，重启 app 后仍在。
- 面板窗口提供完整的对话历史、人格设定、记忆查看与设置入口。
- **会话列表元信息**：dropdown 每个会话标题旁附 "(N 条)"消息总数，跨会话切换前一眼分辨深会话 vs 空会话，省去"切进去才发现是新建空白"的来回。
- **`✨ LLM 重写标题`**：session tab 右键菜单加 LLM 自动重写标题入口 —— 调一次非流式 `chat/completions`（30s timeout / temperature 0.3 / max_tokens 30），上下文是会话尾部 10 条 user/assistant turn（multipart content 抽 text + 每条 cap 400 字防 prompt 超长），prompt 让 LLM 给 ≤ 10 字概括；返回清洗（剥首尾引号 / 句号 / 换行折成空格 / cap 30 char）后写回 session.title + save。免去"首条 user 消息前 20 字"硬截带来的不切题标题，对 20+ session 的用户尤其有用。
- **会话标题搜索**：session > 5 时会话下拉顶部浮出标题筛选框，子串匹配实时过滤；与 chip filter（📅 今日 / 📷 含图片 / 📋 含派单 / 📌 钉住）AND 组合，让"📋 含派单 + 标题含 Downloads"这类组合查询一键命中。打开下拉自动 focus，Esc 一键清查询 / 关下拉；下拉关闭时 query 自动复位避免下次开 dropdown 残留旧查询。
- **会话「📌 钉住」过滤 chip**：当 sessionList 含任意 pinned 会话时，下拉顶部 chip 行末追加第 4 个「📌 钉住」chip 一键过滤只显已钉住的会话；点击进入"只看钉住"态，再点关闭过滤显具体命中数。pinned 会话本来就靠后端排序浮顶，chip 提供"只看钉住"的补充入口。本地 derive ids 无 IPC 往返，与任务面板的「📌 N」chip 形成跨模块对称体验。
- **会话下拉按月份分组折叠**：sessionList > 20 时启用月份分组 —— pinned 自然占首段「📌 钉住 (N)」，余者按 updated_at 月份归到「本月 (N) / 上月 (N) / YYYY-MM (N) / 更早 (N)」section。section header `position: sticky` 滚动时粘顶让 owner 始终知道"我在哪个月段"。≤ 20 条不分组，避免新用户面对"本月 (3)"这种无意义 header；filter 收窄时仍按 sessionList 总量判断，避免"chip 过滤 → header 消失 → 关 chip → header 重现" 的认知抖动。Fragment 包装 + 预扫 idx Map，最小侵入既有 row 渲染。

### 2. 主动聊天 — 后台陪伴
- 后台长期运行的 **proactive 引擎**：根据你的空闲时长、当前情绪、近期话题、最近反馈与陪伴节奏，决定何时、用什么语气主动开口。
- 内建多重「门控」（mute / 安静时段 / 冷却时间 / 截止时间紧迫度）避免打扰。
- **早安简报**：每日固定时刻（默认 8:30，可配置）自动开口，调用 weather / calendar / memory 工具把天气、日程、提醒和昨日回顾汇成一段短播报；尊重 mute，绕过普通发言冷却。
- **任务完成报喜**：butler_task 被 LLM 标 [done] 后，下次 proactive tick 检测出"刚转 done"，prompt 注入 `[任务刚完成]` 段，让宠物在合适时机简短确认产物；与 mute / cooldown 等 gate 自然协同。
- 每一次主动发言的决策都记录在 **decision log** 中，可在调试面板复盘。
- **决策日志过滤 + 批量复制**：除按 kind 多选 / reason 子串过滤外，新增"近 10m / 30m / 1h"快捷时间窗（三层 AND 叠加，方便 debug 短时间内事件）；filter 行尾"📋 复制 N"把当前过滤后的决策按 `[ts] kind reason` 多行格式一键复制，贴 issue / 终端 grep 都友好。

### 3. 自我进化 — 情绪 / 记忆 / 技能
- **情绪系统**：宠物拥有持续演化的心情，会影响台词风格与外观动画。
- **记忆系统**：聊天与互动会沉淀为长期记忆，定期由后台 **consolidate 循环** 整理压缩。
- **陪伴感知**：累计陪伴天数、每日发言次数、情绪曲线等指标可在面板查看。
- **反馈学习**：对气泡的忽略、关闭、点赞会被记录并反馈到主动发言策略。
- **记忆搜索高亮 + 分类活跃度**：搜索结果里 keyword 在 title / description 黄底深棕字标出（与聊天 / 设置 / 任务搜索同款）；每个 memory category section 标题附"最近 X 天前更新"小字，让用户感知哪些区域在活跃迭代、哪些是死库存。
- **记忆按时间排序 toggle**：面板顶部 `📅 默认序 / 按时间` 一键切换全局排序模式 —— 按时间倒序时各 category 的 rest 段按 `updated_at` 倒序，pinned 仍优先挂头但段内也按时间，"最近改的"最先看到。偏好 localStorage 持久。
- **记忆批量删除**：每条 memory 行左侧加 checkbox（跨 category 同名走 `cat::title` key 避免碰撞），选中后顶部浮 action bar 显「已选 N 条」+ 「🗑 批量删除」+「取消选择」。删除走单条 `memory_edit("delete", ...)` 逐条（与 mirror 双写、search 刷新等同 audit trail），arm/confirm 二次确认避免误删。失败合并到既有 message toast，部分成功也有清晰报告。
- **记忆 description 双击 inline 编辑**：与既有「双击 title 改名」对偶 —— 双击 description 文字立刻进 textarea 编辑（`Enter` 保存 · `Shift+Enter` 换行 · `Esc` 取消 · 失焦自动保存），免开 modal。原值未变 / trim 后等价时短路 noop 不写盘。rename 输入框激活时双击 description noop 防两 inline UI 打架；task-ref token 自带 stopPropagation 双击仍走任务跳转语义不冲突。
- **技能简档**：人格 tab 新增「最近常用的工具」面板，从最近 30 次工具调用里派生 top 5（按使用频次 + 最近时间），显示宠物正在练什么手艺，与自我画像 + 当下心情拼成「我说自己是谁 / 现在感觉如何 / 实际在做什么」三层自我感。
- **`/whoami` 自我介绍**：聊天敲 `/whoami` 一行清单"陪伴天数 / 当前心情 / 自我画像首段 ≤ 90 字 / 近常用工具 top 3 含频次"，把分散在人格 tab 的四块信号在聊天里聚合一次，让宠物像 IM 朋友一样自报家门。所有数据源并发 fetch + 单源失败不挂整段（`Promise.all` 各自 catch fallback）。**TG 端 `/whoami` 同款对偶**：同一份信号源，同一份排版模板（首段切分 + 90 字截断 + 工具 top 3 含频次），让手机端也能让宠物自报家门；每个源独立缺省（未配 user_name 不渲染该行 / 全空兜底"还没攒到自我介绍的素材"）。

### 4. 宠物管家 — 通用任务执行
- 内置工具集：`file_tools`、`shell_tools`、`calendar_tool`、`weather_tool`、`memory_tools`、`system_tools`。
- 通过 **MCP（Model Context Protocol）** 接入外部工具服务器，扩展能力边界。
- 工具调用前可通过 **tool review** 机制人工审核高风险操作（基于 `tool_risk` 的分级）。
- 支持后台计划任务（butler schedule）、提醒、每日小结。
- **任务队列面板**：在「任务」标签页填标题 / 描述 / 优先级（0-9）/ 截止时间，宠物在 proactive 循环里按"过期 → 优先级 → 早到期 → 早创建"自动取单执行，结果通过 `[done]` / `[error: ...]` 标记回流到面板。
- **自然语言派单**：在「聊天」里直接说「帮我整理 Downloads」/「记得明天下午催报告」，宠物识别后弹出任务确认卡（含解析好的标题/描述/优先级/截止时间），点「创建任务」即入队，省去切到面板填表单的步骤。
- **长任务心跳**：被宠物动过手却停滞超过阈值（默认 30 分钟，可配置）的 pending 任务会在下次 proactive turn 里被点名，宠物必须写一句进展或标 done / error，避免任务静默淤积。TG 派出的任务停滞也会通过 bot 主动发"任务 X 卡 N 分钟了，要不要我点一下"，附 `/retry` `/cancel` 命令模板，让多端用户也能即时响应。
- **任务取消与重试**：失败任务一键「重试」（剥掉 error 标记回到 pending）；进行中的任务可一键「取消」并填原因（写入 `[cancelled: 原因]` 标记 + decision log），把"已完成"与"已取消"在面板上区分展示。
- **周报合成**：每周日 20:00 后由后台 consolidate 自动汇总本周的任务（管家事件计数 + 完成/取消列表）、对话（主动开口次数）、情绪（top 心情 motion）、陪伴（累计天数），写入 `ai_insights/weekly_summary_YYYY-Www`。确定性流水线，不依赖 LLM —— 即便 API 失效也按时落地。
- **工具风险设置**：在「设置」标签页可以为每个内置工具单独选「自动 / 总是审核 / 总是放行」，覆盖分类器的默认行为。`bash` / `write_file` 等高危工具默认要审核，但用户可改成"放行"批量自动化；只读工具默认放行，但洁癖型用户可改成"总是审核"上一道保险。
- **Telegram 派单 + 状态管理**：在 TG 里直接说「帮我整理 Downloads」/「记得明天提醒我交报告」，宠物自动调 `task_create` 入队（无需面板确认卡）。任务执行完毕（成功 / 失败 / 取消）由后台 watcher 主动把结果回传到原 TG 会话；手机端用 `/done <title>` / `/cancel <title>` / `/retry <title>` 直接管理状态（三个命令都支持按 `/tasks` 显示顺序的数字编号代替 title）；`/snooze <title> [preset]` 把任务暂停一段时间（preset = `30m` / `2h` / `tonight` / `tomorrow` / `monday`，缺省 30m），`/unsnooze <title>` 解除暂停 —— 与桌面右键 Snooze 子菜单同语义对偶；`/pin <title>` `/unpin <title>` 钉 / 解钉任务（与桌面「📌 N」chip 过滤同源）；`/pinned` 列出本聊天派单中所有钉住任务（按状态分组 + 空集合教学引导）；`/stats` 一行汇总待办 / 逾期 / 今日完成 / 出错 / 今日取消的计数；`/today` 列今日到期 + 今日已完成标题清单；`/mood` 查看宠物当前心情（与桌面 MoodWidget 同源）；`/reset` 清掉 LLM 对话上下文（保留人设）。桌面与 TG 之间形成派单 → 执行 → 状态管理 → 回传的闭环。
- **任务-记忆联动**：任务描述支持 `#tag` 标签和 `[result: 产物]` 标记。完成的任务在面板上独立显示「✓ 产物：…」一行；周报按 tag 聚合（`#organize × 3、#weekly × 1`）+ 完成清单带产物，让"本周往哪个主题投入最多"和"具体做了什么"一目了然。
- **任务依赖（`[blockedBy: …]`）**：description 写 `[blockedBy: 标题 A, 标题 B]` 声明先决任务；先决全部 done / cancelled 之前主任务**自动从 proactive prompt 块里隐藏**（LLM 看不到 → 不会 pick），prompt header 透明告知"另有 N 条被卡住"；面板仍渲染主任务并附 🔒 chip + tooltip 列出仍卡着的 blocker，让 owner 看到队列里"为什么没人做这条"。typo / 已删的 blocker 视作已解决避免永久死锁；`butler_task_edit` 工具描述列出 marker 形式与示例引导 LLM 使用。
- **任务钉住（`[pinned]`）**：description 写 `[pinned]` 把任务标为"关键"。任务行右键 → 「📌 钉住」/「📌 取消钉住」一键切换（owner 偏好与状态正交，done / cancelled 也可标）；面板 chip 行 `pinnedCount > 0` 时常驻 amber「📌 N」过滤 chip，点击仅显 pinned 任务，状态 localStorage 跨 session 持久。row chip 区显 📌 让"哪些被钉住"一眼可见。与 `[snooze:]` / `[blockedBy:]` 同 description-marker 协议族 —— LLM 用 `butler_task_edit` 工具也能看到 / 改 pinned 状态。strip-before-write 保多次 toggle 不让 description 累积冗余 marker。**Proactive prompt 优先级 boost**：宠物 LLM 在 `format_butler_tasks_block` 看到 pinned 任务时按 `钉住 → 到期 → 最早委托` 排序，每条 prompt line 带「📌 钉住 ·」前缀，header 透明告知"其中 N 条由 owner 钉住（优先做）"，footer 教学"哪怕做一小步也好" —— owner 钉 → LLM 优先做 → 完成回流的闭环。**最近 24h 完成总览**：proactive prompt 还含独立 `[最近 24h 完成]` 段（rolling window），让 LLM 看到 owner / pet 过去一天完成的 N 件 butler_task —— 与瞬时 `[任务刚完成]` 互补，可用作"咱昨天搞定的 X 怎么样了 / 前面那个 Y 看起来挺顺手"等连贯关怀的抓手。cap 8 条 + result 截 80 字 + 最近完成在前 + 数据 corrupt（未解析时间戳 / 未来时间戳）跳过。**批量 pin / unpin**：bulk 工具栏在「改 tags」与「复制标题」之间多两个按钮「📌 钉住」/「📌 取消钉」 —— 圈选 N 条任务一击批处理，与 ✓ 标 done / 改优先级 等同模板（progress 反馈含"已跳过 N 条已钉住 / 未钉住"）。解决"长 pending 队列里关键任务被淹"的问题。
- **butler_task `[every: 工作日/周末/周一 HH:MM]` weekday-set 限定**：`[every:]` 支持在时间前加 weekday-set 关键词限制触发日：`[every: 工作日 09:00]` 仅周一到周五 09:00 触发（standup / 日报场景）；`[every: 周末 10:00]` 仅周六周日 10:00（周末整理）；`[every: 周一 09:00]` 单日（周一周会）。中文识别集："工作日 / 周一到周五 / 双休 / 周末 / 周一-周日 / 星期一-星期日 / 礼拜一-礼拜日"；英文识别："weekday / weekdays / weekend / weekends / mon-sun / monday-sunday"。后端 `ButlerSchedule::EveryOnWeekdays(mask, h, m)` 7 位 bitmask（bit 0 = Mon）；`is_butler_due` 向回扫 ≤ 7 天找最近 mask 命中日 + HH:MM 作 most-recent-fire。面板 schedule chip 显 "🔁 工作日 09:00" / "🔁 周末 10:00" 等可读标签，hover 显完整解释。SCHEDULE_TEMPLATES 加 "🔁 工作日" / "🔁 周末" quick-insert 按钮；placeholder 含示例。覆盖 90% "上班日 standup" / "周末整理" / "周一周会准备" 等高频场景。
- **butler_task `[silent]` 静默标记**：owner 给某个 butler_task 描述加 `[silent]` 即可让它**完全不进 LLM 的 proactive cycle 主动 pick 队列**（与 `[blockedBy: …]` / `[snooze: ...]` 同 filter pipeline，但维度是 owner 显式意图而非依赖 / 时间）。常用于"想保留这条记录 / 知道存在 / 偶尔自己手动做，但不希望 pet 主动催"的事项（如"给某长辈打电话" 这种 owner 自己记得就行的任务）。PanelMemory 行显灰色 🔇 silent chip 可点 toggle 仅看本段 silent 任务；section header 显 "🔇 N silent" 计数 chip 与 "📌 N pinned" / "💤 M snooze" 同模板；SCHEDULE_TEMPLATES 加了 quick-insert 按钮；placeholder 含示例。PanelTasks 行右键菜单加「🔇 标 silent / 解除 silent」一键 toggle 按钮，调 `task_set_silent` Tauri 命令 atomic strip-before-write。**TG bot 同款命令**：`/silent <title>` / `/unsilent <title>` 三层 resolve（数字编号 / fuzzy / exact）+ 反向命令提示，与 `/pin` `/snooze` 同 dispatch 路径完整跨端覆盖。proactive prompt header 透明告知 LLM "另有 N 条被 owner 标 [silent] 不选"，让宠物知道存在但不主动选；全部 silent 时也输出特殊说明 "用户委托给你的管家任务：全部被 owner 标 [silent]（共 N 条），不在主动 cycle 里出现，等 [silent] marker 移除后再出现"。
- **butler_task `[reminderMin: N]` 软提醒**：在 butler_task 描述里叠加 `[reminderMin: N]`（N = 1..1440 分钟）让到点前 N 分钟在桌面 ChatMini 浮一条 `🔔 提醒：「X」将在约 N 分钟后到点` 的软消息 —— 不打开 Live2D 主动开口、不抢焦点，给 owner 抬头 buffer。例如 `[once: 2026-05-20 18:00] [reminderMin: 5] 准备会议材料` 会在 17:55 浮一次提醒；`[every: 09:00] [reminderMin: 3] 早安播报` 每天 08:57 提醒一次。PanelMemory 任务行显绿色「🔔 -Nmin」chip 让 owner 一眼看到"这条会在 N 分前 ping 我"；SCHEDULE_TEMPLATES 里加了 quick-insert 按钮。桌面 pet 内 60s 轮询 + `${title}::${fireTimeIso}` dedup key 保 same fire-cycle 只触发一次、every 跨日自动允许下次；进程重启后 Set reset → 同 cycle 可能重新提醒一次（重启低频 + "重提醒 < 漏提醒"）。
- **任务 snooze（`[snooze: YYYY-MM-DD HH:MM]`）**：与 blockedBy 同语言但时间维度。description 写 `[snooze: 2026-05-20 09:00]` 把任务暂停到指定时刻 —— 到点前从 proactive prompt 自动隐藏（与 blockedBy 同 filter 路径，header 透明告知 N 条暂停），到点后 marker 自然失效不必清理。多个 `[snooze:]` 取最后一个有效值 —— LLM 可以 append 新 marker 重新延后，不必先剥旧。面板 pending/error 行显紫色 💤 chip "至 MM-DD HH:MM"，hover 看完整时刻。**任务行右键菜单聚合 4 个 snooze 预设**（30 分钟 / 今晚 18:00 / 明早 09:00 / 下周一 09:00），免手敲 marker；当前 snoozed 时多出「☀️ 解除暂停」一行。新 `task_set_snooze` Tauri 命令 atomic 写 / 撤 marker —— 多次切换 description 自动 normalize（旧 marker strip + 新 marker append + 空白合并）。**自然短串预设入参**：`task_set_snooze` / TG `/snooze` 现接受 EN (tonight / tomorrow / monday / Nm / Nh) + CJK (今晚 / 明早 / 明天 / 明日 / 下周一 / 周一 / N分 / N小时) 预设，命中后自动按"当前时刻" 解析到绝对 `YYYY-MM-DD HH:MM` 串再写盘 —— LLM 不必先算时间。Tonight = 今日 18:00 或明日 18:00（今日已过则跳明日）/ Tomorrow = 明日 09:00 / Monday = 下个周一 09:00。
- **✨ LLM 重写任务标题**：任务行右键菜单加 LLM 自动重写入口 —— 调一次非流式 `chat/completions`（30s timeout / temperature 0.3 / max_tokens 30），上下文是 task title + description + detail.md 前 600 字，prompt 让 LLM 给一句 ≤ 10 字中文新标题；返回清洗（剥首尾引号 / 句号 / 换行折成空格 / cap 30 char）后 atomic 调 `memory_rename` 写回。免去手动想新名的脑力开销，对那种"原名已严重失真"的老任务（如 owner 写"周一会议"但实际进展是 "周一 standup 提 GrowthBook 灰度问题 + 设计接力"）尤其有用。与 PanelChat session ctx menu 的同名按钮共享 IO 模板。
- **任务复盘视图**：队列标题下显"今日完成 X · 近 7 天 Y" 完成率统计；每条任务卡片附"X 天前创建"相对时间，分辨"新积压 vs 老欠债"；showFinished 视图把 done/cancelled 按"今天 / 昨天 / 本周 / 更早"分组渲染，配合完成率形成立体复盘视图。**老任务 🕰 chip**：pending / error 且 `created_at ≥ 3 天` 时 row chip 区显 muted gray 的 `🕰 N 天前` —— 让 owner 扫长队列时一眼看到"这条放了 14 天"，配 📌 钉住 chip 形成"重要 × 久放"二维视觉信号，hover tooltip 引导拆 / 改 priority / cancel 决策。终态行不渲（任务结束后年龄无 actionable 信号）。
- **新建任务 due 快捷 chips**：「今晚 18:00 / 明天 09:00 / 下周一 09:00 / +7 天」一键填入，省手敲 datetime-local；含「清除」按钮一键回到无 due 状态。今晚已过 18:00 时自动跳明晚（防回退到过去）；今日周一仍跳下周一（"周一"语义 = 下周第一天）。
- **任务模板个性化**：「📋 从模板」下拉除 4 条内置范例外，用户可点旁边「💾 存为」把当前 title / body 存为自定义模板（localStorage 持久，上限 20 条，label ≤ 20 字 + 重名拒绝），下拉用 optgroup 把「内置范例」与「我存的」分组。custom > 0 时多出「管理 N」按钮打开 Modal 逐条删除。
- **任务自动归档 + 一键恢复 + 手动清理**：consolidate 循环把 done / cancelled 且 `updated_at` 超过 30 天（可在「记忆整理」面板调，0 = 关闭）的 butler_tasks 自动挪到 `task_archive` 类目，让活跃队列长期保持轻量；归档条目带 `[archived: YYYY-MM-DD]` 头，title 加日期前缀防重名碰撞。「任务」面板底部「📦 归档」折叠区一键回看老任务（lazy load + 刷新按钮 + 顶部搜索框按 title/description 子串过滤），每条归档行有「↩ 恢复」按钮把它剥光 done / result 等终态标记重建为 pending butler_task；header 多出「🗑 清理 >30 天」（armed 二次确认）批量删除老归档，让归档区也不至于无界增长。
- **任务详情 markdown 工具栏**：detail.md 编辑器顶部 9 个快捷按钮（**B** 粗体 / **•** 列表 / **🔗** 链接 / **`</>`** 代码块 / **☐** 待办 / **❝** 引用块 / **📊** 3×3 GFM 表格 / **📅** 当前时间 / **✓** 完成行），点击在光标位置或选中区域包/插对应 markdown 语法，少敲 `**...**` / `[](url)` / ``` ``` ``` / `- [ ]` / `> ` / `| --- | --- |` 这种符号。代码块用 `</>` 字体 monospaced 与"代码"语义对齐；待办按钮插 `- [ ]` 行首前缀；引用块按钮每选中行行首加 `> `（粘别人的话 / 引用之前结论 / 提示框都用得到）；表格按钮一键插 3×3 骨架 + 第一格"列 1"自动 select 让用户敲即覆盖（光标不在行首时自动补换行让表格独占段落）；时间按钮插 `YYYY-MM-DD HH:MM` 本地时间戳（与 `[snooze:]` / `[once:]` marker 协议同形，复制即可包成 marker），记录里程碑 / 进度笔记零摩擦；✓ 完成行按钮在光标所在行首插 `- [x] YYYY-MM-DD HH:MM ` 模板，光标落尾让你直接敲『做了什么』 —— "做完一小步 + 记下来什么时候做的"零摩擦，配合既有 GFM checklist 渲染自动呈勾选 + 删除线视觉。**📂 在 Finder 显示** 按钮一键 `open -R memories/<path>` 让系统文件管理器高亮选中本 detail.md，方便 owner 拖图附件 / git add / 用 VSCode 打开 / 重命名等"编辑器外操作"；Windows 走 `explorer /select,`，Linux 退化到打开父目录；detail.md 尚未存盘时 toast 提示 ⌘S 一次再点。**渲染层连续 `>` 行合并为单 blockquote 容器**：accent 50% 半透明左竖条 + 4% accent 底纹 + 4×10px padding + 右下圆角，跨多行视觉融为一段。**📑 大纲浮窗**：detail.md 含 H1-H3 标题且在 split / preview 模式时，视图模式切换行末显「📑」toggle 按钮。点击展开 inline 大纲面板列所有标题（缩进显层级 + `#` 前缀 + 跳过来的目标节 hover 高亮），点击任一标题 `scrollIntoView smooth` 跳到预览 pane 对应位置。parseMarkdown 加 `headingIdPrefix` 选项让 heading divs 挂 `id={prefix-h{counter}}`，counter 按出现顺序累计避开同名标题碰撞 / 中文 slug 化复杂度。`scrollMarginTop: 12` 让跳节后 heading 不被工具栏遮住。**Active heading 高亮**：IntersectionObserver 监听 preview pane 所有 heading（`rootMargin: -70%` 让观察区缩到视口顶部 30%），把"最靠上可见"的 heading 在大纲对应 item 上加 tint 蓝 bg + 加粗，owner 滚 preview 时一眼知道"我在哪节"。content 变化时自动重建 observer。**Heading 旁 📋 复制本节**：每个 heading 旁有低 opacity 0.5 → hover 1 的 📋 按钮，点击拷"heading + 后续内容直到下个同级 / 更高级 heading 之前"全段 markdown 到剪贴板（H2 节含 H3 子节）—— 长 detail 分节复制单节贴 share / issue 零摩擦。**60s 自动草稿 + 恢复 banner**：编辑器每 60s 把 textarea 内容（仅 dirty 时）dump 到 localStorage `pet-detail-draft-${title}`。下次打开同任务 detail 时若检测 draft 与磁盘版不同 → 弹 amber tint banner 「📝 检测到上次未保存的草稿（N 分钟前）—— 与磁盘版差 N 字符」+ 「🔄 恢复 / ✕ 忽略」两按钮。保存成功清掉 draft；Esc cancel / 关 panel / 崩溃 → 保留 draft 让下次恢复。与"dirty > 60s 红色 pulse 警示" 同 60s 阈值 —— 主动 reminder + 主动 backup 双保险。**底部光标状态栏「行 N / 共 M」**：edit / split 模式 textarea 跟踪光标位置（onChange + onSelect + onKeyUp + onClick 多事件覆盖），底部状态行在 ● 未保存与字数 counter 之间渲染当前光标行号 / 文档总行数，IDE 状态栏同体验，写长 detail.md / 调试 markdown 段时定位无脑。**☑ checklist 进度 chip**：扫 detail.md 里所有 `- [ ]` / `- [x]` 行计数，状态栏中段（行号之后字数之前）渲染「☑ done / 共 total」chip；全勾完时变绿 + 加粗作视觉奖励。total === 0 时不显避免噪音；preview / edit / split 三态都显。配合工具栏 ☐ 待办 + ✓ 完成行按钮形成 "插 marker → 看进度" 的完整反馈环。
- **任务详情粘贴图片自动压缩**：detail.md 编辑器粘贴 / 拖入图片时，> 256 KiB 的 blob 走 canvas resize（长边 cap 1600 px） + JPEG 0.85 重编码，base64 后内嵌 markdown `![](data:…)`；≤ 256 KiB 的小图（emoji / 小 logo / 短动图）保留原 mime 直通。一张 6-10 MB 的 macOS 全屏截图通常被压到 200-400 KB，detail.md 不再被几张高清截图撑爆 fs IO；触发压缩时面板浮 toast「已压缩 N 张图片（X.X MB → Y KB）」让用户对体积变化心里有数。失败时 catch 回退到原图，保"图比报错重要"。
- **任务详情 `[task: 标题]` 跨任务 ref chip**：detail.md 里写 `[task: 整理 Downloads]` 语法（冒号后空格 + 任意 title + 闭 `]`）渲染为蓝色 chip 含 status emoji（✅ done / ❌ error / 🚫 cancelled / 📋 pending）+ 可选 📌 pin 前缀 + title。click chip → 跳到目标 task 行（清 filter + 显 finished + scrollIntoView + flash 高亮，与"完成小卡跳行"同 pendingTitleFocus pipeline）。lookup 未命中（task 已删 / typo）→ muted 灰底 dashed border + "(未找到)" hint 让 owner 自我 debug。仅 read-only detail 展开 + hover preview 两条 callsite 识别，编辑期间 preview 看字面量 —— 与 chat 「title」ref / @ picker 双轨各自不冲突。
- **任务详情 bare URL 渲染为 chip 卡片 + 特殊域名 emoji**：detail.md 里贴的 bare https/http URL 自动升级为「emoji hostname」chip 卡片（点击调 `plugin-opener` 打开默认浏览器，hover 显完整 URL）。19 个常用引用源走专属 emoji —— 🐙 GitHub / 🦊 GitLab / 📐 Linear / 🎨 Figma / 📓 Notion / ▶️ YouTube / 📄 Google Docs / 🗂️ Google Drive / 🐦 Twitter & X / 📚 Stack Overflow / 📦 npm / 🟧 HN / 👽 Reddit / 📜 arXiv / 🌐 Wikipedia / ✍️ Medium 等，未命中 fallback 📎 通用。匹配按"完全相等"或"以 `.<root>` 结尾"双语义，让 `gist.github.com` / `api.github.com` 等子域名都共享父域 emoji。`www.` 前缀剥归一。negative lookbehind 排除 markdown 链接 `[text](url)` 里的 url —— 那种有显式锚文本路径仍走 parseMarkdown 标准超链接渲染。trailing 标点（句号 / 逗号 / 引号）剥到尾巴让"看这里 https://a.com。"不把"。"吃进 URL。让 detail.md 里的引用链接以附件形态独立呈现且类型一眼可读。
- **任务详情图片懒加载**：detail.md 渲染层把内嵌图片用 IntersectionObserver 包起来 —— 距 viewport 300 px 内才把 `<img src>` 挂上去触发 decode；未挂载阶段渲染 `maxSize × 0.6` 占位 div（虚线边框 + 🖼 + "懒加载"小字）保 layout reservation，加载完成时位移最小。原生 `loading="lazy"` 对 data URL 无效（不走网络只走 decode），必须靠 IO 控制 mount —— 一条含 8 张 200 KB JPEG 的 detail 打开瞬间不再 decode 1.6 MB base64 卡 paint，首屏只 decode 可见的几张。点击占位 = 强制加载 + 打开 lightbox，让用户能主动戳穿懒加载。ChatMini / 工具卡片调用点不传 lazy 参数保持原行为。
- **可勾选 todo checkbox**：detail.md 渲染层把 `- [ ]` / `- [x]` / `- [X]` 行解析为 native `<input type="checkbox">` + 标签；编辑模式（split / preview）下勾选直接 toggle 源 markdown `[ ]↔[x]`（functional setState 保多次点击安全），勾上后文字 line-through 视觉对齐；只读 detail 视图渲染同款 checkbox 但 disabled —— 让任务真的能"勾掉"，GitHub / Obsidian / Notion 习惯无缝迁移。
- **本地数据目录可见**：「设置」面板新增「本地数据目录」section，显示绝对路径（如 `~/.config/pet/`）+「在 Finder 中打开」+「复制路径」按钮，下面解释 `config.yaml` / `SOUL.md` / `memories/` / `sessions/` 各自存什么；备份 / 迁移 / 排查时不必再翻 docs。
- **视觉占用控制**：单任务长描述（> 200 字）默认折叠到前 120 字 + "展开 (N 字)"按钮（搜索命中时强制展开避免高亮被遮蔽）；butler_tasks 的"最近执行"section > 5 条时显前 5 + "展开全部 N 条"按钮 — 长 session 下面板不再被冗长内容压扁。

### 5. 多端接入
- **桌面**：主窗口 + 面板窗口 + 调试窗口（应用日志 / LLM 日志 / 统计三个 tab，统计页把 cache / LLM 决策 / 环境工具 / prompt 倾向 / 心情前缀 / 陪伴等计数器拆成独立卡片，不再挤在一行 chip 上）。
- **Telegram Bot**：在设置中填入 Bot Token 后，宠物的主动发言会同步到 TG，回复也会回流到桌面会话。

### 6. 深色 / 浅色主题
- 面板窗口右上角 ☀️/🌙 一键切换；偏好持久化到 `localStorage`，重启保留。
- 基于 CSS 变量的设计令牌系统（6 个 framework token + 6 对 section tint），主题切换不触发 React 重渲染。
- 全部主面板（聊天 / 任务 / 调试 / 记忆 / 设置 / 人格）已完成 dark 适配；状态色 / 错误色 / 成功色 / 高风险审核 / 心情 motion 等"语义信号"色跨主题保持一致，不被主题覆盖。
- **视觉抛光（第一波）**：Panel 底层叠加两团 5-7% accent 径向光晕给整面板「温度」；tab bar 顶部 4% accent 渐变带玻璃感 + `backdrop-filter saturate`；active tab 指示器 halo 10px 60% accent。SectionTitle 圆点升级为 radial highlight + glow，主标字号 13 → 14。各 panel section 卡片统一为 12px radius、顶端 3% accent 渐变 + sm shadow（PanelMemory / PanelSettings / PanelPersona / PanelTasks formCard 同语言）。Modal backdrop 加 `blur(6px) saturate(120%)`、卡片顶部 accent 渐变 + 14 radius；EmptyState icon 套 accent halo 圆形容器。新增 `.pet-card-elev` / `.pet-chip` / `.pet-divider` / `.pet-row-hover` 等 utility class 供后续增量迁移。`prefers-reduced-motion` 全面退化 transition / animation。

### 7. 键盘速查
- 面板内任一 tab 按 `?` 弹出快捷键帮助层（也可点 tab bar 上的「?」），集中列出 ⌘F / `/` 搜索、`n` 新建任务、↑↓ Home End 焦点导航、空格选中、Enter 展开、`d` 标 done、`r` 重试、`p` 切换 pinned（与右键菜单「📌 钉住」对偶；所有 status 都响应）、Delete 取消、Esc 关弹窗等。新增快捷键时帮助层会同步更新。
- `⌘1` – `⌘5`（含 Ctrl 等价）跳到对应 tab（设置 / 聊天 / 任务 / 记忆 / 人格），Chrome / Slack / Linear 风的肌肉记忆；输入框聚焦时自动让出键位不打扰打字。调试窗口同款 `⌘1` – `⌘4` 跳到（应用 / 日志 / LLM 日志 / 统计）。聊天页 `⌘N` 新建会话 / `⌘K` 召唤任务 ref picker / `⌘B` 切上一会话，全局 + textarea 双接入。
- **搜索框三件套**：记忆 / 任务 / 跨会话搜索三处共享一套行为 —— `Esc` 非空时清掉 query 保持焦点；`Enter` 把 query 入历史 datalist，再次聚焦输入框时浏览器原生浮自动补全下拉。
- **聊天输入历史栈**：桌面 pet 窗输入框与面板大聊天框共用 shell 风发送历史（cap 20 · dedup · localStorage 持久），空输入或正在浏览历史时按 `↑` 拉上一条、`↓` 反向；跨窗口 / 重启后仍在。

### 8. 持久化分层
- **memory**：只承担"大模型记忆/回想"职责 —— 长期用户偏好（`user_profile`）、宠物自我画像 / daily_plan / daily_review（`ai_insights`）、其它技能性知识（`general`）。
- **SQLite (`~/.config/pet/pet.db`)**：业务态独立存表。`butler_tasks` / `todo` / `task_archive` 各自一张表（schema 复用 memory 字段：title / description / status / detail_path / tags / created_at / updated_at），`mood` / `persona_summary` / `daily_plan` / `daily_review_<date>` 等单值条目走 `kv_state` 公共表。
- **专用 LLM 工具**：`butler_task_edit` / `todo_edit` 替代 `memory_edit("butler_tasks"/"todo", ...)`。**LLM 面向的 `memory_edit` 现在硬拒 `butler_tasks` / `todo` / `task_archive` 三个 category**（schema enum 把它们移除，运行时仍拦截一道，错误信息指向对应专用工具），实现"LLM 通过专用工具读写各域，不再共用 memory_edit"的架构边界。前端 `invoke('memory_edit', ...)` 走 Tauri 命令路径不受影响。PanelDebug 顶部「🛠 专用工具占比」chip 实时显示 LLM 切换效果（稳态期望 `memory_edit_butler_count` / `memory_edit_todo_count` 为 0）。

## 技术栈

| 层 | 技术 |
| --- | --- |
| 前端 | React 19 + TypeScript + Vite |
| 形象 | pixi.js 7 + pixi-live2d-display-lipsyncpatch |
| 后端 | Rust + Tauri 2（macos-private-api） |
| LLM | OpenAI 兼容协议（reqwest + 流式 SSE） |
| 工具 | 自研 tool registry + rmcp（MCP 客户端） |
| 通讯 | teloxide（Telegram Bot） |

后端模块概览见 [`src-tauri/src/lib.rs`](src-tauri/src/lib.rs)。

## 目录结构

```
.
├── src/                    # 前端 (React)
│   ├── App.tsx             # 主窗口（桌面宠物）
│   ├── PanelApp.tsx        # 面板窗口（聊天 / 设置 / 记忆 / 调试）
│   ├── DebugApp.tsx        # 调试窗口
│   ├── components/         # UI 组件（含 Live2D、气泡、面板）
│   └── hooks/              # 自定义 hooks
├── src-tauri/              # 后端 (Rust / Tauri)
│   └── src/
│       ├── commands/       # Tauri 命令（前端调用入口）
│       ├── proactive/      # 主动发言引擎
│       ├── tools/          # 内建工具集
│       ├── mcp/            # MCP 客户端
│       ├── telegram/       # Telegram Bot
│       ├── mood*.rs        # 情绪系统
│       └── …               # 记忆 / 反馈 / 决策日志 / 输入空闲检测 / 任务队列
├── docs/                   # 产品文档 (GOAL / TODO / 已完成迭代记录)
└── public/                 # Live2D 模型与静态资源
```

## 快速开始

### 环境要求
- Node.js 18+ 与 [pnpm](https://pnpm.io/)
- Rust 工具链（`rustup`）
- macOS（其它平台未做适配，依赖 `macos-private-api`）

### 安装与运行

```bash
pnpm install
cp .env.example .env        # 填入你的 OPENAI_API_KEY
pnpm tauri dev              # 开发模式启动
# 或重启已在运行的实例：
pnpm relaunch
```

### 配置

- **`.env`**：模型与 API 密钥
  ```
  OPENAI_API_KEY=sk-...
  OPENAI_BASE_URL=https://api.openai.com/v1
  OPENAI_MODEL=gpt-4o-mini
  ```
- **应用设置**：通过面板窗口的「设置」页编辑，运行时持久化到用户配置目录。
- **MCP 服务器 / Telegram Bot**：同样在设置面板中配置。

### 构建发布版

```bash
pnpm tauri build
```

产物位于 `src-tauri/target/release/bundle/`。