# PanelDebug 「🔄 reset ⚙️」 按钮（iter #438）

## Background

iter #431 加 ⚙️ shell exit code chip 显近 1h ShellStore 缓存的
success / failure / running_or_unknown 分布。但 debug 时 owner 想
「从这里开始重测」（如改了 prompt 重 trigger LLM shell 用法 → 想
独立 audit 新一轮的失败率不被旧累积污染）— 当前要等 cleanup_old_tasks
1h cutoff 才会自然清空。

本 iter 加显式 reset 按钮，让 owner 立即清空 ShellStore 内
**finished** task（running 保留 — 仍在执行的子进程状态需被
check_shell_status 继续观察）。

## Changes

### `src-tauri/src/commands/shell.rs`

```rust
#[tauri::command]
pub fn reset_shell_store(store: State<'_, ShellStore>) -> u32 {
    let mut map = store.0.lock().unwrap();
    let to_remove: Vec<String> = map.iter()
        .filter(|(_, t)| t.status == TaskStatus::Finished)
        .map(|(id, _)| id.clone())
        .collect();
    let removed = to_remove.len() as u32;
    for id in to_remove {
        if let Some(task) = map.remove(&id) {
            let _ = std::fs::remove_file(&task.stdout_path);
            let _ = std::fs::remove_file(&task.stderr_path);
        }
    }
    removed
}
```

设计：
- **仅删 finished**：running 保留 — 与 cleanup_old_tasks 同策略
  避免让正在跑的 shell tool 失联（check_shell_status 找不到 task
  会让 LLM 工具调用流程崩）
- **删 stdout/stderr 文件**：与 cleanup_old_tasks 同 finalize
  pattern；防遗弃孤儿文件
- **返清掉数 u32**：前端用作 toast 反馈（"已清 N 条"）让 owner
  确认动作生效

### `src-tauri/src/lib.rs`

注册 `reset_shell_store` 到 invoke handler list。

### `src/components/panel/PanelDebug.tsx`

紧贴 ⚙️ shell chip 之后加 🔄 按钮：

```tsx
{shellExitStats.success + shellExitStats.failure > 0 && (
  <button
    onClick={async () => {
      try {
        const removed = await invoke<number>("reset_shell_store");
        setDebugExportMsg(`🔄 已清 ${removed} 条 finished shell task`);
      } catch (e) {
        setDebugExportMsg(`重置 shell stats 失败：${e}`);
      }
      setTimeout(() => setDebugExportMsg(""), 3000);
    }}
    title="清 ShellStore 内已完成的 shell task..."
  >
    🔄 reset ⚙️
  </button>
)}
```

设计要点：
- **gate by success + failure > 0**：仅 finished 计数 > 0 时显
  按钮，没什么可清时不渲免 dead UI（running 计数不算 — running
  本来就不可清，显按钮误导）
- **复用 debugExportMsg 反馈通道**：与既有 📋 导出快照 toast 同
  channel — 不引第二条反馈系统
- **3s toast 自清**：与既有 export feedback 同 disappear pattern
- **emoji 🔄 + label "reset ⚙️"**：let owner 一眼连到 ⚙️ chip
  对偶语义；3s 内点完即可看 ⚙️ chip 计数归零

## Key design decisions

- **不清 running**：与 cleanup_old_tasks 同；强清 running 会让
  正在 await check_shell_status 的 LLM tool call 错失结果
- **不暴露 dry-run / preview**：单纯 cleanup 不破坏不可逆 task
  本身（task 已 finished），只清缓存 + stdout/stderr 文件。删除
  这些不影响 task 已写到 log / decision_log 的事件记录
- **不为 reset 触发 ⚙️ stats 立即刷**：30s 轮询会拉 — 短延迟可
  接受（owner 看到 toast 后等 ⚙️ chip 在下个 tick 归零）；强制
  refresh 引入额外 invoke 不划算
- **不为单按钮引 unit test**：纯 HashMap 删除 + setState；
  cleanup_old_tasks 既有逻辑覆盖 finished + 文件清理边界；
  build pass + 手测足够（让 LLM 跑几条 shell → 看 ⚙️ chip 显
  → 点 🔄 → 看 toast "已清 N 条" → 等 30s 看 ⚙️ chip 消失）

## Verification

- `cargo build --lib`（backend）— clean（仅 pre-existing 8 warnings）
- `cargo test --lib`（全表）— 1483 / 1483 通过
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.46s)
