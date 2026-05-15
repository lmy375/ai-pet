# 桌面 pet 右键菜单加「📂 打开数据目录」入口

## 背景

TODO 上 auto-proposed 一条："桌面 pet 右键菜单加「📂 打开数据目录」入口：让 owner 不开 Panel 就能直奔 `~/.config/pet/` 浏览（与既有 open_pet_data_dir 命令复用）。"

桌面 pet 右键菜单刚 ship 了一轮含 6 个动作（打开面板 / 切主题 / mute 30 / mute 60 / 解除 mute / 重启窗口）。但 owner 想直奔数据目录（看 SOUL.md / memories / sessions / 把 config.yaml 备份 / git status 等）仍需先打开 Panel → 切到设置页 → 点「在 Finder 中打开」。三步太绕。

`open_pet_data_dir` 后端命令早已存在（PanelSettings 那个按钮的同后端）。补一行菜单 item 让 desktop 右键也成入口。

## 改动

### `src/App.tsx`

在 `📋 打开面板` 之后、第一个 separator 之前插入：

```tsx
<button
  type="button"
  style={itemStyle}
  onMouseOver={itemHoverIn}
  onMouseOut={itemHoverOut}
  onClick={async () => {
    setPetCtxMenu(null);
    try {
      await invoke("open_pet_data_dir");
    } catch (e) {
      console.error("open_pet_data_dir failed:", e);
    }
  }}
  title="在系统文件管理器里打开宠物数据目录（~/.config/pet/）—— 含 config.yaml / SOUL.md / memories/ / sessions/ 等。"
>
  📂 打开数据目录
</button>
```

菜单视口 clamp 高度从 H=240 调到 270 适配多一行：

```ts
// H 经验值 ~ 7 个 button (button ≈ 26px) + 3 个 separator (≈ 9px) +
// 8px padding ≈ 217；加点余量到 270 给字体放大 / 不同主题边距浮动。
const H = 270;
```

## 关键设计

- **位置紧跟 📋 打开面板**：两者都是"open something"动作，自然形成"打开"集群。同段无 separator 隔（与既有"😴 mute 30 / 60 / 解除 mute" 同集群无 sep 思路一致）。
- **后端命令复用**：`open_pet_data_dir` 与 PanelSettings 「在 Finder 中打开」按钮同后端 —— 单一真相源 / 跨平台分支（macOS `open` / Windows `explorer` / 其它 `xdg-open`）已稳。本 iter 仅多个前端入口。
- **错误 console 静默**：Finder / Explorer 打开是 fire-and-forget；用户能看到（或不能看到）系统文件管理器跳出。toast 反而冗余。失败极少（仅 fs 权限 / 数据目录刚被强删等边界）。
- **H clamp 270**：6 个 button + 3 sep + 4 padding ≈ 217；270 留 ~50px 余量 cover dark / light 边距 + 字体可能略大于经验值。viewport 大时 clamp 不生效（只在底边贴齐时回退）。

## 不做

- **不挂快捷键到菜单 item**：菜单内部不放 key hint（与现有 6 项保持一致风格）。本动作不高频到值得键绑。
- **不写测试**：纯 onClick → IPC，逻辑 ~10 行；既有右键菜单 / PanelSettings 同 callsite 都无单测。视觉验证（右键 → 点📂 → Finder 跳出）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.14s
- 改动 ~30 行（按钮 JSX + H 调整 + comment）；既有 6 个右键菜单按钮、open_pet_data_dir 后端、PanelSettings 同 callsite 完全不动。

## TODO 状态

6 条候选 auto-proposed 已完成 3 条，余 3 条留池：
- 桌面 pet Esc 收起窗口
- detail.md LinkCard 特殊域名 emoji
- 任务行 hover preview 段也走 LinkCard

## 后续

- 菜单加「📤 设置」直跳 PanelSettings tab（与 ⌘1 同源），让 owner 不必先打开 panel 再点 tab。
- 菜单加 footer 显当前数据目录绝对路径 chip（与 PanelSettings 风格统一）—— hover preview，方便快速对路径。
