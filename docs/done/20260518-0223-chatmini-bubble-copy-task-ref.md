# ChatMini bubble 右键「🔗 复制 task ref」（iter #444）

## Background

pet 在 reply 里频繁带 `「title」` ref token（task / memory item 引用 —
PanelChat / detail.md 渲染时双击跳源）。owner 想把 pet reply 里提到
的 task refs 收集起来塞到新 task description / TG /quick / detail.md
checklist 等场景时，要手工拣字符出来 + 拼空格。

本 iter 加 ChatMini bubble 右键 ctx menu 入口「🔗 复制 task ref」—
扫 bubble 文本里所有 `「...」` token，dedupe 保留顺序，拼成空格分隔
inline ref 串复制到剪贴板。粘到任何渲染 ref 的地方（PanelChat /
detail.md / PanelMemory description / task description）仍保留 ref 语
义（双击跳源 task）。

## Changes

### `src/components/ChatMini.tsx`

#### 1. refTitles 计算（紧贴 IIFE 顶 `text` extract）

```ts
const refTitlesSet: string[] = [];
if (hasText) {
  const seen = new Set<string>();
  const re = /「([^「」\n]+)」/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    const t = match[1].trim();
    if (t && !seen.has(t)) {
      seen.add(t);
      refTitlesSet.push(t);
    }
  }
}
const refTitles = refTitlesSet;
const hasRefs = refTitles.length > 0;
```

- regex `「([^「」\n]+)」`：不允许嵌套 / 跨行 token — 与 PanelChat
  parseRefTokens 同 lens
- dedupe 保留首次出现顺序 — pet reply 常重复提同 task 多次，owner
  只想要每个 ref 一份
- 空 title（trim 后）跳过 — 防 `「」` 误占位
- 不前置 task list 校验：bubble 文本里的 「」 token 就是 pet 派单
  里的 task title 引用（pet 自己生成），无需 frontend 二次校验

#### 2. 右键菜单按钮（紧贴 「⌚ 复制 · 含时间戳」之后）

```tsx
<button
  disabled={!hasRefs}
  title={hasRefs
    ? `复制 ${refTitles.length} 个 task ref：${refTitles.map(t=>`「${t}」`).join(" ")}`
    : "本条未提到任何 「title」 ref token"}
  onClick={() => {
    setCtxMenu(null);
    if (!hasRefs) return;
    const payload = refTitles.map((t) => `「${t}」`).join(" ");
    navigator.clipboard.writeText(payload).then(() => {
      setBubbleCopyIdx(ctxMenu.idx);  // 1.5s ✓ 反馈
      window.setTimeout(() => setBubbleCopyIdx(cur => cur === ctxMenu.idx ? null : cur), 1500);
    }).catch(err => console.error("copy task ref tokens failed:", err));
  }}
>
  🔗 复制 task ref{hasRefs ? ` (${refTitles.length})` : ""}
</button>
```

- disabled 时灰显（默认 `<button disabled>` 样式）+ tooltip 解释为啥
  灰
- 标签内数字 hint `(N)` 让 owner 在 hover 之前就知道有几个 ref 可复
  制 — 与既有 `「💭 针对这条再问」` 等按钮的固定文案不同，本按钮
  数据敏感所以加 hint
- 复用既有 `setBubbleCopyIdx` 1.5s ✓ 视觉反馈（与 ⌚ 含时间戳同模板）
- payload 是空格拼接（不是 comma / 中文顿号）— 与 markdown / detail.md
  `「a」 「b」` 自然渲染对齐 + 粘到 task description 可作 inline
  refs 序列

## Key design decisions

- **regex `「([^「」\n]+)」`（拒嵌套 + 跨行）**：「」嵌套 in title 是
  非法（title 本来就含中文标点用 ASCII / 中点等），跨行也不可能 —
  ref token 单行内。命中点最小 + 最直接。greedy `+` 不必怕回溯爆炸
  （bubble text < 数百 KB 量级，量纲安全）
- **dedupe 保留出现顺序**：pet 在 reply 里"整理 Downloads ... 整理
  Downloads 之后 ..." 等重复时只复制一份；按首次出现顺序粘出去时
  阅读顺序自然
- **空格分隔（不是 comma / 顿号）**：粘到 `[blockedBy: 「a」 「b」]`
  / detail.md `「a」「b」 都需要做` 等场景空格更自然；comma 反而需
  要 owner 手动删；顿号是中文标点，与 ASCII 空格分隔不一致
- **不前置 task list 校验**：frontend 不知道哪些 ref 是"真实派单"
  vs "pet 编出来 hallucinate"。但 owner 视角"pet 提到的 ref 全部
  收集"就是正确语义 —— 假如有 hallucinate，复制后 owner 自然会从粘
  贴目的端的渲染（detail.md 渲染时双击 ref 跳 source 失败）发现并
  剔除；不该在复制源处过度过滤
- **disabled 不藏 button**：与既有「💭 针对这条再问」、「↺ 重发本
  条」 role 条件化藏入口不同 — 本入口对每 bubble 语义一致（都"可能
  有 refs"），藏会让 owner 怀疑功能消失；灰显 + tooltip 解释更稳
- **不引 task list invoke 校验**：上述 + 性能（每开 ctx menu 都 invoke
  一次 list_tasks 太重）
- **不写 unit test**：纯 regex extract + array join + clipboard 副作
  用，逻辑在 `tsc` + `vite build` 通过即可保证。GOAL.md「meaningful
  tests」规则下，这种装饰性 ctx-menu 测试不引入
- **位置在 ⌚ 之后 / 💭 之前**：所有 copy-like 动作（📋 复制本条 /
  ⌚ 含时间戳 / 🔗 复制 task ref）连贯成「复制族」；下方才是 💭
  re-prompt / ↺ resend / 📝 transient_note 等「行动族」 — 视觉分
  层与心智模型对齐

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
- 后端无改动 — 纯前端 UI 增强
- 手测：右键 pet bubble 含 「a」 「b」 → 看 menu 出现「🔗 复制 task
  ref (2)」+ tooltip 含 preview → 点击 → 粘到 detail.md → 看 `「a」 「b」`
  渲为 ref tokens；无 「」 的 bubble 上 menu 出现「🔗 复制 task
  ref」灰显 + tooltip 「本条未提到任何 「title」 ref token」
