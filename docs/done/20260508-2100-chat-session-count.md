# PanelChat 会话列表显示消息条数（Iter R93）

> 对应需求（来自 docs/TODO.md）：
> PanelChat 会话列表标题显示消息条数：session 行的标题旁加 "(N 条)"，跨会话切换时一眼知道每个 session 对话深度，避免点开才发现是空会话。

## 目标

PanelChat 顶 bar 点会话名展开 dropdown 列出全部历史会话。当前每行只显标题
+ updated_at 日期 + 删除按钮，没有"会话有多少条对话"信息。结果是用户切换会
话时常点到空会话（刚创建未对话）/ 短会话，浪费一次"切换 → 滚动 → 退回" 的
时间。

加 "(N 条)" 在标题旁，对话深度一眼可见。

## 非目标

- 不算 system message —— 用户不关心 prompt 系统消息
- 不区分 user / assistant / tool 条 —— 总条数足以表达深度
- 不动 search panel 的 SearchResultRow（那是另一个交互维度）

## 设计

### 后端 — SessionMeta 加 item_count

`src-tauri/src/commands/session.rs`：

```diff
 pub struct SessionMeta {
     pub id: String,
     pub title: String,
     pub created_at: String,
     pub updated_at: String,
+    #[serde(default)]
+    pub item_count: usize,
 }
```

`#[serde(default)]` 让旧 index.json（无 item_count 字段）反序列化到 0 而非
panic；下次 save_session 时会被填入实际值，迁移自动发生。

`save_session` 需要同时维护 item_count：

```diff
 if let Some(meta) = index.sessions.iter_mut().find(|m| m.id == session.id) {
     meta.title = session.title.clone();
     meta.updated_at = session.updated_at.clone();
+    meta.item_count = session.items.len();
 } else {
     index.sessions.push(SessionMeta {
         id: session.id.clone(),
         title: session.title.clone(),
         created_at: session.created_at.clone(),
         updated_at: session.updated_at.clone(),
+        item_count: session.items.len(),
     });
 }
```

注意：count 是 `session.items.len()`，不是 `session.messages.len()` ——
items 是用户可见的 chat row 集合（user / assistant / tool / error），而
messages 含 system role。后者对用户没语义。

### 前端 — interface + 渲染

```diff
 interface SessionMeta {
   id: string;
   title: string;
   created_at: string;
   updated_at: string;
+  item_count?: number;
 }
```

`?` 让 TS 接受老索引（旧前端代码下载新版后立即 fetch 老 index 期间）。

dropdown row 渲染：

```diff
 <div style={{ fontSize: "13px", color: "var(--pet-color-fg)", ... }}>
-  {s.title}
+  {s.title}
+  {typeof s.item_count === "number" && (
+    <span style={{ fontWeight: 400, color: "var(--pet-color-muted)", marginLeft: 6 }}>
+      ({s.item_count} 条)
+    </span>
+  )}
 </div>
```

`typeof === "number"` 守卫覆盖：未迁移老索引下读到 undefined（非显式 0）→
不显示括号，避免误导用户"以为是 0 条"。一旦该会话被保存一次，item_count
就自动填入 → 显示真实数。

### 测试

无单测；手测：
- 启动后立刻打开 dropdown：旧会话可能仍未渲染计数（未迁移）；点开聊一句、关闭，下次打开 dropdown 该 session 显 "(N 条)"
- 新建会话 → 立即出现 "(0 条)"（save_session 在 create_session 末尾跑过）
- 与 cross-session search panel 不互相干扰

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 后端 SessionMeta + save_session |
| **M2** | 前端 interface + 渲染 |
| **M3** | tsc + cargo check + build |

## 复用清单

- 既有 `Session.items` 数组（commands/session.rs Session struct）
- 既有 dropdown 渲染流（PanelChat line 727+）

## 进度日志

- 2026-05-08 21:00 — 创建本文档；准备 M1。
- 2026-05-08 21:08 — M1 完成。后端 `commands/session.rs` SessionMeta 加 `item_count: usize`（带 `#[serde(default)]` 让旧 index.json 反序列化到 0）；save_session 同时维护 `meta.item_count = session.items.len()`，新会话 push SessionMeta 时也带上。Session struct 不动 —— 它已经有 items 字段，count 是其 len()。
- 2026-05-08 21:11 — M2 完成。前端 SessionMeta interface 加 `item_count?: number`（optional 守卫覆盖老 index 期间）；dropdown row 标题旁条件渲染 `({s.item_count} 条)` muted 灰字 + tooltip。
- 2026-05-08 21:15 — M3 完成。`pnpm tsc --noEmit` 0 错误；`cargo check` 通过 (10.25s)；`pnpm build` 通过 (499 modules, 973ms)。归档至 done。
