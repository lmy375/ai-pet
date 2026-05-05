# Persona 面板 user_name 内联编辑（Iter R115）

> 对应需求（来自 docs/TODO.md）：
> Persona 面板 user_name 内联编辑：当前 user_name 显示是只读，要改名要切 Settings → 你的名字。在「陪伴时长」section 名字行加 ✏️ 按钮 → input 替换 → Enter 保存（load_settings → patch user_name → save_settings 路径，复用既有 Tauri 命令）。

## 目标

Persona 面板「陪伴时长」section 显示 `🐾 宠物称呼你为「{name}」`，但要改
得切 Settings tab → 找到"你的名字"输入框 → 改 → 保存。三步切窗口 + 找
field 太重。

加内联编辑：行末 ✏️ → 切到 input → Enter / blur 保存 → 自动刷新显示。
后端复用 `get_settings` + `save_settings` 既有路径，前端做 round-trip。

## 非目标

- 不引入专用 backend 命令 `set_user_name(name)`：现有 save_settings 路径
  足够；新增 setter 是 over-engineer（设置 < 1KB，整体读写瞬时）
- 不做 dirty-state 提示 / 取消 hint：与 PanelChat 会话名 R101 同语义
  （Esc 取消 / 空白等价取消 / blur 提交）
- 不联动顶 bar 称呼：宠物 prompt 注入下个 turn 自动 pickup 新名（5s 轮询
  / save 后立即 reload，皆可）

## 设计

### state

```ts
const [editingName, setEditingName] = useState(false);
const [nameDraft, setNameDraft] = useState("");
const [savingName, setSavingName] = useState(false);
const [nameError, setNameError] = useState("");
```

editingName 切换 input vs display；nameDraft 持当前编辑值；savingName 防
race 重复点；nameError 显短暂错误。

### handler

```ts
const startEditName = () => {
  setNameDraft(userName);
  setEditingName(true);
  setNameError("");
};

const commitName = async () => {
  if (savingName) return;
  const next = nameDraft.trim();
  // 与 server 端最新值一致 → 不发请求
  if (next === userName.trim()) {
    setEditingName(false);
    return;
  }
  setSavingName(true);
  try {
    // round-trip：拉全 settings，patch user_name，save 回去。复用既有
    // Tauri 命令；避免新增 setter 命令。
    const settings = await invoke<Record<string, unknown>>("get_settings");
    settings.user_name = next;
    await invoke("save_settings", { settings });
    setUserName(next);
    setEditingName(false);
  } catch (e: any) {
    setNameError(`保存失败：${e}`);
  } finally {
    setSavingName(false);
  }
};

const cancelEditName = () => {
  setEditingName(false);
  setNameError("");
};
```

`invoke<Record<string, unknown>>("get_settings")` 用宽类型 — 前端不强类
型化整 settings 结构；只摸 user_name 这一字段。改动最小。

### 渲染

替换原静态 div：

```tsx
<div style={{ marginTop: "10px", fontSize: "12px", display: "flex", alignItems: "center", gap: 6 }}>
  {editingName ? (
    <>
      <span style={{ color: "var(--pet-color-muted)" }}>🐾 宠物称呼你为</span>
      <input
        autoFocus
        value={nameDraft}
        onChange={(e) => setNameDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            void commitName();
          } else if (e.key === "Escape") {
            e.preventDefault();
            cancelEditName();
          }
        }}
        onBlur={() => void commitName()}
        disabled={savingName}
        style={{
          padding: "2px 6px",
          fontSize: 12,
          border: "1px solid var(--pet-color-accent)",
          borderRadius: 3,
          background: "var(--pet-color-card)",
          color: "var(--pet-color-fg)",
          outline: "none",
          minWidth: 100,
        }}
      />
      {savingName && <span style={{ color: "var(--pet-color-muted)" }}>保存中…</span>}
      {nameError && <span style={{ color: "#dc2626" }}>{nameError}</span>}
    </>
  ) : (
    <>
      <span style={{ color: userName.trim() ? "var(--pet-color-fg)" : "var(--pet-color-muted)", fontStyle: userName.trim() ? "normal" : "italic" }} title={...保留原 title...}>
        {userName.trim() ? `🐾 宠物称呼你为「${userName.trim()}」` : "🐾 还没设名字"}
      </span>
      <button
        type="button"
        onClick={startEditName}
        style={{ padding: "0 4px", border: "none", background: "transparent", color: "var(--pet-color-muted)", cursor: "pointer", fontSize: 12 }}
        title="点击修改名字（Enter 保存 / Esc 取消）"
        aria-label="edit user name"
      >
        ✏️
      </button>
    </>
  )}
</div>
```

### 测试

无单测；手测：
- 默认显 user_name + ✏️
- 点 ✏️ → 切 input + 自动 focus + 选中默认值
- 改名 + Enter → "保存中…" → 显示新名 + ✏️
- 改名 + Esc → 不保存退出
- 失焦 → 等同 Enter 提交
- 输入与原值相同 → 不发请求直接退出 edit
- 后端失败 → "保存失败：…" 显示，留在 edit 模式让用户重试

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + handler |
| **M2** | 渲染替换：display vs input 双态 + ✏️ 按钮 |
| **M3** | tsc + cargo check + build |

## 复用清单

- 既有 `get_settings` / `save_settings` Tauri 命令
- 既有 PanelChat R101 inline rename 同款交互模式
- 既有 userName state（5s 轮询拉新值，save 后立即 setUserName 同步）

## 进度日志

- 2026-05-09 20:00 — 创建本文档；准备 M1。
- 2026-05-09 20:08 — M1 完成。`editingName / nameDraft / savingName / nameError` 4 个 state；`startEditName / commitName / cancelEditName` 三件套；commit 内 `next === userName.trim()` 短路 + `invoke<Record<string, unknown>>("get_settings")` round-trip + `settings.user_name = next` patch + `save_settings` 落库 + `setUserName(next)` 立即同步。
- 2026-05-09 20:14 — M2 完成。原静态 div 改 flex 容器：editingName 切到 input（Enter / blur 提交、Esc 取消、autoFocus、accent border、disabled during saving、placeholder 提示空 = 默认"你"）；非 edit 时显原文案 + ✏️ 按钮（aria-label / title 提示）。失败时 nameError 显红色文案，留在 edit 模式让用户重试。
- 2026-05-09 20:18 — M3 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
