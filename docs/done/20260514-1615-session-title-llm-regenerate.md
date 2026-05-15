# 会话标题 LLM 自动重写按钮

## 背景

TODO 最后一项（上几轮 auto-proposed）：

> 会话标题 LLM 自动重写按钮：session 右键菜单加"✨ 重写标题"，让 LLM 看最近 5-10 条 turn 给一句 ≤ 10 字概括，替代"首条 user 消息前 20 字"硬截。

session.title 当前由 saveCurrentSession 自动生成 —— "首条 user 消息前 20 字 + …"。新建 session 时挺合理；但用户聊几十条后话题早飘了，标题仍卡在第一条问题上，list 视图里失去标识度。20+ session 的 power user 找"那条关于 weekly report 的会话"全靠人脑回想第一条说了什么。

让 LLM 看尾部 turn 给个 ≤ 10 字概括解决这个问题：标题贴话题主体，不再被开场白绑架。

## 改动

### Backend（Rust）

#### `src-tauri/src/commands/chat.rs`

新 `regenerate_session_title` Tauri 命令，与既有 `chat_test` 同模板（非流式 POST / 30s timeout / 短 max_tokens / status 透传）：

```rust
#[tauri::command]
pub async fn regenerate_session_title(id: String) -> Result<String, String> {
    // 1. settings 校验（api_key / model 非空）
    // 2. load_session
    // 3. 抽 user / assistant turn 的纯 text（multipart content → text 段拼），
    //    过滤空，每条 cap 400 字
    // 4. 取尾部 ~10 条 turn
    // 5. append 一条 user role 指令："用 ≤ 10 字概括上面这段对话的主题..."
    // 6. POST chat/completions { messages, max_tokens: 30, temperature: 0.3, stream: false }
    // 7. 解析 choices[0].message.content
    // 8. 清洗：trim 首尾引号（"/'/“/”/‘/’/.) + 句号（./。）+ 换行替空格
    // 9. cap 30 chars
    // 10. 写回 session.title + save_session
    // 11. return Ok(title)
}
```

**关键设计**：

- **不走 chat_pipeline（不注入 tool / system / persona / mood / deadline / telegram_dispatch / mood_note 等 layer）**：本调用是"总结历史"工具调用，宠物自我画像 / 工具用法等 layer 注入只会污染 prompt。bare-bones 几条 raw user/assistant 即可。
- **temperature 0.3**：标题风稳定；不要每次跑出不同 wording。
- **max_tokens 30**：硬上限防 LLM 罗嗦超出标题语义。
- **清洗 + cap**：LLM 偶尔会加引号 / 句号 / 表情；首尾 trim + char cap 兜底。换行替空格（不是删除）保多行标题信息不丢。
- **同步 save_session**：写回 session 文件 + index meta（既有 save_session 路径就做）；不另开 IPC。

注册到 lib.rs `invoke_handler!` 紧贴 `chat_test`。

### Frontend（TypeScript）

#### `src/components/panel/PanelChat.tsx`

session tab 右键菜单（`sessionTabCtxMenu`）紧贴"📋 复制标题"按钮之后追加：

```tsx
<button
  onClick={async () => {
    setSessionTabCtxMenu(null);
    setExportToast(`✨ 正在让 LLM 重写「${m.title}」的标题…`);
    try {
      const newTitle = await invoke<string>("regenerate_session_title", { id: m.id });
      const idx = await invoke<SessionIndex>("list_sessions");
      setSessionList(idx.sessions);
      if (m.id === sessionId) setSessionTitle(newTitle);
      setExportToast(`✨ 已重写标题：${newTitle}`);
      setTimeout(() => setExportToast(""), 3000);
    } catch (e) {
      setExportToast(`重写失败：${e}`);
      setTimeout(() => setExportToast(""), 4000);
    }
  }}
  title="让 LLM 看会话末尾 10 条 turn 自动取个 ≤ 10 字标题..."
>
  ✨ LLM 重写标题
</button>
```

**关键设计**：

- **toast 即时显"进行中"**：LLM 调用约 1-3s 延迟 + 可能费用，让用户立即知道"我点了什么 / 正在做什么"。
- **成功后刷 sessionList**：让新 title 立即出现在 tab bar / 下拉里。
- **当前 session 命中时 setSessionTitle**：让 PanelChat 标题区也同步（saveCurrentSession 内部用此值生成 auto-title，下次再 save 不会被覆盖）。
- **错误 toast**：原样透传 backend error（含 status + body 前 200 字），便于 debug LLM 返 4xx / 5xx。

## 不做

- **不批量重写**：一键给所有 session 重写要按 N 次 LLM，费用爆炸。session-by-session 让用户对成本有感。
- **不动 saveCurrentSession 自动 title 逻辑**：新建 session 时仍走"首条 user 消息前 20 字"的默认路径 —— 那个对刚开始的 session 已足够好，LLM 重写是聊了几轮之后的可选 polish。
- **不 cache LLM 输出**：每次按按钮重新请求；用户可能不满意第一次结果想再 roll 一遍，cache 会挡住。
- **不动 TG / 桌面 chat**：本入口只在 PanelChat 的 session tab 右键菜单。
- **不写测试**：纯 IO（reqwest），既无可单测的纯算法（清洗 trim 是 string ops 简单显然），也无 vitest 基础设施。

## 验证

- `cargo check` ✓ 0 error
- `cargo test --lib` ✓ **988 / 988 通过**（无新测试也无回归）
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.19s
- 改动 ~130 行（backend command 100 + lib.rs 1 + 前端按钮 30）；既有 session ctx menu / save_session / load_session 路径全部不动。

## TODO 状态

empty —— 下次启动 TODO 流程会进入 auto-propose 分支提新需求。

## 后续

- 测试纯字符串清洗函数：把"trim 引号 + 换行替空格 + cap 30 char"抽成 pure helper + 加 unit test。
- 给 prompt 加宠物语气（与 SOUL.md 风格一致）让标题略个性化而非完全中立。
- 一键预览：弹 modal 显 LLM 候选 + 让用户选 / 编辑后再 commit。
- TG bot 也能调（用 TG ctx menu / button）；暂时 desktop-only。
