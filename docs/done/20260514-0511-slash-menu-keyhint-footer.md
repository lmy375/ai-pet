# SlashCommandMenu 加键盘提示 footer

## 背景

slash 命令菜单浮出后，用户能用 ↑↓ 选、Enter 执行、Tab 补全、Esc 关 —— 但这些都是隐式 keymap，菜单本身没说。新用户敲 `/` 看到列表只会用鼠标点。

加一条 footer：

```
↑↓ 选 · Enter 执行 · Tab 补全 · Esc 关
```

让发现性补齐。

## 改动

`src/components/panel/SlashCommandMenu.tsx`：

在 commands.map() 结束 + 外层 `</div>` 之前加 footer 节点：

```tsx
<div
  style={{
    padding: "5px 12px",
    borderTop: "1px solid var(--pet-color-border)",
    background: "var(--pet-color-bg)",
    fontSize: 10,
    color: "var(--pet-color-muted)",
    display: "flex",
    gap: 8,
    fontFamily: "'SF Mono', Menlo, monospace",
    letterSpacing: 0.2,
  }}
>
  <span><kbd>↑↓</kbd> 选</span>
  <span><kbd>Enter</kbd> 执行</span>
  <span><kbd>Tab</kbd> 补全</span>
  <span><kbd>Esc</kbd> 关</span>
</div>
```

`<kbd>` 标签让按键名在主题里有差异化字色（与正文 muted 相比 fg 稍亮）。不强加底色 —— 防止 footer 在 dark 模式过抢眼。

empty state (`commands.length === 0`) 时**不**渲染 footer（"没有匹配的命令"只 1 行，加 footer 反而比内容还高）。

## 不做

- 不动 onKeyDown 行为：键盘逻辑已在 PanelChat 那边稳定
- 不加 "/imagehelp" 等子模式专属提示：本菜单只在 slash prefix 触发时出，子模式由 ImagePromptHistoryMenu 等其它组件管
- 不写测试：纯 view 添加，无逻辑

## 验收

- `npx tsc --noEmit` ✅
- 聊天输入框敲 `/` → 菜单浮出，底部多一行 keyboard 提示
- 敲 `/xxx` 0 命中 → 仍显原"没有匹配的命令"，无 footer

## 完成

- [x] SlashCommandMenu.tsx: footer 节点
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
