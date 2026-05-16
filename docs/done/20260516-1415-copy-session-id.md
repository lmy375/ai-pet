# ChatPanel session tab 右键加「🔑 复制会话 ID」入口

## 背景

TODO 上 auto-proposed 一条："ChatPanel session tab 右键加『📋 复制会话 ID』入口：debug / 上报 issue 时方便包含 session id。"

session id 是 uuid 形态字符串（~36 字符），定位到 `~/.config/pet/sessions/<id>.yaml` 具体文件。debug 时用户描述"我会话标题是 xxx 出问题了"远不如"会话 id xxx 出问题了"精确 —— title 可能重名 / 已被 LLM 重写、id 永远唯一。

session tab 右键菜单已聚合 pin / 改名 / 复制标题 / LLM 重写 / 删除 等动作。补一行复制 ID 是最直接的入口。

## 改动

### `src/components/panel/PanelChat.tsx`

在「📋 复制标题」之后、「✨ LLM 重写标题」之前插入：

```tsx
<button
  type="button"
  style={itemBtn}
  onMouseOver={hoverIn}
  onMouseOut={hoverOut}
  onClick={async () => {
    setSessionTabCtxMenu(null);
    try {
      await navigator.clipboard.writeText(m.id);
      setExportToast(`已复制会话 ID：${m.id.slice(0, 8)}…`);
      setTimeout(() => setExportToast(""), 2500);
    } catch (e) {
      setExportToast(`复制失败：${e}`);
      setTimeout(() => setExportToast(""), 3000);
    }
  }}
  title={`把会话 ID 复制到剪贴板（用于 debug / 上报 issue 时定位具体会话文件）。完整 ID：${m.id}`}
>
  🔑 复制会话 ID
</button>
```

## 关键设计

- **🔑 emoji 而非 📋**：复制标题用 📋，复制 ID 区分 emoji 让 owner 在快速 hover 菜单时不会按错。🔑 钥匙意指"唯一标识"，与 ID 语义对齐。
- **toast 显前 8 位 + 省略号**：完整 uuid 在 toast 横幅太长（36 字符）；前 8 位足以让 owner 知道"我复制对了哪条"。完整 ID 已在剪贴板可粘贴验证。
- **title attr 显完整 ID**：hover 菜单 item 即可看完整 36 字符串，不必复制再粘。debug 场景下 owner 可能想"先扫一眼是不是这个 id" 再决定要不要复制。
- **复用 exportToast 通道**：与既有复制标题 / 导出归档 / LLM 重写等动作同 toast channel，UI 一致。
- **位置在复制标题与 LLM 重写之间**：两个"复制"动作自然挨着（标题 / ID），形成"复制集群"。
- **不动其它菜单 item**：iter 是单点补 entry。

## 不做

- **不写测试**：纯 onClick → `navigator.clipboard.writeText`，逻辑 ~12 行。既有复制标题路径无单测。视觉验证（右键 → 🔑 复制 → toast 显 → 粘贴验证 ID）足够。
- **不接桌面 ChatMini 同款入口**：mini chat 没"session tab 列表"概念（mini chat 视野只有当前 active session），不需要"复制 N 个 session 中某个的 ID"。当前 active session ID 也很少用户会从 mini chat 想拷出来。
- **不加"复制 session 文件路径"分离入口**：完整路径 = `~/.config/pet/sessions/<id>.yaml`，用户拿到 ID 后自己拼即可。多一项 menu item 反增噪。
- **不加 modifier key 复制变体（⌘+click 复制完整 url 等）**：本动作太低频，不值得多变体复杂化。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.17s
- 改动 ~30 行（button JSX + 注释 + tooltip）；既有 session tab ctx menu 路径（pin / 改名 / 复制标题 / LLM 重写 / 分隔 / 删除）完全不动。

## TODO 状态

6 条 auto-proposed 已完成 3 条，余 3 条留池：
- PanelMemory 类目内 items > 20 时按 updated_at 月份分组
- detail.md preview「📑 大纲」浮窗
- 任务 detail.md 中文配对引号 / 括号

## 后续

- 复制 ID 之后浮"📂 也复制路径吗？" 二次菜单 —— 当前路径用户自己拼。等真有诉求再做。
- 把 session ID 直接写到导出 markdown 的 frontmatter 里：让 owner 导出 + 复制时一并带身份信息。当前导出 markdown 是 H1 标题 + 时间，不含 id。
