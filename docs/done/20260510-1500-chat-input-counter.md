# PanelChat 输入框字数 counter（Iter R134）

> 对应需求（来自 docs/TODO.md）：
> PanelChat 输入框字数 counter：textarea 内非空时下方显 "X 字" muted 小字（与 R113 PanelMemory 描述 / R121 detail.md 同模式），长 prompt 调试时让用户感知体量；空时不显避免 noise。

## 目标

PanelChat 输入框是 R126 的 auto-grow textarea，可写多行长 prompt。debug
LLM / 写复杂指令时常输 200-500 字，但用户没字数体感（textarea 高度只
反映行数不反映字数）。R113 / R119 / R121 已在 PanelMemory / PanelTasks
建立"字数 counter"模式；本轮镜像到 chat input。

非空时浮显 "N 字" muted 小字；空时不显（避免 noise）。

## 非目标

- 不限制 maxLength —— chat 消息长度受 LLM context 限制（单次发送不强限）
- 不区分 token / char —— 字符数已足够 "感知体量"，token 化要 tokenizer
  依赖
- 不做接近上限警示 —— 没有硬上限，纯信息

## 设计

### 渲染

R132 历史模式提示 hint 已浮在 form 顶右（`top: -22, right: 16`）。新 counter
浮在 form 顶左（`top: -22, left: 16`），两者不打架；都用 `pointerEvents: none`
避免阻挡按钮。

```tsx
{input.length > 0 && (
  <div
    style={{
      position: "absolute",
      top: -22,
      left: 16,
      fontSize: 10,
      color: "var(--pet-color-muted)",
      pointerEvents: "none",
      fontFamily: "'SF Mono', 'Menlo', monospace",
      whiteSpace: "nowrap",
    }}
    title="当前消息字符数（Unicode code units 计；含换行 / 空白）"
  >
    {input.length} 字
  </div>
)}
```

`top: -22` 与 R132 history hint 同高度；left vs right 错开。

### 与 R132 history hint 共存

两 hint 同时可能出现：用户从 history 召回一条长消息时 historyCursor 非
null + input 非空，两 hint 同时显（左 "X 字"、右 "↑ 历史 i / N"）。视觉
不冲突，反而互补：用户能同时看到"我浏览到第几条"和"这条多长"。

### 测试

无单测；手测：
- 空 input → 不显
- 输 "嗨" → 显 "1 字"
- 输长段落 → 显实时更新
- 多行（含换行）→ 字数包含换行 char
- 召回历史时左右两 hint 共存，不重叠
- 提交后 input 清空 → counter 消失

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | hint 渲染（form 内，与 R132 history hint 旁） |
| **M2** | tsc + build |

## 复用清单

- 既有 form `position: relative` 锚
- R132 history hint 同款 absolute 浮 hint 模式
- R113 / R121 字数 counter 风格

## 进度日志

- 2026-05-10 15:00 — 创建本文档；准备 M1。
- 2026-05-10 15:08 — M1 完成。form 内 R132 历史 hint 之前插 `input.length > 0` 条件 div：absolute top -22 left 16；fontSize 10 monospace + muted；pointerEvents none；title 解释字符计算口径。与 R132 history hint 错开顶左 / 顶右共存。
- 2026-05-10 15:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 1.00s)。归档至 done。
