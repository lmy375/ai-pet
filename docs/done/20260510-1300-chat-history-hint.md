# PanelChat 历史模式视觉提示（Iter R132）

> 对应需求（来自 docs/TODO.md）：
> PanelChat 历史模式视觉提示：R129 加了多条历史召回但用户看不出当前浏览到第几条；historyCursor 非 null 时 textarea 上方显小字 "↑ 历史 (i+1) / N"，让位置即时可见，按 Esc 退出时消失。

## 目标

R129 加了 `messageHistory` ring buffer 让 ↑ / ↓ 穿越多条历史。但 textarea
没显示位置，用户不知道:
- 当前是历史中第几条
- 还能往前 ↑ 几次
- 还能往后 ↓ 几次

加 floating hint chip：historyCursor 非 null 时显 `↑ 历史 i+1 / N`，浮在
input bar 上方右侧；输入 / Esc / 提交退出后自动消失。

## 非目标

- 不显历史内容预览 —— textarea 已展示当前 cursor 内容，hint 只标位置
- 不引入"浏览历史的键盘快捷键 cheatsheet" 弹窗 —— hint 文案 "↑ 历史 N"
  含 ↑ 暗示快捷键存在
- 不持久化 hint visibility —— 完全派生自 historyCursor

## 设计

### 渲染

form 容器（line 1226-）已是 `position: relative`，slash 菜单用 bottom:
100% 锚定。新 hint 用 `top: -22` 锚定到 form 顶之外（浮在 input 行上方），
不与 slash 菜单冲突（slash 菜单在 form 顶之上更高位置 / form 内）。

```tsx
{historyCursor !== null && (
  <div
    style={{
      position: "absolute",
      top: -22,
      right: 16,
      fontSize: 10,
      background: "var(--pet-color-card)",
      border: "1px solid var(--pet-color-border)",
      borderRadius: 4,
      padding: "2px 8px",
      color: "var(--pet-color-muted)",
      pointerEvents: "none",
      whiteSpace: "nowrap",
      fontFamily: "'SF Mono', 'Menlo', monospace",
    }}
    title={`当前浏览历史第 ${historyCursor + 1} / ${messageHistory.length} 条；↑ 往更早，↓ 往更新，Esc 退出`}
  >
    ↑ 历史 {historyCursor + 1} / {messageHistory.length}
  </div>
)}
```

`pointerEvents: "none"` 让 hint 不挡按钮 / 不可点（纯指示）。
`top: -22` 让 hint 浮在 input bar 之上，不挤压 textarea 高度。

### 测试

无单测；手测：
- 默认 historyCursor = null → 不显
- 空 input + ↑ → cursor = length-1，hint 显 "↑ 历史 N / N"
- 再 ↑ → "↑ 历史 N-1 / N"
- ↓ → 索引递增；超过末尾 → cursor null → hint 消失
- 改写 input → onChange 守卫 set cursor null → hint 消失
- Esc → 同上
- 历史空时 ↑ → 不进入 history 模式 → hint 不显

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | hint 渲染（form 内，slash 菜单旁） |
| **M2** | tsc + build |

## 复用清单

- R129 `messageHistory / historyCursor` state
- 既有 form `position: relative` 锚

## 进度日志

- 2026-05-10 13:00 — 创建本文档；准备 M1。
- 2026-05-10 13:08 — M1 完成。form 内 slash 菜单条件渲染之后插 historyCursor !== null 条件 div：absolute top -22 right 16 浮 input bar 上方右侧；fontSize 10 monospace + muted；pointerEvents none 让 hint 不挡按钮；title 解释 ↑/↓/Esc 三键语义。
- 2026-05-10 13:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 984ms)。归档至 done。
