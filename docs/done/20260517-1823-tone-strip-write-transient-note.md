# PanelToneStrip「✍️ 写 transient_note」按钮 + popover（iter #364）

## Background

iter #363 给 TG bot 加了 `/transient <text> [minutes]` 命令；本 iter
是其桌面 UI 对偶：PanelToneStrip 此前只**显示**当前 transient_note
chip（📝 ...），但没有写入入口。owner 想在桌面"在开会，半小时别打
扰我"时需通过 TG 或 Rust internal — 这个 UX gap 现在补上。

## Changes

### `src/components/panel/PanelToneStrip.tsx`

#### 1. imports + useState

新增 useEffect / useRef / useState / invoke（之前是纯 props 组件，
无任何 hook）：
- `editorOpen`：popover 开关
- `draftText`：textarea 当前 buffer（打开时预填既有 transient_note）
- `pendingMinutes`：选中的时长 preset
- `submitting`：invoke in-flight 防双触
- `errorMsg`：保存失败短反馈

#### 2. TRANSIENT_PRESETS 常量

```ts
[{ minutes: 15, label: "15m" }, { minutes: 30, label: "30m" },
 { minutes: 60, label: "1h" }, { minutes: 120, label: "2h" }]
```

桌面 4 档常用值；想精细化任意分钟数走 TG /transient（cap 7d）。

#### 3. 入口 `<button>` ✍️ 写

放在 strip 最前（first chip 位）— always-visible 入口，无论当前有
没有 transient_note。tone.transient_note 非空时 title 提示"编辑 /
替换当前"，空时提示新建。editorOpen 时按钮变 cyan 实底视觉表明
"popover 当前展开"。

#### 4. popover

`position: absolute` 锚在 strip 容器（已加 `position: relative`），
top: calc(100% + 4px)。内容：
- 标题行 "✍️ 写 transient_note（in-memory · 不存盘）" — 让 owner
  心智明确这条不入永久 memory
- textarea（3 行可垂直拉伸）+ placeholder 示例
- 时长 preset chips（4 档），active 态 cyan 实底
- 错误行（保存失败时显）
- footer：⌘Enter 保存 / Esc 取消 提示 + 三按钮（清除[条件] / 取消 /
  保存）

#### 5. 行为细节

- 打开 popover 时 useEffect 预填 draftText = tone.transient_note ??
  ""，pendingMinutes 解析 remaining seconds 到最近 preset（让 owner
  "改主意延长 2h" 一键覆盖）
- ⌘Enter 提交（与 detail.md 编辑器同手势）；Esc 关闭
- 空 trimmed text 时"保存"按钮 disabled — 想清空走专属"🗑 清除"
- 「🗑 清除」仅在 tone.transient_note 非空时显（无 note 可清 ⇒ 不
  渲 dead button）
- submit 内部 invoke `set_transient_note { text, minutes }`，复用
  iter #363 同 Tauri command
- 不显式 refresh tone：PanelDebug 已 1s polling get_debug_snapshot，
  写完后 chip 在 ≤1s 内自动同步

## Key design decisions

- **state 在组件内部而非提父级**：popover 生命周期短（写完 / Esc
  关），不需要跨组件同步；提父级会让 PanelToneStrip props 膨胀。
- **入口位置 = strip 第一 chip**：高 discoverability + 紧贴既有
  📝 chip 让"读 / 写"行为成组。考虑过"仅在 📝 chip 旁条件渲染"
  但那让"无 note 时也想新建"路径绕。
- **复用 set_transient_note Tauri command 而非加新 wrapper**：iter
  #363 的 backend 路径已通用（text/minutes signature），UI 直接调
  即可 — 两入口（TG + UI）走同一后端是干净 architecture。
- **预填既有 note 而非空白起步**：常见 workflow 是"延长 / 修改"
  当前 note 而非每次从头写。空白起步会让 owner 重打字。
- **空 text submit 禁用而非允许 "= clear"**：empty submit 走 backend
  会被解释成 clear（与 TG /transient minutes=0 等价）；但 UI 单独
  提供「🗑 清除」按钮让"清除"动作显式 — 避免 owner 选 60m 然后清
  空 textarea 然后按"保存"时的语义混淆（这里清空文本是 typo 还是
  真要清？走专门按钮消除歧义）。
- **不持久化 pendingMinutes 偏好**：每次打开默认 30m 或解析既有
  remaining — 不挂 localStorage。owner 想固定偏好可走 TG /transient
  <text> <minutes>。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.23s)
- 后端无改动 — iter #363 已暴露 set_transient_note Tauri command
