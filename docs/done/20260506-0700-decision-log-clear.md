# 决策日志清空按钮 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 决策日志清空按钮：调试面板里 decisions 是 in-memory 列表，重启清空但运行期会越攒越多；加一个「清空」按钮让 prompt 调试时能清掉无关历史聚焦在新跑的几条上。

## 目标

`DecisionLog` 是 16 条 ring buffer，会循环 drop 旧条目，但调 prompt 时用户
往往想"现在我点 立即开口 → 看新 push 进来这几条"，旧的 14 条会让定位新条目
费眼。本轮加一个"清空"按钮，一键把 in-memory ring buffer drain 到 0，让用户
后续 push 的条目从顶部开始。

## 非目标

- 不持久化清空（重启进程相当于自然清空）。
- 不清空决策日志之外的 in-memory 数据（speech_history / mood_history /
  feedback_history 都有自己的 IO + 自管入口，本轮只动 decision_log）。
- 不写 README —— 调试器维护补强。

## 设计

### 后端

`DecisionLog` 加 `clear()` 方法 + `clear_proactive_decisions` Tauri 命令：

```rust
impl DecisionLog {
    pub fn clear(&self) {
        if let Ok(mut g) = self.buf.lock() {
            g.clear();
        }
    }
}

#[tauri::command]
pub fn clear_proactive_decisions(store: tauri::State<'_, DecisionLogStore>) {
    store.clear();
}
```

注册到 `lib.rs::run` 的 invoke_handler。

### 前端

`PanelDebug.tsx` 决策日志段：在 `PanelFilterButtonRow` 旁边加一个「清空」小
按钮 → invoke + `setDecisions([])` 立刻清空本地 React 状态（避免等下一次
fetch 回填的滞后）。

### 测试

后端：新增单测 `clear_drains_buffer`（push 后 clear → snapshot 空）。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `DecisionLog::clear` + Tauri 命令 + 单测 + 注册 |
| **M2** | PanelDebug 「清空」按钮 + handler |
| **M3** | cargo test + tsc + build + cleanup |

## 复用清单

- 既有 `DecisionLogStore` / Tauri State pattern
- PanelDebug 既有 `setDecisions` / 决策段布局

## 待用户裁定的开放问题

- 清空是否需要二次确认？决策日志是 in-memory ring buffer 不保存任何用户数据，
  失误清空只丢 16 条 debug 痕迹，零风险 —— 不加确认。

## 进度日志

- 2026-05-06 07:00 — 创建本文档；准备 M1。
- 2026-05-06 07:10 — 完成实现：
  - **M1**：`decision_log.rs` 加 `DecisionLog::clear()` 方法 + `clear_proactive_decisions(store)` Tauri 命令；新增 1 条单测覆盖 push → clear → snapshot 空 + clear 后仍可继续 push（验证 mutex 健康）。注册到 lib.rs。
  - **M2**：`PanelDebug.tsx` 决策日志段标题行加「清空」按钮（仅 decisions.length > 0 时显示）；onClick → invoke + `setDecisions([])` 即时清空本地 React 状态避免等下一次 fetch 回填的滞后。
  - **M3**：`cargo test --lib` 888/888（+1）；`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 调试器维护补强。
  - **设计取舍**：不做二次确认（决策日志是 16 条 in-memory ring buffer 不存用户数据，失误清空零风险）；前端 setDecisions([]) 同步即时反馈而非等 fetch（避免感知滞后）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；后端有单测，前端 invoke + setState 链路简单。
