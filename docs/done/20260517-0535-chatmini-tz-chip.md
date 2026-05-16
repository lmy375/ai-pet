# ChatMini 顶部「🌐 当前时区」mini chip（iter #254）

## Background

owner 跨时区出差 / 远程协作 / 在 task description 里写"明天 14:00"时常会想确
认"本机当前时区是什么"。Tauri 桌面 pet 没有系统状态栏（mac status bar 在屏幕
顶部不属于 app），owner 要看时区要么打开 Finder/Calendar 要么记忆。

本迭代在 ChatMini 顶部 ⛶ / 📋 按钮之左加一个 mini chip 显当前 tz 缩写
（`🌐+8`），hover 显完整 IANA 名（`Asia/Shanghai (UTC+08:00)`），click 复制
IANA 名到剪贴板 — 方便在 task / chat 里写绝对时区限定（"明天 14:00
（Asia/Shanghai）"）。

## Changes

仅 `src/components/ChatMini.tsx`：

- 新增 `tzCopyOk: boolean` state — 1.5s ✓ 反馈
- 在 `onOpenPanel` 块之后插入新 chip：
  - 计算：`Intl.DateTimeFormat().resolvedOptions().timeZone` 拿 IANA 名；
    `-new Date().getTimezoneOffset()` 得 UTC offset 分钟数 → 拼 short
    （`+8` / `+5:30`）与 full（`UTC+08:00`）格式
  - 显示：`🌐${offsetShort}`；click → navigator.clipboard.writeText(IANA 名)
    → 1.5s `✓` 反馈
  - 位置：right: onOpenPanel ? "76px" : "48px"（在 📋 之左 28px）；与 ⛶
    / 📋 同 height 20px / `pet-mini-maxbtn` 风格保持视觉一致
  - tooltip：`本机当前时区：${tzName}（${offsetFull}）· 点击复制 IANA 名`

## Key design decisions

- **chip 文字只显 offset short**（`🌐+8`）：pet 窗 ~300px 窄，节省横向空间；
  full IANA 名走 tooltip。短 offset 对常见时区（+8 / -5 / +0）足够直观；半小
  时偏移（India +5:30 / Newfoundland -3:30）也能正确展示。
- **复制 IANA 名而不是 offset**：IANA 名是夏令时 / 历史变化感知的稳定标识，
  owner 写到 task 描述里更可靠（`Asia/Shanghai` vs `UTC+8`：前者不会因为 DST
  rule 变化而二义性，且 LLM / 后端 chrono crate 都能解析）。
- **不动态刷新**：tz 信息每次 render 重新计算（`Intl.DateTimeFormat()` 是
  cheap call），但 owner 主动改 timezone 后需要重启 / 关闭 ChatMini 才生效
  — 实际场景里 tz 在 session 内基本不变（用户改 macOS tz 也是少见操作），
  不挂 setInterval 省 re-render。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.23s)
