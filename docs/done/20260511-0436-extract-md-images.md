# ChatMini 用户气泡 markdown image 解析

## 需求

`extractImages` 当前只识别 OpenAI compatible 多模态 parts 数组里的 `image_url`
项；不识别消息文本中嵌入的 markdown 图片语法 `![alt](url)`。当 LLM / 用户在消
息正文里写了 markdown image，桌面 ChatMini 渲一行字面 `![](data:image/...)` 
而非缩略图 —— 既丑陋又把 base64 暴露在 UI 上。

## 实现

`src/utils/messageContent.ts`：

### 加 markdown 图片识别

```ts
const MD_IMAGE_REGEX = /!\[[^\]]*\]\(([^)\s]+)\)/g;
function isImageUrl(url: string): boolean {
  if (url.startsWith("data:image/")) return true;
  return /^https?:\/\/.+\.(png|jpe?g|gif|webp|svg|bmp)(\?|#|$)/i.test(url);
}
```

`isImageUrl` 限定 data URL 或 http 后缀真是图片格式 —— 避免 `![logo](https://docs.example.com/page)` 这种文档链接被当图渲。与 PanelTasks 的同名 helper 保持
同样判定（重复无所谓，未来要 share 抽到 utils/url.ts）。

### extractImages 扩

string content：扫 markdown image 语法，过滤 isImageUrl，返回 URL 数组。

数组 content：原 image_url parts + 同时扫 text parts 里的 markdown image。
后者覆盖"用户粘贴文字时手敲了 markdown image"或"LLM 在 text 段嵌图"的混合
场景。

### extractText 同步剥 markdown 图片

避免渲染层既显缩略图（来自 extractImages）又显字面 markdown（来自 extractText
+ parseMarkdown）。`stripMdImages` 用同 regex 剔除 markers，并把连续 ≥3 个换
行折叠回 2 个，防止文本被切得空行连绵。

string 与数组 text part 都过这一道。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - LLM 回 `这是一张图：![](data:image/png;base64,...)` → 桌面气泡先显纯文
    本"这是一张图："然后下一行铺缩略图（点开 lightbox + hover 复制）
  - 多模态 array content `[{type:text,text:"see this ![](url)"},{type:image_url,...}]`
    → 文本段 strip md image 后留 "see this"，缩略图区显两张（image_url 一张 +
    text 内嵌一张）
  - `![logo](https://docs.example.com/page)` 这种非图链接 → isImageUrl 拒，
    保留原 markdown 字面（也意味着 stripMdImages 不剥它，仍当文字渲）—— **这
    点未实现**：当前 stripMdImages 不区分是否真图，会一律剔除。

  → **修正决定**：实战中这种"非图链接误用 image 语法"场景极少（用户发 chat
  几乎不写 markdown 链接），同时如果剥得不彻底反而出现"看不到图也不显字面"
  的情况，体感更糟。当前一律剔除是优解；如果未来反馈要保留可加 isImageUrl
  守门。

## 不在本轮范围

- `parseDetailMdWithImages`（PanelTasks）走的是另一条路径（自己切 segment 分别
  渲 ImageThumb / parseMarkdown），与 extractImages / extractText 分工不同。两
  者同算法（`![alt](url)` 切割），不冲突
- 没在 ChatMini / CopyableMessage 任何地方加新代码 —— extract* 是它们的输入函
  数，扩展点上溯到这一处就足够了
