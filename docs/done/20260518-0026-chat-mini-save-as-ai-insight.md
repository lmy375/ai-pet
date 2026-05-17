# ChatMini 选区 toolbar「📚 加到 ai_insights」按钮（iter #430）

## Background

ChatMini 选区 toolbar 既有 📝 note 入口 — 写选段到 `general` cat
（杂项 brain-dump）。但 owner 反思 / 自我洞察类内容（"今天回顾：
我对中断接受度过高"）想存到 `ai_insights` cat（与 /reflect TG
命令 / PanelMemory AI 洞察段同后端）时只能切到 PanelMemory 手敲。

本 iter 加 📚 选区按钮 — 与 📝 note 同 channel pattern 但分流到
`ai_insights` cat，避免 ai_insights 段被日常杂项稀释。

## Changes

### `src/App.tsx`

#### 新 `handleMiniSaveAsAiInsight` callback

```ts
const handleMiniSaveAsAiInsight = useCallback(async (text: string) => {
  const body = text.trim();
  if (!body) return;
  const now = new Date();
  // title: reflect-YYYY-MM-DDTHH-MM-SS（与 /reflect TG 命令同命名）
  const title = `reflect-${y}-${mo}-${d}T${hh}-${mm}-${ss}`;
  try {
    await invoke<string>("memory_edit", {
      action: "create",
      category: "ai_insights",
      title,
      description: body,
      detailContent: null,
    });
    appendAssistant(`📚 已记到 ai_insights/${title}`);
  } catch (e) {
    appendAssistant(`📚 记 ai_insights 失败：${e}`);
  }
}, [appendAssistant]);
```

与 `handleMiniSaveAsNote` 同模板但 cat=`ai_insights` + title 前缀
`reflect-`。与 TG `/reflect` 命名约定一致（命名规范跨 surface 统一）。

通过 `onSaveAsAiInsight={handleMiniSaveAsAiInsight}` prop 传给
ChatMini。

### `src/components/ChatMini.tsx`

#### 1. props 新增

```ts
onSaveAsAiInsight?: (text: string) => void;
```

#### 2. 解构 + selection toolbar 按钮

紧贴 📝 note 按钮之后：

```tsx
{onSaveAsAiInsight && (
  <button onClick={() => {
    const text = selectionToolbar.text;
    setSelectionToolbar(null);
    onSaveAsAiInsight(text);
  }}>
    📚
  </button>
)}
```

emoji 📚 与 📝 同视觉重量但分类语义清楚（书 = ai_insights 学
习沉淀 / 笔 = note 杂项随手记）。title attr 显完整 "反思 / 自我
洞察"对比 "杂项 brain-dump" 引导 owner 分流入口。

## Key design decisions

- **分流 cat 而非合并**：与 /note vs /reflect TG 命令分类决策一致
  — 让 ai_insights 段保「反思 / 洞察」语义纯净，不被日常杂项稀释
- **title 用 reflect- 前缀**：与 TG /reflect 同约定 — owner 在
  PanelMemory ai_insights 段看到 reflect-* item 一眼知道 "这是
  我在 ChatMini / TG 写的反思" — 跨 surface 来源统一
- **复用 onSetTransientNote pattern**：与既有 onSaveAsTask /
  onSaveAsNote 同 channel — prop 传 + App.tsx 提供 callback；
  ChatMini 不依赖 App / Tauri，可独立测试 / mock
- **不引 "选 cat" picker**：分流二选一已覆盖 95% 场景（task /
  note / ai_insights）；更细分类（chat_persona / todo / user_profile）
  不该在 selection toolbar 浮 — popover 太重对选段动作不匹配
- **不为单按钮引 unit test**：行为是 callback + invoke 既有 backend；
  build pass + 手测足够（选段 → 看 📚 浮 → click → 看 toast「已
  记到 ai_insights/reflect-...」→ PanelMemory ai_insights 段看
  到新 item）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.38s)
- 后端无改动 — 复用既有 memory_edit("create", "ai_insights") 同
  /reflect TG 命令 + PanelMemory 既有写入路径
