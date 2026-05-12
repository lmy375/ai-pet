# /image 历史 prompt 菜单显缩略图

## 需求

`/image` 输入触发的历史 prompt 菜单只显文字 prompt（最近 5 条），用户记不住
"上次那条 'realistic miku at sunset' 画出来好不好看"。给每条历史加 24×24 缩
略图（生成成功的首图）让"上次画的就是这条"可视。

## 实现

### `src/components/panel/slashCommands.ts`

- 新 export `ImagePromptEntry = { prompt: string; thumb?: string }`
- `readImagePrompts()` 返回类型升级为 `ImagePromptEntry[]`，向后兼容老 `string[]`：
  - 老 string → `{ prompt: v }`
  - 新 object → 校验 prompt 字段后保留 thumb（如果有）
- `writeImagePrompts(list: ImagePromptEntry[])` 写新格式
- `recordImagePrompt(prompt)` dedupe 时回填旧条目 thumb（用户连续用同 prompt
  不会"画面瞬时丢"，下次 attachThumb 自动更新）
- 新 `attachThumbToImagePrompt(prompt, thumbDataUrl)`：findIndex 命中后写
  thumb；找不到 noop（用户在生成期间敲新 prompt 把老的挤出 cap 时容忍）

### `src/components/panel/ImagePromptHistoryMenu.tsx`

- props 类型从 `prompts: string[]` 改 `prompts: ImagePromptEntry[]`
- 每行布局：`<img 24x24>` / `🎨` fallback + prompt 文本 + ellipsis；用 flex
  让"有图 / 无图"行的文字起点对齐
- onSelect 接收 `entry.prompt` 字符串保持回调签名兼容

### `src/components/panel/PanelChat.tsx`

- import `attachThumbToImagePrompt` + `type ImagePromptEntry`
- 新 helper `makeImagePromptThumb(dataUrl, maxSize=64)`：canvas drawImage 等比
  缩到 64px 短边内 + `toDataURL("image/jpeg", 0.7)`。失败 reject 让调用者吞掉
- `allImagePrompts: string[]` → `ImagePromptEntry[]`
- `imagePromptHistory` filter 改用 `e.prompt.toLowerCase().includes(q)`
- Enter / Tab 键 picked 后访问 `picked.prompt` 而非 picked
- `runImageGenerate` 成功路径（`result.urls.length > 0`）：拿 `result.urls[0]`
  → `makeImagePromptThumb` → `attachThumbToImagePrompt(prompt, thumb)` →
  `setAllImagePrompts(readImagePrompts())` 让正打开的菜单立即看到新图

## 容量考量

5 条 cap × 64×64 JPEG 0.7 ≈ 每条 ~6KB → ~30KB localStorage 占用，远低于 5MB
默认配额。即便用户切到大图（如 1024×1024）的 prompt 列表，缩略图仍是 64px
压缩态，无需额外手动管理。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 首次启动 → 老用户存的 `string[]` 历史自动转 `{prompt}` 无 thumb，菜单仍可
    用（fallback 🎨 emoji）
  - 敲 `/image realistic miku` 回车 → 成功生成 → 下次再敲 `/image` 触发历史菜
    单 → "realistic miku" 行左侧出现 24px 缩略图
  - 改用同一 prompt 重新生成不同结果 → dedupe 保留旧 thumb 直到新 thumb 算
    出来回填（< 100ms 通常）
  - 生成失败 → 历史仍记录文字 prompt（无 thumb）；retry 后成功才补 thumb
  - 关 panel 再开 → 缩略图保留（localStorage 持久）

## 不在本轮范围

- 没做"同 prompt 多次生成都保留 N 张缩略图"：cap 是 5 条 prompt 不是 5 张
  图；每个 prompt 只留最新一次首图，避免菜单行太宽
- 没做"hover 缩略图放大预览"：24px 已足够认出，再做 lightbox 反而拖慢菜单
  打开速度
- 没做后端持久（独立文件 / DB）：localStorage 完全够用且天然 per-user

## TODO 池

清空后按规则 #1 自主提出 5 条新需求。

## TODO 池新提案

1. PanelChat 顶部 session 横排 tab-like 标签栏
2. PanelDebug LLM 日志多 chip 过滤（model / round / tool）
3. ChatMini 历史 bubble 单条复制按钮
4. PanelTasks origin 过滤 chip（user / pet）
5. PanelSettings motion mapping 即点即触发预览
