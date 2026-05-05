# PanelDebug "立即开口" 二次确认（Iter R125）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug "立即开口" 按钮二次确认：现是单击直触，demo / 演示场景常误点，干扰真实 cooldown / quiet hours 节奏。改成首次点击 → armed 态（3s 内再点确认 + 红字提示），3s 后自动 revert（与决策日志"清空"按钮 R83 二次确认同模式）。

## 目标

`立即开口`（顶部 toolbar）调 `trigger_proactive_turn`，绕过 idle / cooldown
/ quiet / focus 等闸门强制 LLM 跑一次主动开口。demo 时常需，但偶尔在
真实使用中误点 → 把宠物的"该不该开口"判断节奏打乱。

加 2 步确认（mirror R83 决策日志清空按钮模式）：
- 第 1 击：armed 态，按钮变 amber + 文案 "再点确认 (3s)"
- 第 2 击（3s 内）：真触发
- 3s 超时：自动 revert 到 idle（防长时间挂着误点）

## 非目标

- 不动决策日志行的 "重跑" 按钮（line 1803-）—— 那是 power-user 调试场景，
  快速迭代 prompt 时连续点击；二次确认会拖慢工作流
- 不 confirm 触发后的整个 turn 流程 —— 只 confirm 第一步入口
- 不持久化 armed 状态 —— session 内瞬时

## 设计

### state

```ts
const [triggerArmed, setTriggerArmed] = useState(false);
```

### handler 改造

```ts
const handleTriggerProactiveClick = () => {
  if (!triggerArmed) {
    setTriggerArmed(true);
    window.setTimeout(() => setTriggerArmed(false), 3000);
    return;
  }
  setTriggerArmed(false);
  void handleTriggerProactive(); // 真正调 trigger_proactive_turn
};
```

`handleTriggerProactive`（既有）保留不动；包一层 click handler 做 armed
门控。决策日志行的 "重跑" 仍直接 `onClick={handleTriggerProactive}` 不走
门控。

### 渲染

toolbar 按钮三态：
- `triggeringProactive`（请求进行中）→ 灰背景 + "开口中…"，disabled
- `triggerArmed`（armed 等确认）→ amber 背景 + "再点确认 (3s)" + 红边框
- 默认 → 绿色 "立即开口"

```tsx
<button
  onClick={handleTriggerProactiveClick}
  disabled={triggeringProactive}
  title={
    triggeringProactive
      ? "正在调 trigger_proactive_turn…"
      : triggerArmed
        ? "再次点击立即触发主动开口（3s 内有效）"
        : "绕过 idle/cooldown/quiet/focus 等闸门，立刻让宠物跑一次主动开口检查（用于测试 prompt 或现场 demo）。点击后 3s 内需再点确认，防误触。"
  }
  style={{
    ...toolBtnStyle,
    background: triggeringProactive
      ? "#94a3b8"
      : triggerArmed
        ? "#fef2f2"
        : "#10b981",
    color: triggeringProactive
      ? "#fff"
      : triggerArmed
        ? "#b91c1c"
        : "#fff",
    borderColor: triggerArmed ? "#dc2626" : undefined,
    fontWeight: triggerArmed ? 600 : undefined,
  }}
>
  {triggeringProactive
    ? "开口中…"
    : triggerArmed
      ? "再点确认 (3s)"
      : "立即开口"}
</button>
```

armed 视觉用红色（与"危险动作"语义一致），与默认绿色（"安全的常规动作"）
形成反差，让用户视觉上看到状态变化。

### 测试

无单测；手测：
- 默认绿色 "立即开口"
- 点 → 红色 "再点确认 (3s)"
- 3s 内再点 → 真触发，状态回绿、按钮变灰 "开口中…" → 完成回绿
- 等 4s 后再点 → 重新进入 armed（重新 3s 窗口）
- 点 1 次后离开 panel → state 在卸载时丢失（与 React 树同生命周期，无副作用）
- 决策日志行的 "重跑" 仍单击直触，不受影响

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + click 包装 + 渲染三态 |
| **M2** | tsc + build |

## 复用清单

- 既有 `handleTriggerProactive`（保留直触语义给"重跑"路径）
- 既有 `clearDecisionsArmed` 二次确认 R83 模式
- 既有 `triggeringProactive` 进行中状态

## 进度日志

- 2026-05-10 06:00 — 创建本文档；准备 M1。
- 2026-05-10 06:08 — M1 完成。`triggerArmed` state 加在 `triggeringProactive` 旁；toolbar 按钮 onClick 改包装：if !armed → setArmed + 3s setTimeout revert + return；else clear armed + void handleTriggerProactive。三态视觉：进行中灰 / armed amber+红边 / 默认绿。决策日志行 "重跑" 不动（仍直触 handleTriggerProactive，power-user iterate prompt 工作流）。
- 2026-05-10 06:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
