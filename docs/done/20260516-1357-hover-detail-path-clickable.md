# 任务 hover preview「📄 detail_path」chip 可点击 reveal

## 背景

TODO 上 auto-proposed 一条："任务 hover preview '📄 detail_path' chip 可点击 → 直跳 Finder 显示文件（复用既有 memory_reveal_detail_in_finder 命令）。"

任务行 hover 500ms 后浮 preview 含元数据 chips / detail.md 摘要 + 顶部 `📄 path` 行显示 detail.md 路径 —— 当前是纯文本，只能看。`memory_reveal_detail_in_finder` Tauri 命令早已存在（detail.md 编辑器顶部 📂 按钮的同后端），让 hover preview 的路径 chip 一键调即可省"展开任务 → 切到 detail tab → 点 📂"三步。

## 改动

### `src/components/panel/PanelTasks.tsx`

hover preview 容器自带 `pointerEvents: "none"`（line 5143）—— 让 hover area 不抢 row 主 onClick。CSS 继承让所有子元素也 non-clickable，除非显式 override。

把 `📄 {t.detail_path}` 包成 button：

```tsx
<button
  type="button"
  onClick={async (e) => {
    e.stopPropagation();
    if (!t.detail_path) return;
    try {
      await invoke<void>("memory_reveal_detail_in_finder", {
        detailPath: t.detail_path,
      });
    } catch (err) {
      setActionErr(`在 Finder 打开失败：${err}（detail.md 可能尚未保存到磁盘）`);
      window.setTimeout(() => setActionErr(""), 3500);
    }
  }}
  onMouseDown={(e) => e.stopPropagation()}
  title={`在系统文件管理器里显示 detail.md（路径：memories/${t.detail_path}）...`}
  style={{
    pointerEvents: "auto",  // override 父级 "none"
    background: "transparent",
    border: "none",
    color: "var(--pet-color-muted)",
    fontFamily: "inherit",
    fontSize: "inherit",
    padding: 0,
    cursor: "pointer",
    textAlign: "left",
    textDecoration: "underline dotted",
    textUnderlineOffset: 2,
  }}
>
  📄 {t.detail_path}
</button>
```

## 关键设计

- **`pointerEvents: "auto"` override**：CSS `pointer-events` 是继承的。父级 hover preview `pointerEvents: "none"` 让整个气泡透明穿透 mouse 事件到下方 row（保 hover-and-leave 体验 + row 还能被 click 展开）。chip 显式 `auto` override 让 click 仅在这个具体元素上 reach 到。其它 preview 内容（meta chips / detail snippet / history）继续穿透 —— 这是 CSS 给的精确控制。
- **`onMouseDown` + onClick stopPropagation**：防 click 冒泡触发 row onClick（展开任务）—— 用户期望"点 chip 打开 Finder"，不应同时触发"展开任务详情"。
- **`textDecoration: underline dotted`**：visual hint 告诉用户 "这是可点击"。dotted underline 比 solid 更轻量，与 hover preview 的"安静展示"基调一致；hover 时 cursor: pointer 强化。
- **复用既有 setActionErr toast**：与 detail.md 编辑器 📂 按钮同 error 文案 / 同 3.5s 自清。让两个入口（hover preview vs 编辑器顶部）的失败反馈一致。
- **`!t.detail_path` early return**：理论上 hover preview 仅在 t.detail_path 存在时才能进入这条分支，但显式 guard 防御 race / 老 session 迁移期。
- **不动其它 hover preview 内容**：meta chips / detail snippet / history / "暂停" / "等待" 各 chip 都继续 `pointerEvents: none`（继承父级）—— 它们是 informational，不需要 click 入口。仅 📄 path 是真正 actionable。

## 不做

- **不把整个 hover preview 改成 `pointerEvents: auto`**：那会让气泡抢 mouse 事件，破坏既有"鼠标离开 trigger 自动隐藏"行为。逐个 child override 才是 surgical。
- **不写测试**：纯 onClick + IPC + CSS override；既有 hover preview / memory_reveal_detail_in_finder 路径都视觉验证。点 chip → Finder 跳出高亮选中 → 视觉确认。
- **不接 right-click 菜单到这个 chip**：单击 reveal 已足够；多入口会拖慢决策。
- **不动详情编辑器顶部 📂 按钮**：那是 edit 模式的 reveal；本 iter 给 hover preview 的对偶入口，两者并存。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.14s
- 改动 ~50 行（button JSX 替换原 div + 注释）；既有 hover preview 容器 / memory_reveal_detail_in_finder 后端 / 编辑器 📂 按钮 / setActionErr 路径完全不动。

## TODO 状态

6 条 auto-proposed 已完成 1 条，余 5 条留池：
- PanelMemory 类目内 items > 20 时按 updated_at 月份分组
- ChatPanel session tab 右键加「📋 复制会话 ID」
- 桌面 mini chat ts label hover tooltip
- detail.md preview「📑 大纲」浮窗
- 任务 detail.md 中文配对引号 / 括号

## 后续

- hover preview 内容其它 actionable chip：如 due chip → 点击 deeplink 到「📅 今日」chip filter；priority chip → 点击改 priority 等。逐步把 hover preview 从纯信息变 actionable surface。
- chip 失败 toast 改用更显眼的"行内 hover preview banner"，而非 row 下方 setActionErr —— 当前 hover preview 还在显，banner 在底下用户看不到。可加 in-preview error 行。
