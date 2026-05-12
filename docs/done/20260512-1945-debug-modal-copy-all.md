# PanelDebug "看上次 prompt" modal 全文复制按钮

## 需求

modal 现在分段提供"复制 prompt"和"复制 reply"两个按钮，但用户想
完整 audit 一 turn（复盘 prompt / 提 issue / 把当下上下文喂给另一
LLM 自查）时要 4 次操作：复制 prompt → 粘 → 切回复制 reply → 粘 →
还得手敲 timestamp / outcome / tools 等 meta。补一个 "📋 全文复制"
拼成 markdown 一次到位。

## 实现

`src/components/panel/PanelDebug.tsx` modal header：

- 在 close ✕ 按钮前插一个新按钮 "📋 全文复制"
- onClick 拼 markdown sections：
  - `# Proactive turn`
  - meta 行（`**Timestamp:** ...` / `**Outcome:** spoke|silent` /
    `**Tools used:** ...`）按需可缺
  - `## PROMPT` + fenced code block 包 lastPrompt
  - `## REPLY` + fenced code block 包 lastReply（空 / silent 时显
    `（空 / <silent>）`）
  - `## TOOL CALLS`（lastToolCalls 非空时）每条:
    - `### {tool name}`
    - `**arguments**` + fenced json
    - `**result**` + fenced
- `navigator.clipboard.writeText(sections.join("\n\n"))`
- 复用既有 `copyMsg` 状态显 2.5s "全文已复制" 反馈

样式与其它 modal header 按钮平行（轻 border + small radius），不抢
眼但能找到。

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 打开 "看上次 prompt" modal → 头部见 📋 全文复制 button
  - 点击 → 剪贴板装 markdown 全文（H1 标题 + meta + 三段 fenced code）
  - 2.5s 反馈 "全文已复制"
  - 任意 turn 切换（«/»）后再点 → 新 turn 的全文进剪贴板（与 currentTurn 同步）
  - turn 无 tool calls → TOOL CALLS 段不浮（保持 markdown 干净）
  - prompt / reply 空 → 占位 "（空）" / "（空 / <silent>）"
  - 失败（剪贴板拒权限等）→ "复制失败：xxx" 4s 反馈
- 粘到 chat / issue → 渲为结构化 markdown（H1 / H2 / 代码块），方便审

## 不在本轮范围

- 没做 "复制为 JSON"（让程序化分析）：markdown 已经覆盖人类阅读 + 多
  数 LLM 二次输入；JSON 是工程化场景，下一轮可加
- 没做 "复制最近 N 次 turns"：单 turn 已经覆盖最常见 case；多 turn
  需排版 / 时序分组，工作量翻倍
- 没改既有"复制 prompt" / "复制 reply" 分段按钮：分段按钮是窄场景
  仍有用（只想看 prompt 而不带 reply 噪声等），与全文复制是不同动
  作，并存

## TODO 池剩余

- PanelTasks 任务卡详情区 raw_description > 300 字时折叠 + 展开按钮
- PanelChat 长 assistant 消息（> 1000 字）默认折叠中段
