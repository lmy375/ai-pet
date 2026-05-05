# PanelChat "发送" 按钮 hover 强化（Iter R142）

> 对应需求（来自 docs/TODO.md）：
> PanelChat "发送"按钮 hover 强化：现 inline 的 send button 仅 cursor pointer；hover 时无视觉反馈。加 className + CSS rule 让 hover 时 bg 加深一档（accent / 80% darken）+ subtle scale，给 click 物理感。

## 目标

PanelChat 输入栏的 "发送" 按钮：默认 accent bg + 白字 + cursor pointer。
hover 时无视觉反馈，让"按钮可点 / 不可点"心理边界模糊（特别是 disabled
loading 状态切回 active 后用户难判断是否生效）。

加 hover：
- bg 加深 ~10%（用 filter brightness 0.92 简单又跨色域）
- 微弱 scale(0.98)，给 click 触感

disabled 时不应用 hover —— 通过 `:not(:disabled)` 选择器保护。

## 非目标

- 不动 disabled 时灰 bg —— 既有清晰
- 不动按钮文字 / padding / radius —— 仅 hover 视觉
- 不引入 active state 单独样式（mousedown 时压缩感）—— hover scale 已够

## 设计

### CSS

PanelChat 已有 `<style>` block（line 753）。追加：

```css
.pet-chat-send:not(:disabled):hover {
  filter: brightness(0.92);
  transform: scale(0.98);
}
.pet-chat-send {
  transition: filter 0.12s ease, transform 0.08s ease;
}
```

`filter: brightness(0.92)` 在 accent 蓝上让 bg 略深；跨主题（light/dark
accent 不同色）都生效不需 hardcode 颜色。

### className

```diff
 <button
   type="submit"
+  className="pet-chat-send"
   disabled={isLoading}
   style={{...}}
 >
   {isLoading ? "..." : "发送"}
 </button>
```

### 测试

无单测；手测：
- 非 loading 时 hover → bg 略深 + 按钮微缩；移开恢复
- loading 时 hover → 灰 bg 不变（disabled 路径）
- 点击瞬间 → scale 短促压缩感
- light / dark 切换：filter brightness 仍工作

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `<style>` rule + className |
| **M2** | tsc + build |

## 复用清单

- 既有 `<style>` block (line 753)
- 既有 send button accent bg

## 进度日志

- 2026-05-10 23:00 — 创建本文档；准备 M1。
- 2026-05-10 23:08 — M1 完成。`<style>` block 末追加 `.pet-chat-send` transition + `:not(:disabled):hover` filter brightness 0.92 + scale 0.98 rule；send button 加 className="pet-chat-send"。loading 灰态 disabled 不被 hover 干扰。
- 2026-05-10 23:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 962ms)。归档至 done。
