import { useState } from "react";
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

/// 桌面气泡用的"够用的"block-level markdown 解析器。在 `parseInlineMarkdown`
/// 之上叠加按行处理，状态机消费 fence code block / 表格 / 列表 / 标题 / 段落。
/// 每个 block 渲染为 `<div>` / `<pre>` / `<table>`；inline 部分继续走
/// `parseInlineMarkdown`，所以 `**bold**` / `` `code` `` / URL 等仍生效。
///
/// 支持的块级语法：
/// - ` ``` ... ``` ` fence code（带可选 language tag）→ monospace pre 块，
///   行号 + 简单 syntax tinting（仅按 lang 给整个块上轻 tint，不深 highlight
///   减少 bundle）。
/// - `| col | col |` 表头 + `|---|---|` 分隔 + body 行 → `<table>` 渲染
/// - 行首 `- ` / `* `（允许前导空格） → 无序列表项
/// - 行首 `1. ` / `2. ` 等数字 → 有序列表项（保留数字）
/// - `# ` / `## ` / `### ` → 标题（1.4-1.0x 字号 + 加粗）
/// - 空行 → 段落间 4px 视觉 gap
/// - 其它 → 普通段落 `<div>{parseInlineMarkdown(line)}</div>`
///
/// 不识别：> 引用 / [link](url)（已通过 URL 自动识别）/ ![image] / 嵌套列表
/// 深层缩进。桌面气泡空间小，复杂排版反而干扰。
/// 调用方为 `- [ ]` / `- [x]` 任务项接 toggle 回调。`lineOffset` 让 parser
/// 可以把"slice 内 line idx"加上偏移再回传给上层，定位到完整 md 的全局行号。
/// 不传 `checkboxToggle` 时 checkbox 渲染为 disabled（read-only）；想纯纯
/// 静态显也行（与现有渲染同视觉，仅 input disabled）。
export interface ParseMarkdownOpts {
  checkboxToggle?: {
    lineOffset: number;
    onToggle: (globalLineIdx: number, checked: boolean) => void;
  };
}

