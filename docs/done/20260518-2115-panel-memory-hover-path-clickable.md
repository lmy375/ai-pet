# PanelMemory item hover preview 「📄 path」可点复制绝对路径（iter #501）

## Background

PanelMemory item hover preview popover 已含 `📄 <relative path>` 文本
行（line 6639）— hover 任意 item 500ms 浮的 detail.md 预览卡内一行
路径展示。但该行**仅显示**，不可点。

owner 想拿 detail.md **绝对路径** 粘到 VSCode `⌘P` / Finder `⇧⌘G` /
shell `open` 时只能：
1. 展开 item 进 expanded view
2. 找到 expanded 内的「📋📄 复制 detail.md 绝对路径」button
3. 点

3 步 — 走 hover preview 看 item 时不需要全展开就该能拿到 path。

本 iter 让 hover preview 内既有 `📄 <path>` 文本行**可点 → 复制绝对
路径**。复用既有 `memory_detail_abs_path` Tauri 命令（与 expanded view
的 📋📄 button 同后端），仅前端 click handler + cursor: pointer。

## Changes

### `src/components/panel/PanelMemory.tsx` line 6632

```tsx
// Before：纯展示 div
<div style={{ fontSize: 10, color: "muted", marginBottom: 4 }}>
  📄 {item.detail_path}
</div>

// After：可点 div
<div
  onClick={async (e) => {
    e.stopPropagation();
    try {
      const abs = await invoke<string>(
        "memory_detail_abs_path",
        { detailPath: item.detail_path },
      );
      await navigator.clipboard.writeText(abs);
      setMessage(`📄 已复制 detail.md 绝对路径`);
    } catch (err) {
      setMessage(`复制 path 失败：${err}`);
    }
    window.setTimeout(() => setMessage(""), 2500);
  }}
  title={`点击复制绝对路径...`}
  role="button"
  tabIndex={0}
  style={{
    fontSize: 10,
    color: "var(--pet-color-muted)",
    marginBottom: 4,
    cursor: "pointer",
    userSelect: "none",
  }}
>
  📄 {item.detail_path}
</div>
```

## Key design decisions

- **不引新 chip / 不改 layout**：复用既有路径文本行使可点 — 零视觉密
  度增量，hover preview 内本就显此文本，点它复制是直觉
- **复用 `memory_detail_abs_path` 后端**：与 expanded view 的 📋📄
  button 同 Tauri 命令；未来后端改路径解析逻辑两入口自动同步
- **显相对、复制绝对**：preview 卡空间紧凑，绝对路径太长（含 `~/.config
  /pet/memories/...` 30+ 字符）会撑爆；title attr 说明 "点击复制**绝
  对**路径"避免歧义
- **`stopPropagation`**：防 click 冒泡到外层 item row 触发其它 onClick
  路径（如 hover preview close）
- **`role="button"` + `tabIndex={0}`**：a11y — 让屏幕阅读器识别为可
  交互元素；键盘党 Tab 可达
- **`userSelect: "none"`**：防 click 误触发文本选中（与按钮语义不符）
- **不写 unit test**：纯 clipboard write + Tauri invoke 透传；逻辑
  trivial（既有 📋📄 button 同算法 production 验证）。GOAL.md
  "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.35s)
- 后端无改动 — 纯前端 onClick
- 手测：PanelMemory 任意 item hover 500ms → 浮 preview 卡 → 看到 `📄
  <relative path>` 行变 cursor: pointer → 点击 → 顶部 setMessage toast
  「📄 已复制 detail.md 绝对路径」→ 粘到 VSCode ⌘P / Finder ⇧⌘G /
  shell — 看到完整绝对路径含 `~/.config/pet/memories/...` 前缀

## Future iters

- 复制时 toggle key 决定相对 vs 绝对（如 ⌥+click）— 当前一律绝对，相
  对场景无明确需求
- ⌘+click 时打开 detail.md（外部编辑器）— 与既有 expanded view 🚀
  button 对偶；当前 hover preview 内入口已饱和，按需评估
