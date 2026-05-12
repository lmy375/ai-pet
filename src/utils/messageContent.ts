/**
 * 多模态消息内容拆解。LLM 接口走 OpenAI compatible 时，user message 的
 * `content` 既可以是裸字符串，也可以是 `{type, ...}` parts 数组：
 *   string                                                  // 纯文本
 *   [ { type: "text", text }, { type: "image_url", image_url: { url } } ]
 *
 * 渲染层（ChatMini / 等）需要一个稳定的 "文本部分 + 图片 URL 列表" 视图，
 * 上游存什么形态都能正确显出来。后端永远透传，前端这里就是唯一拆解点。
 */

export interface TextPart {
  type: "text";
  text: string;
}
export interface ImagePart {
  type: "image_url";
  image_url: { url: string };
}
export type ContentPart = TextPart | ImagePart;
export type MessageContent = string | ContentPart[];

/// markdown 图片语法 `![alt](url)`。url 段限 `[^)\s]+` 防贪婪吃过界。
const MD_IMAGE_REGEX = /!\[[^\]]*\]\(([^)\s]+)\)/g;

/// 判定 url 是否真是图片（避免 `![](https://docs.example.com/page)` 文档链接
/// 误识别为图）。data:image/... 永远算；http 仅 png/jpg/gif/webp/svg/bmp 后缀。
function isImageUrl(url: string): boolean {
  if (url.startsWith("data:image/")) return true;
  return /^https?:\/\/.+\.(png|jpe?g|gif|webp|svg|bmp)(\?|#|$)/i.test(url);
}

/// 把 markdown 图片标记从字符串中剔除，留下纯文本。
/// 用于渲染场景 —— 文本走 parseMarkdown 时不带 `![...](...)` 残文，避免一行字
/// 里既显缩略图又显字面 markdown 链子。
function stripMdImages(text: string): string {
  return text.replace(MD_IMAGE_REGEX, "").replace(/[ \t]*\n{3,}/g, "\n\n");
}

export function extractText(content: MessageContent | unknown): string {
  if (typeof content === "string") return stripMdImages(content);
  if (!Array.isArray(content)) return "";
  return content
    .filter((p): p is TextPart => !!p && typeof p === "object" && (p as ContentPart).type === "text")
    .map((p) => stripMdImages(p.text))
    .join("\n");
}

export function extractImages(content: MessageContent | unknown): string[] {
  const out: string[] = [];
  // string content：仅靠 markdown image 语法找图。
  if (typeof content === "string") {
    let m: RegExpExecArray | null;
    const re = new RegExp(MD_IMAGE_REGEX.source, MD_IMAGE_REGEX.flags);
    while ((m = re.exec(content)) !== null) {
      if (isImageUrl(m[1])) out.push(m[1]);
    }
    return out;
  }
  if (!Array.isArray(content)) return [];
  // 数组 content：image_url parts + 同时扫 text parts 里的 markdown image。
  // 后者覆盖"用户粘贴文字时手敲了 markdown image 语法"的场景。
  for (const p of content) {
    if (!p || typeof p !== "object") continue;
    const part = p as ContentPart;
    if (part.type === "image_url") {
      const url = part.image_url?.url ?? "";
      if (url.length > 0) out.push(url);
    } else if (part.type === "text") {
      const re = new RegExp(MD_IMAGE_REGEX.source, MD_IMAGE_REGEX.flags);
      let m: RegExpExecArray | null;
      while ((m = re.exec(part.text)) !== null) {
        if (isImageUrl(m[1])) out.push(m[1]);
      }
    }
  }
  return out;
}
