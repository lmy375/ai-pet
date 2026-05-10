# 删除专注模式相关功能与代码

> 对应需求（来自 docs/TODO.md）：
> 删除关于专注模式的相关功能和代码。注意要清理干净，同时保持代码规范。

## 范围

GOAL.md 明确「不要考虑专注模式相关的需求」，但近 20 多次 iter（R62–R75）违反此约束加入了大量深度专注追踪 + macOS Focus 联动逻辑。本轮一次性清理：

1. macOS Focus / 勿扰联动
   - `src-tauri/src/focus_mode.rs`（整个模块）
   - `src-tauri/src/focus_tracker.rs`（整个模块）
   - `Settings.proactive.respect_focus_mode`（默认值 + 字段）
   - `proactive::gate` 内的 focus-mode 跳过分支与对应单测
   - `proactive::Persona.focus_mode` 字段 + tone snapshot 投影
   - `proactive` 模块 prompt 注入用到的 focus_status 调用
   - 前端 `PanelSettings` 内的 `respect_focus_mode` checkbox
   - 前端 `PanelToneStrip` 内的 🎯 focus chip

2. 深度专注（deep focus）追踪
   - `proactive/active_app.rs` 中 `compute_deep_focus_block` + `format_deep_focus_recovery_hint` + DeepFocusHistory 系列
   - `proactive/gate.rs` 内 hard-block gate 与对应单测
   - `proactive/prompt_assembler` 的 `deep_focus_recovery_hint` 与 yesterday 段
   - `proactive.rs` 的 today / weekly / wow trend 字段 + load_block_history_into_memory
   - 前端 `PanelStatsCard` deep-focus 段（today + weekly column + 趋势）
   - 前端 `panelTypes.ts` 中 deep-focus 相关 type 字段

## 非目标

- 不动「番茄计时」/「免打扰静音」（mute / 🔇）—— 这是用户主动控制宠物声音的入口，与「专注模式自动检测」不是同一概念。
- 不动 `transient note`（📝）—— 用户手写指示，与 focus mode 无关。
- 不动 `urgent_deadline_count` 等 deadline 相关字段。

## 编辑顺序

1. 删除 `focus_mode.rs` / `focus_tracker.rs` 整文件
2. `lib.rs` 移 `mod focus_mode;` / `mod focus_tracker;` / `focus_tracker::spawn`
3. `commands/settings.rs` 删 `respect_focus_mode` 字段 + 默认 + 序列化
4. `proactive/gate.rs` 删 focus-mode 分支 + deep-focus 分支 + 单测
5. `proactive/active_app.rs` 删 deep-focus 计算与历史
6. `proactive/prompt_assembler.rs` 删 deep_focus_recovery_hint 字段
7. `proactive.rs` 删 Persona.focus_mode + deep-focus 历史调度 + load_block_history_into_memory
8. 前端：useSettings / PanelSettings / PanelToneStrip / PanelStatsCard / panelTypes
9. `npx tsc --noEmit` + `cargo check` 验证
10. 手动跑一下面板，确认不白屏（与 TODO #4 一并验证 PanelTasks）

## 风险

- compute_deep_focus_block 被 gate.rs 用作 hard-block 触发器；删后仅靠 cooldown / mute 控制频率 —— 与 GOAL "实时陪伴" 一致，更主动而非更安静。
- yesterday recap 段去掉后 prompt 少一类温和召回触发器；留给后续如有需要再用 mood / butler_history 形式重做。
