# PanelChat 当前会话内搜索（Iter R96）

> 对应需求（来自 docs/TODO.md）：
> PanelChat 当前会话内搜索：现 cross-session 搜索只跨会话；单会话内长对话定位特定消息只能滚 + 肉眼。加"只搜本会话"模式开关，复用 SearchResultRow + match offset 高亮路径。

## 目标

PanelChat 顶 bar 🔍 按钮触发 cross-session 搜索面板。当前实现只支持"全部
会话"扫描：单 session 长对话里要找之前讨论过的某个细节，得切到 search、
键入 keyword、再从 50 条结果（很多别的 session 命中）里挑出当前 session
的命中点 —— 多余环节。

加 scope 切换（全部 / 当前会话）：当前会话模式只搜 `sessionId` 那条 session
的 items，结果列表干净。

## 非目标

- 不在主消息区做"inline search"高亮（jump-to-next）—— 搜索 panel 模式已
  足够；把 search 嵌入 message scroll 增加交互层次
- 不持久化 scope 选择 —— session 内即可，关 search 面板自动重置 default
  "全部"

## 设计

### 后端 — `search_sessions` 加 session_id 过滤

`src-tauri/src/commands/session.rs`：

```diff
 #[tauri::command]
-pub fn search_sessions(keyword: String, limit: Option<usize>) -> Vec<SearchHit> {
+pub fn search_sessions(
+    keyword: String,
+    limit: Option<usize>,
+    session_id: Option<String>,
+) -> Vec<SearchHit> {
     // ...
     'outer: for meta in &index.sessions {
+        if let Some(ref sid) = session_id {
+            if &meta.id != sid {
+                continue;
+            }
+        }
         // load_session + 命中逻辑不变
     }
 }
```

后端做过滤而不是前端 post-filter：当前 limit=50 可能被其它 session 命中
吃满，前端再 filter 就丢了当前 session 的命中。后端跳过非目标 session
让 limit 全留给当前 session。

### 前端 — scope 状态 + 切换 UI

```ts
const [searchScope, setSearchScope] = useState<"all" | "current">("all");
```

useEffect 拉数据时按 scope 加 sessionId：

```ts
const args: { keyword: string; sessionId?: string } = { keyword: q };
if (searchScope === "current") args.sessionId = sessionId;
const hits = await invoke<SearchHit[]>("search_sessions", args);
```

依赖加 `searchScope`，切换时即时重 fetch。

UI: 在搜索 input 行 ✕ 之前加双按钮 pill 切换：

```tsx
<div style={{ display: "flex", gap: 0, alignItems: "center", padding: 1, background: "var(--pet-color-bg)", borderRadius: 4 }}>
  {(["all", "current"] as const).map((scope) => {
    const active = searchScope === scope;
    return (
      <button
        key={scope}
        type="button"
        onClick={() => setSearchScope(scope)}
        style={{
          padding: "2px 8px",
          fontSize: 11,
          border: "none",
          borderRadius: 3,
          background: active ? "var(--pet-color-card)" : "transparent",
          color: active ? "var(--pet-color-fg)" : "var(--pet-color-muted)",
          cursor: active ? "default" : "pointer",
          fontWeight: active ? 600 : 400,
        }}
        title={
          scope === "all"
            ? "搜全部历史会话"
            : "只搜当前打开的会话（更精准，结果不被其它 session 抢限额）"
        }
      >
        {scope === "all" ? "全部" : "本会话"}
      </button>
    );
  })}
</div>
```

放在搜索 input 与 ✕ 之间，与现有 inline 控件流连贯。

切换时清掉旧 results 让 useEffect 重 fetch：依赖里有 searchScope，自动会
触发 — 不需要额外手动清空。

退出 search 模式时（`setSearchMode(false)` 路径）顺带 reset scope 到 "all"：
保持"下次进 search 默认全局"的直觉。

### 测试

无单测；手测：
- 默认 "全部"：与原行为一致
- 切到 "本会话"：结果只显当前 session 的命中
- 当前 session 没有命中 → 显"没有匹配的消息"
- 切回 "全部"：结果重 fetch 回归
- 关 search 面板再开 → scope reset 到 "全部"

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 后端 search_sessions 加 session_id 参数 |
| **M2** | 前端 state + useEffect dep + UI 按钮 + 退出 reset |
| **M3** | tsc + cargo check + build |

## 复用清单

- 既有 SearchHit / SearchResultRow / handleSelectSearchHit 路径
- 既有跨会话搜索 useEffect

## 进度日志

- 2026-05-09 01:00 — 创建本文档；准备 M1。
- 2026-05-09 01:08 — M1 完成。后端 `search_sessions` 加 `session_id: Option<String>` 第三参；遍历 `index.sessions` 时 if Some 跳过 id 不匹配的会话；保留 `cap` / `outer break` 机制，让限额完全用在目标会话上。
- 2026-05-09 01:14 — M2 完成。前端 `searchScope: "all" | "current"` state；useEffect 依赖加 searchScope + sessionId，scope=current 时把 sessionId 拼到 invoke args；🔍 按钮关闭时复位 scope；input 旁双键 pill toggle（active 走 card/fg、inactive 走 bg/muted，与系统 toggle pill 配色一致）；ESC handler / handleSelectSearchHit 也复位 scope；placeholder + empty-state 文案随 scope 切换。title 中文双引号撞 JSX 引号闯 TS 报错（行 679 误关闭 attribute）→ 改用『』全角引号修复。
- 2026-05-09 01:18 — M3 完成。`pnpm tsc --noEmit` 0 错误；`cargo check` 通过 (2.51s)；`pnpm build` 通过 (499 modules, 962ms)。归档至 done。
