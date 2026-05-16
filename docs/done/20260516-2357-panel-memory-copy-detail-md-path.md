# PanelMemory item 行加 📋📄 复制 detail.md 绝对路径按钮

## 背景

iter #191 给 PanelTasks 行右键菜单加了 「🔗 复制 detail.md 绝对路径」 + 新 `memory_detail_abs_path` Tauri 命令。owner 粘 path 到 IDE / Finder / shell 直接打开本地文件。

PanelMemory item 行有同款 detail_path 但当前只有 🚀 用外部 editor 打开 + 🔗 复制 ref token —— 缺 "复制 path" 按钮。注意 hover preview tooltip 是 pointerEvents:none，没法把 button 放里头；放 item 行 inline 按钮区。

加按钮，与 PanelTasks 同 backend 命令同 UX。

## 改动

### `src/components/panel/PanelMemory.tsx`

item action button 行（紧贴 🚀 开外部按钮之后、🔗 复制 ref 按钮之前）插：

```tsx
<button
  style={s.btn}
  onClick={async () => {
    try {
      const abs = await invoke<string>("memory_detail_abs_path", {
        detailPath: item.detail_path,
      });
      await navigator.clipboard.writeText(abs);
      setMessage(`已复制 detail.md 绝对路径`);
    } catch (e) {
      setMessage(`复制 path 失败：${e}`);
    }
    setTimeout(() => setMessage(""), 2500);
  }}
  title={`把 ${item.detail_path} 的绝对路径（含 ~/.config/pet/memories/... 前缀）复制到剪贴板。粘到 VSCode ⌘P / IntelliJ ⇧⌘O / Finder ⇧⌘G / shell open 都能直接打开本地文件。`}
  aria-label="copy detail.md absolute path"
>
  📋📄
</button>
```

## 关键设计

- **复用 memory_detail_abs_path Tauri 命令**：iter #191 已经做了 canonicalize + 路径安全检查 + 文件不存在时直接拼接 fallback。无需新后端代码。
- **emoji 📋📄 双 emoji**：与既有 🚀 / 🔗 / 📐 等单 emoji 区分；📋 (复制) + 📄 (文档) 双 emoji 表达"复制文档路径"语义。
- **题中 tooltip 列具体 IDE 用法**：VSCode ⌘P / IntelliJ ⇧⌘O / Finder ⇧⌘G / shell open —— 教学性 tooltip 让 owner 学会 path 用法。
- **inline 在 item 行按钮区**：与既有 🚀 / 🔗 等同区域 cluster，符合 owner 心智"操作本 item 的按钮在这一排"。

## 不做

- **不放进 hover preview tooltip**：pointerEvents:none 不能 click。
- **不在 PanelMemory 全 cat 隐藏 — 仅 detail_path 非空显**：item.detail_path 总是有值（ai_insights / butler_tasks / user_profile 全 cat items 都有 detail_path）；不需要额外 gate。
- **不写测试**：与 iter #191 PanelTasks 行右键同 IPC 路径已经验证。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.23s
- 改动 ~25 行（button + onClick + tooltip + 注释）。既有 🚀 / 🔗 / 📐 / 删除 / 双击 rename / 等 item action 路径完全不动。

## TODO 状态

剩 4 条留池：
- PanelTasks 行 💤 snooze chip click 弹 snooze presets popup
- detail.md 编辑器 toolbar "📋 复制选中段 → 新 task"
- PanelTasks "+ 新建" chip 显未读 / 错误任务计数
- pet 区右键加「📡 ping LLM 测延迟」

## 后续

- ⌥+click "📋📄" 复制相对 path（仅 `butler_tasks/x.md` 短串）让 owner 想用作 `[task: ...]` ref token 时 short form。
- 在 PanelTasks / PanelChat 等其它 panel 也放对应 path 复制按钮 —— "复制 detail.md path" 通用化。
