# ChatMini bubble 右键「💾 转 task」（iter #450）

## Background

ChatMini bubble ctx menu 既有「📋 复制本条」/「⌚ 含时间戳」/「🔗 复
制 task ref」「💭 再问」「↺ 重发」「📝 transient_note」等 6 个动作。
但缺一个常用入口：**一键把这条转 task**。

场景：pet 在 reply 里提出「你该整理 Downloads 了」/ owner 自己输的
brain-dump「记一下下次开会要提的几点」/ pet 给的 todo list 一行 —
owner 想立即"塞进队列回头处理"时，当前流程要切到 PanelTasks + 「+」
打开 modal + 复制粘贴 + 保存，四步。

本 iter 加「💾 转 task」 ctx menu 项 — 自动 title=前 30 字（flat
whitespace）/ body=全文 / priority=P3 / 无 due，一键创建。后续在
PanelTasks 调 priority / 加 due / 改内容。

## Changes

### `src/components/ChatMini.tsx`

#### 1. 新按钮（紧贴 🔗 复制 task ref 之后）

```tsx
{hasText && (
  <button
    onClick={async () => {
      setCtxMenu(null);
      const flat = text.replace(/\s+/g, " ").trim();
      const titleRaw = flat.slice(0, 30);
      if (!titleRaw) return;
      try {
        await invoke<string>("task_create", {
          args: { title: titleRaw, body: text, priority: 3, due: null },
        });
        setBubbleCopyIdx(ctxMenu.idx);
        window.setTimeout(() => setBubbleCopyIdx(cur => cur === ctxMenu.idx ? null : cur), 1500);
        console.log(`💾 转 task 成功：${titleRaw}`);
      } catch (err) {
        console.error("create task from bubble failed:", err);
      }
    }}
    title={`一键把这条 bubble 转 task（P3，无 due）— 标题取前 30 字「${preview}」…`}
  >
    💾 转 task
  </button>
)}
```

设计：
- **title = flat(text).slice(0, 30)**：whitespace flatten（多空格 / 换行
  → 单空格）+ trim + 前 30 字。换行 / 多空格在 title 里破坏 task list
  视觉
- **body = text 原文**：保 newline / 多空格 / 内嵌 ref token —
  PanelTasks 详情段会保留全文渲染
- **priority P3**：与桌面「+ 新建」默认值一致；快速入队不该让 owner 选
- **due null**：bubble-转-task 是 brain-dump 入队，不预设截止；owner 后
  续要 due 走 PanelTasks 编辑或 TG `/edit_due`
- **复用既有 setBubbleCopyIdx 1.5s ✓ 反馈**：与 「📋 复制本条 / ⌚ 含
  时间戳 / 🔗 task ref」复制族同视觉反馈 — 让 ctx menu 内所有"塞东
  西"动作的成功反馈一致
- **位置在 🔗 task ref 之后 / 💭 再问 之前**：copy 族 / save 族 / action
  族三段排列；💾 转 task 算 save 族（与单 bubble 关联的写盘动作），与
  下方 「📝 transient_note」（save 到 in-memory）形成"保存型动作"小簇

## Key design decisions

- **不弹 modal 让 owner 编辑 title / priority / due**：与 PanelTasks「+
  新建」full-form 互补，那个是"细心规划"入口，本 ctx 项是"快速 dump"
  入口。owner 想细调走 PanelTasks 编辑
- **不防 dup title**：task_create 后端不强校验 dup（memory_edit "create"
  允许同 title），但 dup 会让 `find_butler_task` 返第一个 match — 下游
  有概率混淆。但本场景 bubble 文本 30 字 prefix 重复概率低；如真 dup
  后端不会 throw，owner 在 PanelTasks 看到两条同名手工 rename 即可。
  不加 collision retry 逻辑保 KISS
- **不 try task_clone + dedup-suffix**：那是另一条产品决策（导入 .md
  modal 有 dup skip 策略）— 本 ctx 项偏"快速塞" 不是"精确导入"
- **title 截 30 字（不含 "…" 后缀）**：30 是产品规格定的边界。`…` U+2026
  虽合法但 `「title…」` 作 ref token 显示不直观；trim 后干净
- **不写 unit test**：纯 invoke + 字符串切片 + clipboard-style 视觉反
  馈；逻辑 trivial + 与 task_create backend 集成。GOAL.md "meaningful
  tests only" 规则下不引装饰性测试。`tsc` + `vite build` clean 即够
- **不显 toast / setMessage**：ChatMini 顶级 copyToast UI 是给"复制最近
  回复"用的，与 ctx menu 内 ✓ 反馈职责不重叠；console.log + setBubbleCopyIdx
  的 ✓ 视觉已够。Toast 反而让"低成本快速 dump" 心智变重

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.33s)
- 后端无改动 — 复用 `task_create` Tauri 命令
- 手测：ChatMini 右键 pet bubble → menu 含「💾 转 task」→ tooltip 预
  览 title 30 字 → click → 1.5s ✓ 反馈 → 切 PanelTasks 看新 task 出
  现（P3，body = 全 bubble 文本）
