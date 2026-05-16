# pet ctx menu「🎲 摇一摇主动开口」按钮（iter #263）

## Background

PanelMemory 已有"立刻让宠物现在去看任务"的 fire-proactive 按钮（armed 二次
确认），但 owner 在桌面只看到 ChatMini + Live2D 时，要触发一次 proactive
turn 必须切到 panel → memory tab → fire 按钮 三步。

本迭代在 pet 右键聚合菜单加「🎲 摇一摇 主动开口」按钮：armed 二次确认后调
既有 `trigger_proactive_turn` tauri 命令，立即跑一次 proactive turn 绕过
cooldown / quiet hours，结果以 ChatMini assistant 行反馈给 owner。

## Changes

仅 `src/App.tsx`：

- 新增 `shakeArmed: boolean` state + `shakeArmedTimerRef` 3s 还原 timer
  + `fireShakeBusy: boolean` invoke-in-flight 防双触
- 新增 `armShake()` helper：清旧 timer + setArmed(true) + 启 3s setTimeout
  还原
- 在 pet ctx menu 的 📡 ping LLM 与 🔄 重启窗口 之间插「🎲 摇一摇」按钮：
  - 未 armed：click → `armShake()` 进入 armed 红字态
  - armed：click → 清 timer + `invoke("trigger_proactive_turn")` →
    `appendAssistant("✅ ...")` 或 `("❌ 触发失败：...")`
  - busy 期间 disabled + "🎲 跑中…" 标签
  - tooltip 三态文案（busy / armed / idle）说明语义

## Key design decisions

- **armed 二次确认而非直接 fire**：trigger_proactive_turn 绕过 cooldown +
  quiet hours，可能在 owner 不希望被打扰的时刻插队主动开口。armed 保护防
  误触（与 PanelMemory 同 fire 按钮一致风格）。3s 自动还原是经验值，足够
  owner 想清楚是否确认。
- **复用既有 trigger_proactive_turn 命令**：后端已在 lib.rs 注册并被
  PanelMemory / PanelDebug 调用；不需要新 IPC。
- **结果走 appendAssistant 而非 toast**：proactive turn 完成后通常宠物有
  emit 一条新消息，appendAssistant 文本（"✅ status..."）跟新 bubble 并列
  在 ChatMini，让 owner 看完整 trace 上下文。
- **位置选 📡 ping 与 🔄 重启 之间**：📡 ping 是诊断、🔄 重启是兜底；
  🎲 摇一摇 是主动行为，介于"检测 → 干预 → 重置"光谱中间合理。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
