# ✨ LLM 重写任务标题

## 背景

TODO 上 auto-proposed 一条："✨ LLM 重写任务标题：任务行右键菜单加入口，调一次 chat completion 让 LLM 看 title + 描述给 ≤ 10 字概括（与会话标题 regenerate 同模板）。"

session 标题 LLM 重写（早前已 ship）解决了"聊几十条后标题仍卡在第一条问题"的问题。任务标题同理：owner 创建任务时可能写"周一会议"，但实际 detail.md 已经记成"周一 standup 提 GrowthBook 灰度问题 + 设计接力"，title 已严重失真。手动想新名是脑力开销 —— 让 LLM 看完整上下文给一个切题的短标题就是天然的解决路径。

## 改动

### Backend（Rust）

#### `src-tauri/src/commands/task.rs`

新 `regenerate_task_title(title)` Tauri 命令。与 `regenerate_session_title(id)` 完全同 IO 模板（非流式 / 30s timeout / temperature 0.3 / max_tokens 30 / 输出清洗 / cap 30 char）：

```rust
#[tauri::command]
pub async fn regenerate_task_title(title: String) -> Result<String, String> {
    // 1. settings 校验（api_key / model 非空）
    // 2. find_butler_task(title) → item
    // 3. 拼上下文：title + description + (best-effort) detail.md 前 600 字
    // 4. 一条 user 消息：「{context}\n\n请用 ≤ 10 字中文给这条任务起一个新标题...」
    // 5. POST chat/completions { messages, max_tokens: 30, temperature: 0.3, stream: false }
    // 6. 解析 choices[0].message.content
    // 7. 清洗：trim 首尾引号 / 句号；换行折空格
    // 8. cap 30 chars
    // 9. memory_rename("butler_tasks", old, new) — atomic 写回
    // 10. return Ok(new_title)
}
```

**关键差异 vs session 版本**：

- **上下文是 title + description + detail.md 前 600 字**（而非"尾 10 条 turn"）—— 任务的语义浓缩在描述 + 进度笔记里，不是聊天历史。detail.md 读失败 / 空时降级到 title + description；既有 task 一定都有 description 兜底。
- **memory_rename 内嵌**：与 session 版"set + save_session" 同思路 —— 让一个原子命令做完"LLM 出标题 → 落盘改名"两件事，前端只 await 一次结果。
- **rename 错误透传**：`memory_rename` 已有"new == old"（noop）/ "重名"等错误分支；这些都原样冒泡给前端 toast，让 owner 看到具体原因（特别是 LLM 输出与既有任务重名时）。

#### `src-tauri/src/lib.rs`

`invoke_handler!` 注册 `commands::task::regenerate_task_title` 紧贴 `task_unarchive`。

### Frontend（TypeScript）

#### `src/components/panel/PanelTasks.tsx`

任务行右键菜单（`taskCtxMenu`）—— 在「📌 钉住」按钮之后、「✓ 标 done」之前插入：

```tsx
<button
  type="button"
  style={{ ...itemBtn, color: "var(--pet-color-accent)" }}
  onClick={async () => {
    setTaskCtxMenu(null);
    setBulkResultMsg(`✨ 正在让 LLM 重写「${m.title}」的标题…`);
    setBusyTitle(m.title);
    try {
      const newTitle = await invoke<string>("regenerate_task_title", { title: m.title });
      setBulkResultMsg(`✨ 已重写标题：${newTitle}`);
      setTimeout(() => setBulkResultMsg(""), 4000);
      await reload();
    } catch (e) {
      setActionErr(`重写标题失败：${e}`);
      setBulkResultMsg("");
    } finally {
      setBusyTitle(null);
    }
  }}
  title="让 LLM 看任务标题 + 描述 + detail.md 前 600 字，给一句 ≤ 10 字新标题，并直接改名。免去手动想新名的脑力开销。"
>
  ✨ LLM 重写标题
</button>
```

## 关键设计

- **accent 配色**：与 「✨ LLM 重写标题」session ctx menu 按钮统一品牌色（强调"AI 智能"动作，区别于 destructive 红 / completion 绿）。
- **toast 即时显"进行中"**：LLM 调用 ~1-3s + 可能费用，让用户立即知道点了什么 / 在做什么。复用既有 `bulkResultMsg` 通道（既"批量重试" / "导出归档"等也走它），UI 一致。
- **`busyTitle` lock**：与"snooze / pin"等右键操作同 lock，防同一条任务点了 LLM 重写又同时点 cancel 等。
- **不暴露 modifier 选项**：先做最简单流程（不可选模型 / 不可选温度 / 不可选 prompt）。需要时再加 modal。
- **失败明示错误来源**：`重写标题失败：${e}` 透传后端原始错误（包含 status + body 前段），让 owner 看到"API 4xx" vs "LLM 返空" vs "重名拒绝"等具体区分。
- **不动 LLM 提示词的"宠物语气"**：标题应贴话题主体而非个性化（与既有 session 版同决策）。

## 不做

- **不批量重写**：一键给所有任务重写要按 N 次 LLM 费用爆炸；任务粒度让 owner 对成本有感。
- **不 cache LLM 输出**：用户可能不满意第一次想再 roll 一遍，cache 会挡住。
- **不接 description 自动 polish**：本入口只改 title。description 由 owner / 宠物在 detail.md 编辑环节自然更新；强行 LLM polish description 可能改掉关键信息。
- **不写测试**：纯 IO（reqwest + memory_rename），既有 `regenerate_session_title` 同 IO 模板已无单测（视觉验证即可）。

## 验证

- `cargo build --lib` ✓ 0 error（新 command 编译通过）
- `cargo test --lib` ✓ **997 / 997 通过**（无新测试也无回归）
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.32s
- 改动 ~140 行（backend command 100 + lib.rs 1 + 前端按钮 30）；既有 task ctx menu / `regenerate_session_title` 路径不动。

## TODO 状态

5 条候选 auto-proposed 已完成 3 条（含 stale 移除 1 条），余 3 条留池：
- mini chat 顶部上下文 token 提示 chip
- PanelChat 顶部「📌 钉住会话」 chip 计数
- detail.md textarea 底部行号 status bar

## 后续

- 抽 `regenerate_session_title` + `regenerate_task_title` 共有的"清洗 trim + 换行替空格 + cap 30 char"成 pure helper + 单测。
- 重写前弹 modal 显 LLM 候选 + 让用户选 / 编辑后再 commit，避免一次失败的 roll 直接覆盖原 title。
- TG bot 也能调（`/rename <title>` 走 LLM rewrite）；目前 desktop-only。
- 给 prompt 加宠物语气（与 SOUL.md 风格一致）让标题略个性化而非完全中立 —— 与既有 session 版同后续 TODO。
