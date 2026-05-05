# PanelMemory 描述字数计数器（Iter R113）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory 描述字数计数器：description textarea 现 maxLength=300 但无实时计数提示；textarea 下方加 muted 小字 "X / 300"，>= 90% 转 amber 警示，让用户在打到上限前感知。

## 目标

PanelMemory 编辑 modal 的 description textarea 现 `maxLength={300}`，但
用户没有实时进度感。打到上限时浏览器会突然拒绝继续输入，体验"被掐断"。

加 textarea 下方"X / 300"小字：
- 默认 muted 灰
- >= 270（90%）转 amber 警示
- = 300（100%）转 red 危险

让用户提前感知"快到了"主动收笔。

## 非目标

- 不改 maxLength —— 300 是后端约定上限
- 不计 chars / words 区分 —— 中文场景下"字"概念混淆，统一用 `.length`
  即 UTF-16 code units（中文常字符 = 1 单位，emoji 等 surrogate pair 偶
  尔 = 2 但接近 300 时影响 ≤ 几个字符，不影响"快到上限"判断）
- 不联动 R91 长描述折叠 —— 那是渲染端阈值（200/120）；这里是输入端
  上限。两条独立轴

## 设计

### 阈值

```ts
const MAX = 300;
const WARN = 270; // 90%
const DANGER = 300; // 100%
```

颜色：
- < WARN → `var(--pet-color-muted)`
- >= WARN && < DANGER → amber `#a16207`（与 reminders / decision-log
  buffer-full warning 同色族）
- == DANGER → red `#dc2626`（与既有 stale red 同款）

### 渲染

textarea 之后追加小字 row：

```diff
   <textarea ... />
+  <div
+    style={{
+      fontSize: 10,
+      textAlign: "right",
+      color:
+        len >= MAX ? "#dc2626"
+        : len >= WARN ? "#a16207"
+        : "var(--pet-color-muted)",
+      marginTop: 2,
+    }}
+    title={
+      len >= MAX ? "已达 maxLength；继续输入会被浏览器拒绝"
+      : len >= WARN ? "接近 300 字上限，建议提前收笔"
+      : "描述长度限制 300 字"
+    }
+  >
+    {len} / {MAX}
+  </div>
 </div>
```

`len` 派生自 `editingItem.description.length`，inline 算（不需 useMemo）。

### 测试

无单测；手测：
- 空 textarea → "0 / 300" muted
- 输 100 字 → "100 / 300" muted
- 输 270 字 → "270 / 300" amber
- 输 300 字 → "300 / 300" red
- 切 category（butler_tasks 高 / 其它低）→ counter 跟随同 textarea

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 渲染 counter + 三档颜色 |
| **M2** | tsc + build |

## 复用清单

- 既有 textarea / s.textarea 样式
- 既有 amber / red motion 色（与 reminders / stale 一致）

## 进度日志

- 2026-05-09 18:00 — 创建本文档；准备 M1。
- 2026-05-09 18:08 — M1 完成。description textarea 之后追加 IIFE 渲染 counter：MAX=300 / WARN=270，三档 color（muted < amber < red）按 len 切换，title 提示语义随档变化；right-aligned fontSize 10 让"附属"语义不抢视觉。
- 2026-05-09 18:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
