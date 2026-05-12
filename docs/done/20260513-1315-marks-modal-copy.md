# PanelChat marks modal "📋 复制" 按钮

## 需求

iter #225 modal 显标记消息列表，iter #227 加搜索过滤。但用户想把当
前 filtered 列表 share 出去（给同事 / 提 issue），还得逐条 cmd-c。
补 "📋 复制" 按钮把当前可见 entries 拼成 markdown。

## 实现

`src/components/panel/PanelChat.tsx` modal header 在 search 输入框
和 ✕ 之间插入 📋 复制按钮：

- 仅 `marksModalEntries !== null && length > 0` 时浮（loading / empty
  时不渲染）
- onClick 拼 markdown：
  - H1 标题 + 时间戳 + "共 N 条"（搜索时附 "过滤：xxx"）
  - 每条 H2 段：`{role icon} {session title} · #{idx+1} · 标记于 ts`
    + 内容
- 过滤逻辑与 modal 列表渲染共用同一 includes() 路径
- 成功后 1.5s "✓ 已复制" 反馈（绿字 + bold），与 chat 消息复制按钮
  同款 UX
- 新 state `marksModalCopied: boolean`

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - modal 加载完 + 有 entries → 显 "📋 复制" 按钮
  - 点 → 剪贴板装 markdown + 按钮变 "✓ 已复制" 1.5s
  - 1.5s 后 revert
  - 输入 search query → "📋 复制" 仍工作，复制结果是 filtered 子集
  - search 命中 0 → noop（filter check 内）
  - 粘到 GitHub issue / Notion / Slack → 渲清晰多段结构
  - 老格式 entries（ts=0）不显标记时间，其它信息齐全

## 不在本轮范围

- 没把 markdown 段落里的 ref token / URL 等做特殊处理：保留 raw 文
  本最稳；外部编辑器自己渲染
- 没做"复制带 frontmatter" / "复制为 JSON"：单一 markdown path 覆盖
  人类阅读 + LLM 二次输入
- 没把按钮做成 button group（如 复制选中 / 复制全部）：modal 当前没
  multi-select；"过滤后全部"已足够

## TODO 池剩余

- PanelMemory 顶部 export 单 category 下拉
- PanelDebug "立即开口" 加 "✏️ 编辑临时 prompt"
