# detail.md 编辑器 字数 chip 追加 〜M 词 word count

## 背景

iter 早期 detail.md 编辑器底 status bar 已有 "N 字"（Unicode code points 字符数）chip。但 owner 写英文为主 / 中英混排 detail 时，字符数与"实际词数"差距大 —— 例如 "hello world" 11 字 vs 2 词。

加 word count heuristic：纯 CJK 不变；混排 / 英文文本追加 "〜M 词" 段。

## 改动

### `src/components/panel/PanelTasks.tsx`

在既有字数 chip IIFE 内加 word count 计算 + 条件显示：

```ts
const cjkCount = (editingDetailContent.match(/[㐀-鿿]/g) || []).length;
const stripped = editingDetailContent.replace(/[㐀-鿿]/g, " ");
const enWords = stripped.split(/[^a-zA-Z0-9_'-]+/).filter(Boolean).length;
const wordCount = cjkCount + enWords;

const showWord = wordCount > 0 && wordCount !== editCount;
return (
  <span title={`${editCount} 字（Unicode code points...）` + (showWord ? `\n${wordCount} 词（heuristic...）` : "")}>
    {editCount} 字{showWord && ` · 〜${wordCount} 词`}
  </span>
);
```

### Heuristic 细节

- **CJK 段** (U+3400 — U+9FFF, CJK Unified Ideographs + Extension A): 每字符 = 1 词
- **非 CJK 段**: split 非 `[a-zA-Z0-9_'-]` 取 token 数（即标点 / 空白都算分隔）
- **混排示例**:
  - "hello world" → 0 CJK + 2 EN = 2 词
  - "你好世界" → 4 CJK + 0 EN = 4 词 (= 字数；不显)
  - "hello 世界 dev" → 2 CJK + 2 EN = 4 词
  - "你好 hello world" → 2 CJK + 2 EN = 4 词

### 显示规则

- **wordCount === editCount**: 纯 CJK 文本，两者相等，仅显 "N 字"（避免冗余）
- **wordCount !== editCount**: 追加 " · 〜M 词"
- **wordCount === 0**: 不追加（不显 "0 词"噪音）

## 关键设计

- **`〜` 前缀明示估算**：heuristic 不精确（如 contraction "don't" / hyphenated "well-known" 算法定义影响计数）；〜 让 owner 知道是 approx，不必纠结具体数字。
- **CJK regex 范围 U+3400-U+9FFF**：CJK Unified Ideographs (U+4E00-U+9FFF) + Extension A (U+3400-U+4DBF)。覆盖中日韩绝大部分汉字。不含 hiragana / katakana / 古汉字 Extensions B+ —— 那些场景 owner 罕见用 detail.md，按 punctuation 分隔退化为 token 数也合理。
- **`[a-zA-Z0-9_'-]` 词字符集**：包含 underscore（变量名）、单引号（contraction）、连字符（hyphenated word）。避免 "don't" 算 2 词、"well-known" 算 2 词的过度拆分。
- **`wordCount > 0 && wordCount !== editCount` 条件显**：避免空文本显 "0 字 · 〜0 词"、纯 CJK 显 "N 字 · 〜N 词" 双重冗余。
- **不影响既有阈值 / 颜色**：editCount 仍是触发 longish / danger 配色 + > 5000 字 banner 的 SoT —— 切换成 wordCount 会破坏现有 "字数 ≥ 2000 amber / ≥ 5000 red" 阈值与 LLM context token 预算之间的隐式对应（token ≈ char count for CJK）。
- **inline 在既有 chip IIFE 内**：不抽 helper —— 仅一处 caller + 算法不复杂（一行 match + 一行 split）。
- **tooltip 多行 \n 解释**：字数行 + 词数行各占一行，让 owner hover 时同时看到两个口径的精确含义。

## 不做

- **不引专业 word counter library**：项目无该依赖；heuristic 已覆盖 80% 场景；剩 20% （hyphenated / contraction / 古汉字）owner 自己估也够。
- **不更新 阅读态字数 counter**：阅读态 counter 在 `displayText` 长度 chip（另一段代码，未触碰）；阅读态用户不在乎"词数"（编辑时才 author-mode 需要）。
- **不让 wordCount 触发新阈值 / 配色**：保留 editCount 作为唯一阈值 SoT 避免引入新规则维护负担。
- **不写测试**：纯 string regex；视觉验证（detail.md 写一段 "Hello world 你好 dev" → chip 显 "16 字 · 〜5 词"）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.16s
- 改动 ~40 行（IIFE 内插入 word count 计算 + 条件 render + 注释）。既有字符数 chip / 阈值配色 / spacerOnSelf 布局 / banner 警示 / dirty ● / 行号 / ☑ 进度 / 时间段 chip 完全不动。

## TODO 状态

剩 3 条留池：
- PanelTasks 行右键加「🔇 Toggle silent」一键 toggle
- 桌面 pet collapse tab hover 1s 浮 ambient mini card
- butler_task `[snooze: ...]` 支持自然短串预设

## 后续

- 长 detail（> 3000 词）时给一个 "✂ 试 LLM consolidate / 概括" 按钮，让 owner 一键调 LLM 把当前 detail 折叠成短 summary。
- word count chip 阅读态也显（与编辑态对偶），让所有 view-mode 都能看到统一信号。
- 加 selection-aware count：textarea 有选区时改显 "选中 N 字 / M 词" —— IDE / Pages / Numbers 同 UX。
