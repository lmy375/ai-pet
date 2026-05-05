import type { ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

/// 桌面气泡用的最小 inline markdown 解析器。识别四类标记：
/// - `` `code` `` → `<code>`（**最高优先级**，内部其它标记字面保留 — 与
///   commonmark 直觉一致，避免在代码片段里被 *star 误吞）
/// - `http(s)://...` → 蓝色下划线 `<a>`（次高优先级；先于 bold/italic 让
///   `**https://x.com**` 中的 URL 整体识别而非被 bold 拆分）
/// - `**bold**` → `<strong>`（先于 italic 匹配，避免 `**foo**` 被误识别成
///   两个空斜体之间的字面 `*`）
/// - `*italic*` → `<em>`（要求左 `*` 不紧跟另一个 `*`、close 后也不是 `*`，
///   避免吞掉 `**` 边界）
///
/// 边界 / 已知小毛刺：
/// - 未闭合 token（如 `` `no close ``、`**no close`、`*no close`）一律字面输出
///   —— 不破坏内容，宁可少渲染不该错渲染。
/// - 嵌套（`**bold *italic***`）只识别外层 bold；内层 `*` 字面保留。简化实现，
///   实战 LLM 输出嵌套极少。
/// - 仅处理 inline；不识别块级（标题 / 列表 / 引用 / 代码块）。
/// - HTML 注入安全：解析器只产出 React.ReactNode，**不**走
///   `dangerouslySetInnerHTML`，所以即便 input 含 `<script>` 也只是被 React
///   作为字面字符串渲染。
export function parseInlineMarkdown(input: string): ReactNode[] {
  const out: ReactNode[] = [];
  let buf = "";
  let i = 0;
  let key = 0;
  const flush = () => {
    if (buf.length > 0) {
      out.push(buf);
      buf = "";
    }
  };
  while (i < input.length) {
    // 1. ` … `（最高优先级）
    if (input[i] === "`") {
      const close = input.indexOf("`", i + 1);
      if (close > i) {
        flush();
        out.push(
          <code key={`md-${key++}`} style={INLINE_CODE_STYLE}>
            {input.slice(i + 1, close)}
          </code>,
        );
        i = close + 1;
        continue;
      }
    }
    // 2. http(s)://... URL（次高优先级；先于 bold 让 **https://x** 整 URL
    // 识别）。从首字符扫到下一空白；剥常见句末标点 (. , ; : ! ? 中英括号 /
    // 引号) 让 "Visit https://example.com." 不把句号包进 link。
    if (input.startsWith("http://", i) || input.startsWith("https://", i)) {
      const schemeLen = input.startsWith("https://", i) ? 8 : 7;
      let end = i;
      while (end < input.length && !/\s/.test(input[end])) end++;
      while (
        end > i + schemeLen &&
        /[.,;:!?。，；：！？)）"'”“]/.test(input[end - 1])
      ) {
        end--;
      }
      // 至少 scheme + 1 char host
      if (end > i + schemeLen) {
        flush();
        const url = input.slice(i, end);
        out.push(<UrlLink key={`md-${key++}`} url={url} />);
        i = end;
        continue;
      }
    }
    // 3. ** … **（bold）
    if (input.startsWith("**", i)) {
      const close = input.indexOf("**", i + 2);
      if (close > i + 2) {
        flush();
        out.push(<strong key={`md-${key++}`}>{input.slice(i + 2, close)}</strong>);
        i = close + 2;
        continue;
      }
    }
    // 4. * … *（italic）—— 排除 ** 边界
    if (input[i] === "*" && input[i + 1] !== "*") {
      const close = input.indexOf("*", i + 1);
      if (close > i + 1 && input[close + 1] !== "*") {
        flush();
        out.push(<em key={`md-${key++}`}>{input.slice(i + 1, close)}</em>);
        i = close + 1;
        continue;
      }
    }
    buf += input[i];
    i++;
  }
  flush();
  return out;
}

/// 仅识别 URL 的 parser —— 给 PanelChat 等不想启用完整 markdown 的渲染场景
/// 用（historic 消息含早期非 markdown 意识 LLM 输出，全 markdown 会让散乱
/// `*` / `-` 渲染奇怪；URL 识别误命中风险低，可单独启用）。算法与
/// `parseInlineMarkdown` 内的 URL 分支同源（剥句末标点、scheme + 1 char host
/// 才算有效）。
export function parseUrls(input: string): ReactNode[] {
  const out: ReactNode[] = [];
  let buf = "";
  let i = 0;
  let key = 0;
  const flush = () => {
    if (buf.length > 0) {
      out.push(buf);
      buf = "";
    }
  };
  while (i < input.length) {
    if (input.startsWith("http://", i) || input.startsWith("https://", i)) {
      const schemeLen = input.startsWith("https://", i) ? 8 : 7;
      let end = i;
      while (end < input.length && !/\s/.test(input[end])) end++;
      while (
        end > i + schemeLen &&
        /[.,;:!?。,;:!?)）"'”“]/.test(input[end - 1])
      ) {
        end--;
      }
      if (end > i + schemeLen) {
        flush();
        const url = input.slice(i, end);
        out.push(<UrlLink key={`url-${key++}`} url={url} />);
        i = end;
        continue;
      }
    }
    buf += input[i];
    i++;
  }
  flush();
  return out;
}

/// 蓝色下划线 URL 链接，点击调 plugin-opener 打开默认浏览器。`stopPropagation`
/// 防止冒泡触发气泡 onClick（dismiss + R1b 反馈）；`preventDefault` 防 Tauri
/// WebView 自身尝试导航（webview 没有"新标签页"语义，会让 webview 内空白）。
function UrlLink({ url }: { url: string }) {
  return (
    <a
      href={url}
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        openUrl(url).catch((err) => console.error("openUrl failed:", err));
      }}
      style={{
        color: "#0ea5e9",
        textDecoration: "underline",
        cursor: "pointer",
        wordBreak: "break-all",
      }}
    >
      {url}
    </a>
  );
}

