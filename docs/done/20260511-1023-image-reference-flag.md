# /image -r 引用最近 assistant 文本

## 需求

用户跟宠物聊出了一段描述 → 想画下来。当前要复制 assistant 文本再敲
`/image <粘贴>`。`/image -r <补充描述>` 一步把最近 assistant 文本拼到 prompt
前，省一次复制粘贴。`-r` 可单独使用（用 assistant 原文作 prompt）也可与
`-n` 任意顺序组合。

## 实现

### 解析

`src/components/panel/slashCommands.ts`：

- SlashAction `image` 加 `referenceLastAssistant: boolean`
- 顺序 peel flag：循环最多 2 次试匹配 `-n N` 或 `-r`，匹配到就剥掉；之后
  剩余 = prompt 文本。flag 重复 / N 越界 → unknown。
- `-r` 允许 prompt 为空（"用 assistant 原文作为 prompt"）；其它情况空 prompt
  仍当 unknown
- 命令面板 description 加新用法："引用上文 /image -r <描述>"

### 拼接

`PanelChat.executeSlash case "image"`：

- referenceLastAssistant=true 时倒序找最近一条 assistant 行（trim 后非空），
  text + "\n\n" + 用户 prompt 作为 effectivePrompt
- 找不到 → pushLocalAssistantNote 提示并 break，不发请求
- 用户回声 echo content 显 `-n N -r <prompt>` 组合，与用户输入对齐
- recordImagePrompt 用 effectivePrompt（含 assistant 文本）入历史，复用时可以
  完整带回上下文

ImagePromptHistoryMenu 的 Enter 路径手动构造 SlashAction，加 `referenceLastAssistant: false` 走默认；保留可单独 fuzzy 召回的 prompt 文本。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - `/image -r 加细节` → 拼接最近 assistant 文本 + `\n\n加细节` → 生图
  - `/image -r` → 用 assistant 文本作 prompt
  - `/image -n 4 -r 山水画风` / `/image -r -n 4 山水画风` → 都 work，flag 顺序任意
  - 会话里没 assistant → 提示用户后不发请求
  - 解析错（`-r -r ...` / `-n 100 -r ...`）→ 未知命令提示

## 不在本轮范围

- 没考虑 LLM tool path（give_image）—— 那条路径 LLM 自己拼 prompt，本来就能引
  用上下文；CLI flag 只服务用户显式输入
- 没把 ImagePromptHistoryMenu Tab 填回路径也加 -r 一起 fill —— 用户从历史选
  通常已含完整 prompt，再叠 -r 反而意外
