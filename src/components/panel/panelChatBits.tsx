/**
 * PanelChat 的可复用子片段：会话消息泡、搜索结果行、Markdown 导出、共享类型。
 * 抽出来让 PanelChat 主组件 < 1300 行，且这两块（消息渲染 / 跨会话搜索）
 * 都各有自己独立的"输入数据 → 视图"语义，可以单独 review、单独迭代。
 *
 * 注意保持与 PanelChat 内部的类型一致 —— `ChatItem` 是后端 session 文件
 * 里的项语义；`SearchHit` 与 Rust 端 `commands::session::SearchHit` 同 shape。
 */
import { parseUrls } from "../../utils/inlineMarkdown";

export interface ToolCall {
  name: string;
  arguments: string;
  result?: string;
  isRunning: boolean;
}

export interface ChatItem {
  type: "user" | "assistant" | "tool" | "error";
  content: string;
  toolCalls?: ToolCall[];
}

/// 与后端 `commands::session::SearchHit` 对应。`match_start` 是 char 偏移
/// （而非 byte），用于在 snippet 中精准切片高亮。
export interface SearchHit {
  session_id: string;
  session_title: string;
  session_updated_at: string;
  item_index: number;
  role: "user" | "assistant";
  snippet: string;
  match_start: number;
  match_len: number;
}

/**
 * 导出会话为 Markdown：仅 user / assistant 消息，工具调用与 error 行
 * 不写入。开头一行 `> 导出时间 · 共 N 条`，每条按 `## 🧑 user` / `## 🐾
 * assistant` 区分发言方。
 */
export function exportSessionAsMarkdown(title: string, items: ChatItem[]): string {
  const lines: string[] = [];
  const visibleItems = items.filter(
    (it) => it.type === "user" || it.type === "assistant",
  );
  lines.push(`# ${title}`);
  lines.push(
    `> 导出时间: ${new Date().toLocaleString()} · 共 ${visibleItems.length} 条消息`,
  );
  lines.push("");
  for (const it of visibleItems) {
    const glyph = it.type === "user" ? "🧑" : "🐾";
    lines.push(`## ${glyph} ${it.type}`);
    lines.push("");
    lines.push(it.content);
    lines.push("");
  }
  return lines.join("\n");
}

export function bubbleStyle(role: "user" | "assistant"): React.CSSProperties {
  return {
    maxWidth: "80%",
    padding: "10px 14px",
    borderRadius: role === "user" ? "16px 16px 4px 16px" : "16px 16px 16px 4px",
    background: role === "user" ? "var(--pet-color-accent)" : "var(--pet-color-card)",
    color: role === "user" ? "#fff" : "var(--pet-color-fg)",
    fontSize: "14px",
    lineHeight: "1.6",
    boxShadow: "0 1px 3px rgba(0,0,0,0.08)",
    wordBreak: "break-word",
    whiteSpace: "pre-wrap",
  };
}

/**
 * 单条 user / assistant 消息的渲染容器。在原有 bubble 旁边挂一个 hover-only
 * 的「复制」按钮：assistant 在 bubble 右侧，user 在 bubble 左侧（与 bubble 对齐
 * 方向相反，避免按钮被屏幕边缘挤到看不见）。`data-item-idx` 留在最外层 row
 * 以保留跨会话搜索的 scrollIntoView 路径。
 *
 * 已复制状态 (`copied = true`) 用绿色 + 强制 opacity=1 覆盖默认 hover-only 显示，
 * 让用户在松开鼠标后还能看到 1.5s 的"已复制"反馈。
 */
export function CopyableMessage({
  role,
  content,
  itemIdx,
  copied,
  onCopy,
  wrapperStyle,
}: {
  role: "user" | "assistant";
  content: string;
  itemIdx: number;
  copied: boolean;
  onCopy: (idx: number, text: string) => void;
  wrapperStyle: React.CSSProperties;
}) {
  const button = (
    <button
      type="button"
      className="pet-copy-btn"
      title={copied ? "已复制到剪贴板" : "复制这条消息"}
      onClick={() => onCopy(itemIdx, content)}
      style={{
        alignSelf: "flex-end",
        padding: "2px 6px",
        fontSize: "10px",
        lineHeight: 1.2,
        border: "1px solid var(--pet-color-border)",
        borderRadius: 4,
        background: "var(--pet-color-card)",
        color: copied ? "#16a34a" : "var(--pet-color-muted)",
        cursor: "pointer",
        whiteSpace: "nowrap",
        flexShrink: 0,
        opacity: copied ? 1 : undefined, // copied 状态强制可见，覆盖 CSS hover-only
      }}
    >
      {copied ? "已复制" : "复制"}
    </button>
  );
  // 仅识别 URL（不启用完整 markdown），避免历史里的散乱 `*` / `-` 误渲染。
  // 桌面气泡同 url 化路径但走 parseMarkdown 完整版（气泡是即时一句，无历史
  // 风险）。
  const bubble = <div style={bubbleStyle(role)}>{parseUrls(content)}</div>;
  return (
    <div className="pet-chat-row" data-item-idx={itemIdx} style={wrapperStyle}>
      {/* user 右对齐 → 按钮在 bubble 左侧；assistant 左对齐 → 按钮在 bubble 右侧 */}
      {role === "user" ? (
        <div style={{ display: "flex", alignItems: "flex-end", gap: 6 }}>
          {button}
          {bubble}
        </div>
      ) : (
        <div style={{ display: "flex", alignItems: "flex-end", gap: 6 }}>
          {bubble}
          {button}
        </div>
      )}
    </div>
  );
}

/**
 * 搜索结果单行。把 snippet 在匹配区段切三段（前 / 命中 / 后）渲染，命中段用
 * 浅黄背景与 priBadge 的同色系（与面板内其它"重点"色一致）。整行可点 → 触发
 * 跳转。
 */
export function SearchResultRow({
  hit,
  onSelect,
}: {
  hit: SearchHit;
  onSelect: (hit: SearchHit) => void;
}) {
  // snippet 按 char 切三段：[0..match_start) + [match_start..match_start+match_len) +
  // [tail..]。Array.from 分 char 数组（中文友好；string slice 在 UTF-16 surrogate
  // pair 上不安全，但 char 视角全是单 codepoint 时也仅在 emoji 等极端场景才会有差，
  // 当前内容场景安全）。
  const chars = Array.from(hit.snippet);
  const head = chars.slice(0, hit.match_start).join("");
  const mid = chars.slice(hit.match_start, hit.match_start + hit.match_len).join("");
  const tail = chars.slice(hit.match_start + hit.match_len).join("");
  const roleGlyph = hit.role === "user" ? "🧑" : "🐾";
  return (
    <div
      onClick={() => onSelect(hit)}
      style={{
        padding: "8px 12px",
        cursor: "pointer",
        borderBottom: "1px solid #f1f5f9",
      }}
      title={`跳到「${hit.session_title}」第 ${hit.item_index + 1} 条消息`}
    >
      <div style={{ fontSize: "12px", color: "var(--pet-color-fg)", lineHeight: 1.5, display: "flex", gap: 6, alignItems: "flex-start" }}>
        <span style={{ flexShrink: 0 }}>{roleGlyph}</span>
        <span style={{ wordBreak: "break-word" }}>
          {head}
          <mark style={{ background: "#fef3c7", color: "#92400e", padding: "0 1px", borderRadius: 2 }}>
            {mid}
          </mark>
          {tail}
        </span>
      </div>
      <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginTop: 2 }}>
        {hit.session_title} · {hit.session_updated_at.split("T")[0]}
      </div>
    </div>
  );
}