/// 行内代码段的内联样式 —— 与气泡整体的 13px / 蓝白色调协调，给代码片段一个
/// 视觉"框"让用户一眼能区分。背景色与 panel 任务行 priBadge 同色系（暖色），
/// 避免与气泡蓝边互相争抢。
const INLINE_CODE_STYLE: React.CSSProperties = {
  fontFamily: "'SF Mono', 'Menlo', monospace",
  fontSize: "12px",
  background: "#fef3c7",
  color: "#92400e",
  padding: "0 4px",
  borderRadius: "3px",
};

/// 桌面气泡用的最小 block-level markdown 解析器。在 `parseInlineMarkdown` 之
/// 上叠加按行处理：
/// - 行首 `- ` 或 `* `（允许前导空格） → 列表项：`<div>• ...</div>`，左缩进
/// - 空行 → 段落间 4px 视觉 gap
/// - 其它行 → 普通 `<div>{parseInlineMarkdown(line)}</div>`
///
/// 每行渲染为 block-level `<div>`，自带换行，不需 `<br>`。行内继续走
/// `parseInlineMarkdown`，所以 `**bold**` / `` `code` `` 等仍生效。
///
/// 不识别有序列表 / 表格 / 引用 / 代码块 / 标题 / 链接 / 图片 —— 桌面气泡 max-
/// height 80px 容不下复杂排版；如未来需要，扩展点是这个函数。
export function parseMarkdown(input: string): ReactNode[] {
  const out: ReactNode[] = [];
  const lines = input.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();
    if (trimmed.length === 0) {
      out.push(<div key={`md-blk-${i}`} style={PARAGRAPH_GAP_STYLE} />);
      continue;
    }
    // `- ` 或 `* ` 前导（允许任意空格）→ 列表项
    const listMatch = line.match(/^(\s*)[-*]\s+(.*)$/);
    if (listMatch) {
      const body = listMatch[2];
      out.push(
        <div key={`md-blk-${i}`} style={LIST_ITEM_STYLE}>
          <span style={{ marginRight: 4 }}>•</span>
          {parseInlineMarkdown(body)}
        </div>,
      );
      continue;
    }
    // 普通行
    out.push(
      <div key={`md-blk-${i}`}>{parseInlineMarkdown(line)}</div>,
    );
  }
  return out;
}

const PARAGRAPH_GAP_STYLE: React.CSSProperties = {
  height: 4,
};

const LIST_ITEM_STYLE: React.CSSProperties = {
  paddingLeft: 8,
  display: "flex",
  alignItems: "flex-start",
  gap: 0,
};
