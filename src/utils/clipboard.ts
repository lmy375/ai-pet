/**
 * 把 data URL / http URL 指向的图片以**二进制**写到剪贴板（而非"图片地址字符串"
 * 或 markdown 引用）。其它 app（飞书 / Notion / 设计软件）粘贴时自动当图片处理。
 *
 * 浏览器要求 secure context（https / file:// / Tauri WebView 都满足）。多数引擎
 * 自动把不被原生支持的格式（如某些 SVG）转 PNG，failsafe 是抛错让 caller 显错给
 * 用户。
 */
export async function copyImageToClipboard(src: string): Promise<void> {
  const resp = await fetch(src);
  if (!resp.ok) {
    throw new Error(`fetch image failed: ${resp.status}`);
  }
  const blob = await resp.blob();
  // Safari/WKWebView 对部分 mime（image/webp）支持有限；image/png 通用，浏览器会
  // 在必要时自动转码。caller 不需关心 mime 选择。
  await navigator.clipboard.write([
    new ClipboardItem({ [blob.type]: blob }),
  ]);
}
