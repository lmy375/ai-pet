# 任务完成时让宠物主动开口报喜

> 对应需求（来自 docs/TODO.md）：
> 任务执行落 done 时让宠物主动开口确认产物：butler_task 状态变更接 proactive
> 通道发一句简短报喜，与现有 mute / cooldown gate 协同。

## 现状

宠物管家任务已经能：用户说 → LLM 用 `task_create` 落盘 → 宠物在
`butler_tasks` 类目里见到 → 在 proactive 里被 `[心跳]` / 全景列表持续提
醒 → LLM 在某次聊天里用 `memory_edit` 把描述更新成含 `[done]` 标记。

但 done 之后没有任何回路 —— 用户在 panel 任务页里翻能看到「✓ 产物：
xxx」，但桌面气泡上不会冒出「我整理完了 38 个文件，归档到 ~/Archive 了」
这种确认。GOAL.md 的「通用任务」要的正是「能感觉到工作被完成的宠物
管家」，缺这一步整个产品体验是断的。

## 设计

接住一个轻量的「转 done 检测」hint，和其它 prompt 提示同形式注入到
proactive 流：

1. 新增静态 `LAST_SEEN_BUTLER_DONE_TITLES: Mutex<HashSet<String>>`，
   保存上次 proactive tick 看到处于 done 的 butler_task 标题集合。
2. 新增纯 helper `compute_recent_task_completions(items, prev_seen)` →
   返回 `(new_completions, updated_seen)`。`items` 是 `(title, description)`
   对，`prev_seen` 是上次的标题集合。任意「title 当前是 done 但不在
   prev_seen 里」的条目算 new；返回的 `updated_seen` 是当前所有 done
   的集合（替换 prev）。fully pure，方便单测覆盖。
3. 新增纯 helper `format_task_completion_hint(new_completions)` →
   格式化成一行 `[任务刚完成] 你刚标了 N 条 done：· title1 (产物: ...) · …`
   或 N=0 → 空串。
4. `proactive.rs::run_proactive_turn` 在已有 `build_butler_tasks_hint`
   之后追加 `let task_completion_hint = build_task_completion_hint();`
   IO 包装：读 butler_tasks → 抽 done items → 调 helper → 写回静态。
5. PromptInputs 新增 `task_completion_hint: &'a str`。
6. `proactive/prompt_assembler.rs` 在原有 `butler_tasks_hint` 一行下
   面 `push_if_nonempty(&mut s, inputs.task_completion_hint)`。
7. 单测：覆盖 helper 的「首启后所有 done 都算 new」、「连续两 tick 同
   一条 done 只 fire 一次」、「title 不在 done 时不出现在 new」、result
   有无 / 描述截断。

## 协同 mute / cooldown

完全不需要单独 gate —— 走 proactive 通道意味着所有现有 gate（mute /
cooldown / quiet hours / awaiting_user_reply）天然作用在它身上。muted 时
本来就不会进入 LLM；解 mute 后下次 tick 仍能 catch up（因为 prev_seen
没更新）。

## 风险

- 用户连续标多条 done → hint 列表会变长。format 时截断到 5 条 + 「…还有
  N 条」简短化。
- LLM 不一定每次都 reply 这条 hint —— 它可以选择 silent 或聊别的。这是
  期望行为：和现有"开口与否由 LLM 判断"原则一致，避免每次任务完成都强制
  打扰。

## 非目标

- 不接 telegram 通道；那条已有自己的 inject_*_layer 对应路径，本轮不展。
- 不写 frontend chip；任务页的 ✓ 产物 行已经能让用户在 panel 里看到。
