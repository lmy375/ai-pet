# PanelTasks 行右键菜单加「🔇 Toggle silent」+ 后端 task_set_silent 命令

## 背景

iter #193 引入 `[silent]` marker（butler_task 描述里的 owner 意图标记，让 LLM proactive cycle 不主动选）。owner 只能通过编辑 description 字符串加/删 marker —— 麻烦。

加一个 `task_set_silent` Tauri 命令 + PanelTasks 行右键菜单按钮，一键 toggle silent 状态，与既有 `task_set_pinned` (📌 钉住) 对偶。

## 改动

### Backend

#### `src-tauri/src/task_queue.rs` — `strip_silent_markers` helper

```rust
pub fn strip_silent_markers(desc: &str) -> String {
    let cleaned = remove_bracketed_segments(desc, &["[silent]"]);
    collapse_whitespace(&cleaned)
}
```

与 `strip_pinned_markers` 同一模板。

#### `src-tauri/src/commands/task.rs` — `task_set_silent` command

```rust
#[tauri::command]
pub fn task_set_silent(title: String, silent: bool) -> Result<(), String> {
    let title_trim = title.trim();
    if title_trim.is_empty() { return Err("title is required".to_string()); }
    let item = find_butler_task(title_trim)
        .ok_or_else(|| format!("task not found: {}", title_trim))?;
    let stripped = crate::task_queue::strip_silent_markers(&item.description);
    let new_desc = if silent {
        let base = stripped.trim_end();
        if base.is_empty() { "[silent]".to_string() } else { format!("{} [silent]", base) }
    } else { stripped };
    memory::memory_edit("update".into(), "butler_tasks".into(), item.title.clone(),
                       Some(new_desc), None)?;
    Ok(())
}
```

与 `task_set_pinned` 完全镜像 —— strip 旧 + append 新 + atomic memory_edit。

#### `src-tauri/src/lib.rs` 注册

```rust
commands::task::task_set_silent,
```

#### 3 个新单测

- `parse_silent_strict_literal`：严格字面，大小写敏感，拒 `[silent: reason]` 变体
- `strip_silent_markers_removes_and_normalizes`：常规剥 + 多 marker + 空白归一
- `strip_silent_markers_preserves_other_markers`：剥 silent 不动 [task pri=] / [pinned] / [snooze:] / [origin:tg:]

跑 `cargo test --lib task_queue::tests::parse_silent / strip_silent` ✓ 3 passed。

### Frontend

`src/components/panel/PanelTasks.tsx` —— 右键菜单按钮（紧贴 📌 钉住 按钮后）

```tsx
{(() => {
  const isSilent = !!t?.raw_description?.includes("[silent]");
  return (
    <button
      style={{ ...itemBtn, color: isSilent ? accent : muted }}
      onClick={async () => {
        setTaskCtxMenu(null);
        setBusyTitle(m.title);
        try {
          await invoke<void>("task_set_silent", { title: m.title, silent: !isSilent });
          await reload();
        } catch (e) {
          setActionErr(`silent 切换失败：${e}`);
        } finally { setBusyTitle(null); }
      }}
      title={isSilent ? "已标 [silent] —— 点击解除..." : "标 [silent] —— LLM 不再主动选..."}
    >
      {isSilent ? "🔇 解除 silent" : "🔇 标 silent"}
    </button>
  );
})()}
```

- 从 `t.raw_description` 读 silent 状态（与 `t.pinned` 字段对应位置 —— pinned 是后端解析后的 bool，silent 暂无 field）
- 按钮 label / color 反映"当前状态 + 将切换的方向"，与 📌 钉住按钮同模板

## 关键设计

- **frontend 用 raw_description regex 探**：未给 TaskView 加 `silent: boolean` 字段（避免改 backend 序列化协议 + 兼容老 session）；raw_description 已含完整 markers，inline `includes("[silent]")` 一行查询。`pinned` 字段在 iter Cπ 加了，但 silent 是新 marker，可以慢慢演进 —— 当前一行 regex 够用。
- **mirror task_set_pinned 模板**：strip-before-write / atomic memory_edit / no decision_log / Result error string —— 完全照搬。owner / LLM 调同一通命令族，行为可预测。
- **3 个单测 pin 真行为**：parse 严格性 + strip 归一 + 不动其它 markers —— 与既有 strip_pinned_markers 测试同等覆盖。每个测试都断言一个实际可能写错的 bug。
- **不推 decision_log**：与 pinned / due / snooze 一致 —— silent 是 owner 偏好不是状态转移，不需 audit。
- **按钮放在 📌 钉住 之后**：两个都是"owner 意图标记" toggle，与 markdone / cancel / retry 这种"状态转移" 操作分组。

## 不做

- **不在 TaskView 加 `silent: boolean` field**：raw_description regex 一行查询足够；多一个字段就要后端序列化 + frontend filter chain 多一处兼容。等真有 3+ 处需要这个 bool 时再 lift。
- **不让 silent 任务在 PanelTasks 隐藏**：与既有 pinned filter 类似，silent 任务面板仍可见；filter chip 等下一 iter 加（独立工作）。
- **不在 Telegram bot 加 /silent 命令**：相对 panel 频率低；TG 已有 /pin / /snooze 命令族，silent 可后续 batch 加（与既有命令同 dispatch 路径）。
- **不写 frontend 测试**：无 test runner（与项目惯例一致）。

## 验证

- `cargo check` ✓ (7 既有 warning，无新 error)
- `cargo test --lib task_queue::tests::parse_silent / strip_silent` ✓ 3 新单测 passed
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.18s
- 改动 ~140 行（task_queue strip helper 7 + 单测 50 + task.rs task_set_silent command 30 + lib.rs 注册 1 + PanelTasks button 50 + 注释 2）。既有 task_set_pinned / task_set_snooze / task_set_due / 右键菜单其它 entries / pinned filter chip / silent 行 chip / silent count header chip 完全不动。

## TODO 状态

剩 2 条留池：
- 桌面 pet collapse tab hover 1s 浮 ambient mini card
- butler_task `[snooze: ...]` 支持自然短串预设

## 后续

- Telegram bot `/silent <title>` 命令加入（与 /pin 同 dispatch 路径 + 同 strip-before-write 模板）。
- PanelTasks 顶部加 "🔇 N silent" filter chip（与 📌 N pinned chip 同模板）—— 让 owner 一键查看自己标过的所有 silent 任务。
- TaskView 加 `silent?: boolean` field 后，把 inline regex 改成读 field —— 但等 backend 有第二处需要才迁。
- LLM 端的 `butler_task_edit` tool 描述里列出 [silent] marker，让宠物 自己也能标 silent —— 但要小心 LLM 不该自由 toggle owner 意图，可能需要"LLM 只能写 silent，owner 才能解除"的 invariant。
