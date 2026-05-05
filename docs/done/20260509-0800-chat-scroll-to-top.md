# PanelChat 长会话"跳到顶"按钮（Iter R103）

> 对应需求（来自 docs/TODO.md）：
> PanelChat 长会话"跳到顶"按钮：消息滚动区现自动滚到底，长会话回到开头需手动拖很久；scrollTop > 阈值时右下角浮 ↑ 按钮，点击 scrollIntoView 第一条 item。Slack / TG 等都是这个模式。

## 目标

PanelChat 消息滚动区有 `useEffect` 自动跟随 items 变化滚到底，新消息进
来时自动跟。但用户想回看会话起点（"我最初问的什么？"）时，得手动滚很
久。Slack / Telegram / Discord 等都有"跳到顶 / 跳到指定位置"的浮动按钮。

加 ↑ 浮动按钮：scrollTop > 200 时右下角浮出；点击 scrollTo({top:0})
smooth 动画回到开头。

## 非目标

- 不加"跳到底"按钮 —— 自动滚到底已经覆盖；只在用户翻看历史时给上行入口
- 不做 jump-to-position（输入索引跳转）—— 太重，用户用搜索面板更直接
- 不限制 hover-only 显示 —— 浮动按钮一直可见（在阈值之上）；hover-only
  会让"我刚才看到的按钮咋没了"产生疑惑

## 设计

### state + onScroll

```ts
const [scrolledFromTop, setScrolledFromTop] = useState(false);
const handleMessageScroll = () => {
  const el = scrollRef.current;
  if (!el) return;
  setScrolledFromTop(el.scrollTop > 200);
};
```

阈值 200px：约 2-3 条消息高度。少于此不显（用户接近顶部，没必要跳）。
`useState` 而非 useRef + manual force update：scroll 事件本身就触发；
state 变化让按钮 conditional 渲染走 React tree 自然。

### 包裹 scrollRef 让 absolute 定位有锚

```diff
-<div ref={scrollRef} style={{ flex: 1, overflowY: "auto", padding: "16px" }}>
-  ...items...
-</div>
+<div style={{ flex: 1, position: "relative", overflow: "hidden" }}>
+  <div
+    ref={scrollRef}
+    onScroll={handleMessageScroll}
+    style={{ height: "100%", overflowY: "auto", padding: "16px" }}
+  >
+    ...items...
+  </div>
+  {scrolledFromTop && (
+    <button
+      type="button"
+      onClick={() => scrollRef.current?.scrollTo({ top: 0, behavior: "smooth" })}
+      title="回到会话开头"
+      aria-label="scroll to top"
+      style={{
+        position: "absolute",
+        right: 16,
+        bottom: 16,
+        width: 36,
+        height: 36,
+        borderRadius: "50%",
+        border: "none",
+        background: "var(--pet-color-accent)",
+        color: "#fff",
+        fontSize: 18,
+        cursor: "pointer",
+        boxShadow: "0 2px 8px rgba(0,0,0,0.2)",
+        opacity: 0.92,
+      }}
+    >
+      ↑
+    </button>
+  )}
+</div>
```

外层 `position: relative` + `overflow: hidden` 让内层的 scroll 容器仍能
overflow-auto 滚动，但浮动按钮始终在外层视口的右下角（不被卷入滚动）。

### scroll position reset 时机

`useEffect` 自动滚到底是按 items / currentResponse 等变化触发。当用户切
换 session 时也会跑这段，scrollTop=scrollHeight 后状态会被 onScroll 触
发 reset 到 false（如果新 session 短）。但首屏 onScroll 不会自然触发，
需要手动 reset：

实际上 `el.scrollTop = el.scrollHeight` 这行赋值不会同步触发 onScroll —
但浏览器在程序滚动后会派发 scroll 事件（async）。React 异步处理是 OK 的。
session 切换 → 滚到底 → onScroll 触发 → setScrolledFromTop(scrollTop > 200)
→ 通常新 session 短就 false；超长 session 自动滚到底就 true（按钮显示，
合理）。

### 测试

无单测；手测：
- 短会话（< 5 条）：无论滚不滚动按钮都不显
- 长会话向上滚到中部：按钮显
- 点击：smooth 动画回到顶
- 滚回底部：按钮消失
- 切到短会话：按钮消失
- 切到长会话（自动滚到底）：按钮显（等同"我已经在底部"，按钮入口仍合理）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + onScroll + 包裹 scrollRef + 按钮渲染 |
| **M2** | tsc + build |

## 复用清单

- 既有 `scrollRef` 和 auto-scroll-to-bottom useEffect

## 进度日志

- 2026-05-09 08:00 — 创建本文档；准备 M1。
- 2026-05-09 08:08 — M1 完成。`scrolledFromTop` state；scrollRef 容器外加 `position: relative` + `overflow: hidden` 包裹，内层保持 `height: 100%` + `overflowY: auto`；onScroll 回调读 scrollTop 触发阈值 200 切换；scrolled=true 时渲染 absolute 圆形 ↑ 按钮（accent bg，box-shadow，opacity 0.92）锚定 right: 16, bottom: 16；click 调 `scrollTo({ top: 0, behavior: "smooth" })`。
- 2026-05-09 08:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 980ms)。归档至 done。