export function parseMarkdown(
  input: string,
  opts?: ParseMarkdownOpts,
): ReactNode[] {
  const out: ReactNode[] = [];
  const lines = input.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    // ``` fence code block：consume lines until closing ```
    const fenceOpen = trimmed.match(/^```(\S*)\s*$/);
    if (fenceOpen) {
      const lang = fenceOpen[1] || "";
      const codeLines: string[] = [];
      let j = i + 1;
      while (j < lines.length && !lines[j].trim().match(/^```\s*$/)) {
        codeLines.push(lines[j]);
        j++;
      }
      out.push(
        <FenceCodeBlock
          key={`md-blk-${i}`}
          lang={lang}
          code={codeLines.join("\n")}
        />,
      );
      // skip past closing fence (j < lines.length 时是闭合行；否则 EOF，
      // 把剩余视为代码块，不报错)
      i = j;
      continue;
    }

    // 表格：当前行是 `|...|` + 下一行是 `|---|---|` separator 时开始消费
    if (trimmed.startsWith("|") && trimmed.endsWith("|") && i + 1 < lines.length) {
      const nextTrim = lines[i + 1].trim();
      const isSeparator = /^\|[\s:|-]+\|$/.test(nextTrim) && nextTrim.includes("-");
      if (isSeparator) {
        const splitRow = (row: string) =>
          row
            .replace(/^\|/, "")
            .replace(/\|$/, "")
            .split("|")
            .map((c) => c.trim());
        const header = splitRow(trimmed);
        const bodyRows: string[][] = [];
        let j = i + 2;
        while (
          j < lines.length &&
          lines[j].trim().startsWith("|") &&
          lines[j].trim().endsWith("|")
        ) {
          bodyRows.push(splitRow(lines[j].trim()));
          j++;
        }
        out.push(
          <table key={`md-blk-${i}`} style={TABLE_STYLE}>
            <thead>
              <tr>
                {header.map((h, k) => (
                  <th key={k} style={TABLE_TH_STYLE}>
                    {parseInlineMarkdown(h)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {bodyRows.map((row, ri) => (
                <tr key={ri}>
                  {row.map((c, ci) => (
                    <td key={ci} style={TABLE_TD_STYLE}>
                      {parseInlineMarkdown(c)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>,
        );
        i = j - 1; // for-loop ++ 后跳到 j
        continue;
      }
    }

    if (trimmed.length === 0) {
      out.push(<div key={`md-blk-${i}`} style={PARAGRAPH_GAP_STYLE} />);
      continue;
    }

    // 引用块 `> ...`：consume 连续的 `>` 起首行合并为单个 blockquote
    // 容器。与 fence code block 同模式（合并消费），让多行引用渲成"一整段"
    // 视觉而非每行独立 border。同时保 `>` 后空格灵活（`>` 单字 / `> text`
    // / `>text` 都接受）；本段不支持嵌套引用 `>>` —— 默认按 1 级渲染。
    if (line.match(/^>(\s|$)/) || line.startsWith(">")) {
      // 二次校验：line.startsWith(">") 可能命中 `>>=`（C bit-shift 等）；
      // 真正的 markdown 引用规则要求 `>` 后跟空白 / EOL / EOF。第一 regex
      // 已覆盖；这里若 startsWith 单独命中（如 `>text` 无空白）只有当首字
      // 后无 ASCII 字母 / 数字时才作引用，否则跳到下面普通行处理。
      const isQuote =
        /^>(\s|$)/.test(line) ||
        // 接受 `>text` 这种"忘加空格"的常见误写
        (line.startsWith(">") && line.length > 1 && line[1] !== ">");
      if (isQuote) {
        const quoteLines: string[] = [];
        let j = i;
        while (j < lines.length) {
          const l = lines[j];
          if (
            /^>(\s|$)/.test(l) ||
            (l.startsWith(">") && l.length > 1 && l[1] !== ">")
          ) {
            // 剥首位 `>` + 可选空白
            quoteLines.push(l.replace(/^>\s?/, ""));
            j++;
          } else {
            break;
          }
        }
        out.push(
          <div
            key={`md-blk-${i}`}
            style={{
              borderLeft:
                "3px solid color-mix(in srgb, var(--pet-color-accent) 50%, var(--pet-color-border))",
              padding: "4px 10px",
              margin: "4px 0",
              color: "var(--pet-color-muted)",
              background:
                "color-mix(in srgb, var(--pet-color-accent) 4%, transparent)",
              borderRadius: "0 4px 4px 0",
            }}
          >
            {quoteLines.map((ql, k) => (
              <div key={k} style={{ lineHeight: 1.55 }}>
                {ql.length === 0 ? " " : parseInlineMarkdown(ql)}
              </div>
            ))}
          </div>,
        );
        i = j - 1; // for-loop ++ 后跳到 j
        continue;
      }
    }

    // 标题 # / ## / ###（最多三级，避免无意义大字号占气泡空间）
    const headingMatch = trimmed.match(/^(#{1,3})\s+(.*)$/);
    if (headingMatch) {
      const level = headingMatch[1].length;
      const body = headingMatch[2];
      const fontSize = level === 1 ? "1.25em" : level === 2 ? "1.1em" : "1.0em";
      out.push(
        <div
          key={`md-blk-${i}`}
          style={{
            fontWeight: 600,
            fontSize,
            marginTop: 4,
            marginBottom: 2,
          }}
        >
          {parseInlineMarkdown(body)}
        </div>,
      );
      continue;
    }

    // 有序列表：行首 `<digit>. `
    const olMatch = line.match(/^(\s*)(\d+)\.\s+(.*)$/);
    if (olMatch) {
      const num = olMatch[2];
      const body = olMatch[3];
      out.push(
        <div key={`md-blk-${i}`} style={LIST_ITEM_STYLE}>
          <span style={{ marginRight: 4, minWidth: 16, color: "var(--pet-color-muted)" }}>
            {num}.
          </span>
          {parseInlineMarkdown(body)}
        </div>,
      );
      continue;
    }

    // GitHub-flavored task list `- [ ]` / `- [x]` / `- [X]`：在普通无序列表
    // 之前匹配（普通列表的 regex 会把 `- [ ]` 也吃进去，所以这里得先发）。
    // checkboxToggle 提供时变成可勾选交互（点击触发 onToggle(全局行号, 新状态)）；
    // 不提供时仍渲染 input 但 disabled —— 让读 / 写视图视觉一致，差异只是
    // 能否点。`[ x]` 之后用 `parseInlineMarkdown` 渲 body（保留链接 / 粗体）。
    const taskMatch = line.match(/^(\s*)- \[([ xX])\]\s+(.*)$/);
    if (taskMatch) {
      const checked = taskMatch[2].toLowerCase() === "x";
      const body = taskMatch[3];
      const toggle = opts?.checkboxToggle;
      const globalIdx = (toggle?.lineOffset ?? 0) + i;
      out.push(
        <div key={`md-blk-${i}`} style={LIST_ITEM_STYLE}>
          <input
            type="checkbox"
            checked={checked}
            disabled={!toggle}
            onChange={
              toggle
                ? (e) => toggle.onToggle(globalIdx, e.currentTarget.checked)
                : undefined
            }
            // 与 panel-global focus / Live2D 区视觉对齐：accent 跟随主题；
            // marginRight 与 bullet `•` 同 4px 节奏；flexShrink 防 body 长
            // 时 checkbox 被挤走。
            style={{
              marginRight: 6,
              flexShrink: 0,
              accentColor: "var(--pet-color-accent)",
              cursor: toggle ? "pointer" : "default",
            }}
            aria-label={checked ? "已完成的待办" : "未完成的待办"}
          />
          <span
            style={
              checked
                ? { textDecoration: "line-through", opacity: 0.6 }
                : undefined
            }
          >
            {parseInlineMarkdown(body)}
          </span>
        </div>,
      );
      continue;
    }

    // 无序列表：`- ` 或 `* `（允许任意前导空格）
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
    out.push(<div key={`md-blk-${i}`}>{parseInlineMarkdown(line)}</div>);
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

/// Fence code block 渲染组件。hover 时右上角浮 📋 复制按钮（与 lang badge
/// 同 corner 错开排）；点击 navigator.clipboard.writeText + 1.5s "✓" 反馈。
/// 必须是单独组件而不是 inline JSX —— useState 不能放在 parseMarkdown 这
/// 种纯函数循环内（hook 顺序规则）。
function FenceCodeBlock({ lang, code }: { lang: string; code: string }) {
  const [hovered, setHovered] = useState(false);
  const [copied, setCopied] = useState(false);
  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch (err) {
      console.error("copy code failed:", err);
    }
  };
  return (
    <pre
      style={CODE_BLOCK_STYLE}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {lang && <span style={CODE_LANG_BADGE_STYLE}>{lang}</span>}
      {/* 复制按钮：默认仅 hover 时显（与 lang badge 配色错开避免色块挤）。
          copied 状态强制显 1.5s 让用户看到反馈。 */}
      {(hovered || copied) && (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            void handleCopy();
          }}
          style={{
            ...CODE_COPY_BTN_STYLE,
            color: copied ? "#16a34a" : CODE_COPY_BTN_STYLE.color,
            borderColor: copied ? "#86efac" : (CODE_COPY_BTN_STYLE.borderColor as string),
          }}
          title={copied ? "已复制 code 到剪贴板" : "复制 code 到剪贴板"}
          aria-label="copy code"
        >
          {copied ? "✓" : "📋"}
        </button>
      )}
      <code>{code}</code>
    </pre>
  );
}

/// fence code block 整体样式。背景与 inline code 同色系（暖琥珀）但稍浅 +
/// 边框更柔，让多行块感觉是"代码段"而非"badge"。语言标签飘在右上角，给阅
/// 读者"这是什么语言"信号。
const CODE_BLOCK_STYLE: React.CSSProperties = {
  fontFamily: "'SF Mono', 'Menlo', monospace",
  fontSize: "11.5px",
  background: "#fffbeb",
  color: "#78350f",
  border: "1px solid #fde68a",
  borderRadius: 4,
  padding: "6px 8px",
  margin: "4px 0",
  position: "relative",
  overflowX: "auto",
  whiteSpace: "pre",
  lineHeight: 1.4,
};

const CODE_LANG_BADGE_STYLE: React.CSSProperties = {
  position: "absolute",
  top: 2,
  left: 4,
  fontSize: 9,
  color: "#a16207",
  fontWeight: 500,
  letterSpacing: 0.3,
  userSelect: "none",
};

/// fence code block 复制按钮：hover-only 右上角浮窗。border + bg 让按钮在
/// 暖琥珀代码块底色上仍可读。
const CODE_COPY_BTN_STYLE: React.CSSProperties = {
  position: "absolute",
  top: 2,
  right: 4,
  padding: "1px 6px",
  fontSize: 10,
  lineHeight: 1.2,
  border: "1px solid #fde68a",
  borderRadius: 3,
  background: "#fffbeb",
  color: "#a16207",
  cursor: "pointer",
  fontFamily: "inherit",
  userSelect: "none",
};

/// 表格样式：紧凑 padding（mini 空间紧），细线 border 与气泡背景区分。
const TABLE_STYLE: React.CSSProperties = {
  borderCollapse: "collapse",
  margin: "4px 0",
  fontSize: "11.5px",
};

const TABLE_TH_STYLE: React.CSSProperties = {
  padding: "3px 6px",
  borderBottom: "1px solid var(--pet-color-border)",
  background: "var(--pet-color-bg)",
  fontWeight: 600,
  textAlign: "left",
};

const TABLE_TD_STYLE: React.CSSProperties = {
  padding: "2px 6px",
  borderBottom: "1px solid var(--pet-color-border)",
};
