/**
 * PanelChat 的可复用子片段：会话消息泡、搜索结果行、Markdown 导出、共享类型。
 * 抽出来让 PanelChat 主组件 < 1300 行，且这两块（消息渲染 / 跨会话搜索）
 * 都各有自己独立的"输入数据 → 视图"语义，可以单独 review、单独迭代。
 *
 * 注意保持与 PanelChat 内部的类型一致 —— `ChatItem` 是后端 session 文件
 * 里的项语义；`SearchHit` 与 Rust 端 `commands::session::SearchHit` 同 shape。
 */
import { Fragment, useState } from "react";
import { parseUrls } from "../../utils/inlineMarkdown";
import { ImageLightbox } from "../common/ImageLightbox";
import { ImageThumb } from "../common/ImageThumb";

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
  /// 多模态：用户消息附带的图片，data URL 数组。在 user bubble 上方以缩略图渲染。
  /// assistant 消息暂不携带图片（图像生成任务做完后再扩）。
  images?: string[];
  /// `/image` 失败行专用：保留原 prompt 字面量，UI 看到此字段渲染重试按钮。
  /// 成功行不写（成功的内容自带 images，无需重发）。
  imageRetryPrompt?: string;
  /// 配套的 -n N 参数（多图）。重试时一并 replay；缺省 1。
  imageRetryN?: number;
  /// 配套的 -s WxH size 覆盖。重试时一并 replay；undefined / null 走 settings 默认。
  imageRetrySize?: string | null;
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
    padding: "11px 16px",
    borderRadius: role === "user" ? "18px 18px 6px 18px" : "18px 18px 18px 6px",
    background: role === "user" ? "var(--pet-color-accent)" : "var(--pet-color-card)",
    color: role === "user" ? "#fff" : "var(--pet-color-fg)",
    border: role === "user" ? "none" : "1px solid var(--pet-color-border)",
    fontSize: "14px",
    lineHeight: "1.65",
    // user bubble 走 accent 染色阴影，强调"我说的话"色相一致；assistant 走
    // 全局 shadow-sm token（迭代 1 已分主题定义），dark 主题下自动跟随。
    boxShadow:
      role === "user"
        ? "0 2px 8px color-mix(in srgb, var(--pet-color-accent) 32%, transparent)"
        : "var(--pet-shadow-sm)",
    wordBreak: "break-word",
    whiteSpace: "pre-wrap",
    transition:
      "box-shadow 160ms ease-out, transform 100ms ease-out, border-color 160ms ease-out",
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
/// 用 yellow 高亮 keyword 在 content 中所有 case-insensitive 命中，其余按
/// parseUrls 渲（URL 仍蓝下划线）。keyword 空 / 内容空 → 退到原 parseUrls
/// 路径。给跨会话搜索"跳到 hit 后内文本高亮"用，与 SearchResultRow 的 mark
/// 同色系（黄底深棕字）保持视觉一致性。
/// 把消息正文里所有 `「task_title」`（全角直角引号包裹的任务引用，⌘K
/// picker 插入的格式）替换成一个带 dotted underline + 原生 title 提示
/// 的 span。tooltip 文本拼任务的 status + 最近更新时间，让用户在聊天
/// 历史里 hover 引用 token 就能确认这条 ref 还是不是"活"任务。
/// 未在 taskMap 命中的 ref（已归档 / 重命名）走 muted 配色 + 文案提示，
/// 用户也能识别"这条 ref 已经过期"。空 taskMap / 无命中 → 退到 parseUrls。
/// `onDoubleClick` 命中时附在 span 上 —— 双击 → 切到 PanelTasks tab +
/// 聚焦该 title 的卡片（PanelApp 端 lift state 实现）。
export function renderContentWithTaskRefs(
  content: string,
  taskRefMap: Record<string, { status: string; updated_at: string }>,
  onDoubleClick?: (title: string) => void,
): React.ReactNode[] {
  // 全角「」（U+300C / U+300D）作为定界符；正文里偶发误用半角 ASCII 不命中，
  // 与 ⌘K picker 插入格式严格一致，避免误抓普通文本里的相似符号。
  const re = /「([^「」]+)」/g;
  const out: React.ReactNode[] = [];
  let lastIdx = 0;
  let segKey = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(content)) !== null) {
    const start = m.index;
    const end = start + m[0].length;
    if (start > lastIdx) {
      out.push(
        <Fragment key={`seg-${segKey++}`}>
          {parseUrls(content.slice(lastIdx, start))}
        </Fragment>,
      );
    }
    const title = m[1];
    const info = taskRefMap[title];
    const clickable = !!onDoubleClick;
    const titleAttr = info
      ? `「${title}」\n状态：${info.status}\n最近更新：${info.updated_at.slice(0, 16).replace("T", " ")}${clickable ? "\n\n双击跳到任务面板该卡片" : ""}`
      : `「${title}」\n（任务不在当前队列；可能已完成归档 / 被重命名 / 不存在）${clickable ? "\n\n双击仍尝试跳到任务面板搜索此 title" : ""}`;
    out.push(
      <span
        key={`ref-${segKey++}`}
        style={{
          textDecoration: "underline",
          textDecorationStyle: "dotted",
          textDecorationColor: info
            ? "var(--pet-color-accent)"
            : "var(--pet-color-muted)",
          textUnderlineOffset: 2,
          cursor: clickable ? "pointer" : "help",
        }}
        title={titleAttr}
        onDoubleClick={
          onDoubleClick
            ? (e) => {
                // 阻止气泡 → 让 chat 行级别的复制 / 反馈 / 行 hover 等
                // 不被双击二次触发。
                e.stopPropagation();
                onDoubleClick(title);
              }
            : undefined
        }
      >
        {m[0]}
      </span>,
    );
    lastIdx = end;
  }
  if (out.length === 0) return parseUrls(content);
  if (lastIdx < content.length) {
    out.push(
      <Fragment key={`seg-${segKey++}`}>
        {parseUrls(content.slice(lastIdx))}
      </Fragment>,
    );
  }
  return out;
}

function renderContentWithKeyword(
  content: string,
  keyword: string,
): React.ReactNode[] {
  if (!keyword) return parseUrls(content);
  const lower = content.toLowerCase();
  const klow = keyword.toLowerCase();
  const out: React.ReactNode[] = [];
  let i = 0;
  let segKey = 0;
  while (i < content.length) {
    const found = lower.indexOf(klow, i);
    if (found < 0) {
      out.push(
        <Fragment key={`seg-${segKey++}`}>
          {parseUrls(content.slice(i))}
        </Fragment>,
      );
      break;
    }
    if (found > i) {
      out.push(
        <Fragment key={`seg-${segKey++}`}>
          {parseUrls(content.slice(i, found))}
        </Fragment>,
      );
    }
    const matchEnd = found + keyword.length;
    out.push(
      <mark
        key={`mark-${segKey++}`}
        style={{
          background: "var(--pet-tint-yellow-bg)",
          color: "var(--pet-tint-yellow-fg)",
          padding: "0 1px",
          borderRadius: 2,
        }}
      >
        {content.slice(found, matchEnd)}
      </mark>,
    );
    i = matchEnd;
  }
  return out;
}

export type AssistantReaction = "liked" | "disliked" | "puzzled";

export function CopyableMessage({
  role,
  content,
  itemIdx,
  copied,
  onCopy,
  wrapperStyle,
  images,
  highlightKeyword,
  reaction,
  onReact,
  taskRefMap,
  onRefDoubleClick,
  marked,
  onToggleMark,
}: {
  role: "user" | "assistant";
  content: string;
  itemIdx: number;
  copied: boolean;
  onCopy: (idx: number, text: string, asMarkdown: boolean, withMeta: boolean) => void;
  wrapperStyle: React.CSSProperties;
  /// 多模态用户消息附带的图片 data URL；undefined / 空数组都按"无图"处理。
  images?: string[];
  /// 跨会话搜索 hit 命中后，传入 keyword 让 content 中所有匹配段 mark 高亮。
  /// undefined / 空 → 正常 parseUrls 渲染。
  highlightKeyword?: string;
  /// assistant 消息的反馈状态。null / undefined = 未反馈；非空 = 当前选中
  /// 的 reaction。仅 role=assistant 时使用。
  reaction?: AssistantReaction | null;
  /// 反馈点击 callback。三键互斥（连点同一按钮 = 切换 off / 切换到其它键
  /// 直接覆盖）。仅 role=assistant 时挂；user / tool 行不渲染按钮组。
  onReact?: (idx: number, kind: AssistantReaction, content: string) => void;
  /// task title → 当前 status + updated_at 映射。正文里的 `「title」`
  /// （⌘K picker 插入的引用 token）渲成 hover-able underline，让用户在
  /// 老消息里 hover 就能知道这条 ref 还是不是"活"任务。空 / undefined →
  /// 退到无任务感知的 parseUrls 路径。
  taskRefMap?: Record<string, { status: string; updated_at: string }>;
  /// 双击 ref token 的 callback。命中时 cursor 从 help 变 pointer + native
  /// tooltip 附加"双击跳到任务面板"提示。未传 → ref 仅 hover 显信息无双击
  /// 语义。
  onRefDoubleClick?: (title: string) => void;
  /// 该消息是否被用户 📌 标记。filled 黄色 pin vs outline。未传 → 不渲染
  /// pin 按钮（如 tool / error 消息没有 mark 语义）。
  marked?: boolean;
  /// pin toggle callback。点击切换 marked 状态。
  onToggleMark?: () => void;
}) {
  const button = (
    <button
      type="button"
      className="pet-copy-btn"
      title={
        copied
          ? "已复制到剪贴板"
          : "复制这条消息（去掉「」引用装饰）。⌥/Alt 点击保留原始 markdown（含「ref」token）；⇧/Shift 点击前缀 [session title · 时间戳] 元数据（外部归档 / share 用）。"
      }
      onClick={(e) => onCopy(itemIdx, content, e.altKey, e.shiftKey)}
      style={{
        alignSelf: "flex-end",
        padding: "2px 6px",
        fontSize: "10px",
        lineHeight: 1.2,
        border: "1px solid var(--pet-color-border)",
        borderRadius: 4,
        background: "var(--pet-color-card)",
        color: copied ? "var(--pet-tint-green-fg)" : "var(--pet-color-muted)",
        cursor: "pointer",
        whiteSpace: "nowrap",
        flexShrink: 0,
        opacity: copied ? 1 : undefined, // copied 状态强制可见，覆盖 CSS hover-only
      }}
    >
      {copied ? "已复制" : "复制"}
    </button>
  );
  /// 📌 标记按钮：与复制 / reaction 共用 .pet-copy-btn hover-only 显隐。
  /// marked=true 时强制可见（与 copied 同语义 — 表达持久态）+ 黄底深字。
  /// 未传 onToggleMark → 整个按钮不渲染（如 tool / error 消息无 mark 语义）。
  const markButton =
    onToggleMark !== undefined ? (
      <button
        type="button"
        className="pet-copy-btn"
        onClick={onToggleMark}
        title={
          marked
            ? "取消标记（从 localStorage 收藏集移除）"
            : "标记此消息（localStorage 收藏，跨重启保留；后续会有'全部标记'查看页）"
        }
        aria-label={marked ? "unmark message" : "mark message"}
        style={{
          alignSelf: "flex-end",
          padding: "2px 6px",
          fontSize: "10px",
          lineHeight: 1.2,
          border: `1px solid ${marked ? "var(--pet-tint-yellow-fg)" : "var(--pet-color-border)"}`,
          borderRadius: 4,
          background: marked ? "var(--pet-tint-yellow-bg)" : "var(--pet-color-card)",
          color: marked ? "var(--pet-tint-yellow-fg)" : "var(--pet-color-muted)",
          cursor: "pointer",
          whiteSpace: "nowrap",
          flexShrink: 0,
          opacity: marked ? 1 : undefined,
          fontWeight: marked ? 600 : 400,
        }}
      >
        📌
      </button>
    ) : null;
  /// assistant 三键 reaction（👍/👎/🤔）。共用 .pet-copy-btn 的 hover-only 显
  /// 隐 + 行 hover 提示模式；selected 状态强制 opacity=1 + 标志色 + 实底。
  const reactionRow =
    role === "assistant" && onReact ? (
      <div
        style={{
          display: "flex",
          alignSelf: "flex-end",
          gap: 2,
          flexShrink: 0,
        }}
      >
        {(
          [
            { kind: "liked" as const, glyph: "👍", title: "赞同（写 Liked 到 feedback_history）", color: "var(--pet-tint-green-fg)", bg: "var(--pet-tint-green-bg)" },
            { kind: "puzzled" as const, glyph: "🤔", title: "没看懂 / 没说清（写 Puzzled）", color: "var(--pet-tint-yellow-fg)", bg: "var(--pet-tint-yellow-bg)" },
            { kind: "disliked" as const, glyph: "👎", title: "对这条不满意（写 Dismissed）", color: "var(--pet-tint-red-fg)", bg: "var(--pet-tint-red-bg)" },
          ]
        ).map(({ kind, glyph, title, color, bg }) => {
          const active = reaction === kind;
          return (
            <button
              key={kind}
              type="button"
              className="pet-copy-btn"
              title={active ? `${title} · 已选` : title}
              onClick={() => onReact(itemIdx, kind, content)}
              style={{
                padding: "2px 5px",
                fontSize: "11px",
                lineHeight: 1.2,
                border: "1px solid var(--pet-color-border)",
                borderRadius: 4,
                background: active ? bg : "var(--pet-color-card)",
                color: active ? color : "var(--pet-color-muted)",
                cursor: "pointer",
                whiteSpace: "nowrap",
                opacity: active ? 1 : undefined,
                fontWeight: active ? 600 : 400,
              }}
              aria-label={`react ${kind}`}
            >
              {glyph}
            </button>
          );
        })}
      </div>
    ) : null;
  // 仅识别 URL（不启用完整 markdown），避免历史里的散乱 `*` / `-` 误渲染。
  // 桌面气泡同 url 化路径但走 parseMarkdown 完整版（气泡是即时一句，无历史
  // 风险）。
  const hasImages = !!images && images.length > 0;
  const [lightboxSrc, setLightboxSrc] = useState<string | null>(null);
  /// 长消息中段折叠：> 1000 字默认显前 500 字 + 中段省略标记 + 末 300 字，
  /// 点击底部按钮展开全部。搜索模式（highlightKeyword）下不折叠 —— 命中段
  /// 落在中段会被藏，违反搜索语义。每个 message 独立 state，跨重启 / 切
  /// session 自然回到折叠（消息历史里长输出 reset 阅读基线）。
  const [middleExpanded, setMiddleExpanded] = useState(false);
  const LONG_LIMIT = 1000;
  const HEAD_KEEP = 500;
  const TAIL_KEEP = 300;
  const isLong = content.length > LONG_LIMIT;
  const foldMiddle = isLong && !middleExpanded && !highlightKeyword;
  // 单段文本走与原 bubble 一致的渲染分发（keyword / ref / urls 三档）。
  // foldMiddle 路径下分三段渲（head + 点击展开中段标记 + tail），让中段
  // 文本本身就是 affordance —— 用户不必跳到下方 button 才能触达。
  const renderSegment = (text: string) => {
    if (!text) return null;
    if (highlightKeyword) {
      return renderContentWithKeyword(text, highlightKeyword);
    }
    if (taskRefMap && Object.keys(taskRefMap).length > 0) {
      return renderContentWithTaskRefs(text, taskRefMap, onRefDoubleClick);
    }
    return parseUrls(text);
  };
  const hiddenCount = content.length - HEAD_KEEP - TAIL_KEEP;
  const bubble = (
    <div className="pet-chat-bubble" data-role={role} style={bubbleStyle(role)}>
      {hasImages && (
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: 6,
            marginBottom: content ? 6 : 0,
          }}
        >
          {images!.map((src, i) => (
            <ImageThumb key={i} src={src} onOpen={() => setLightboxSrc(src)} />
          ))}
          <ImageLightbox src={lightboxSrc} onClose={() => setLightboxSrc(null)} />
        </div>
      )}
      {content && (foldMiddle ? (
        <>
          {renderSegment(content.slice(0, HEAD_KEEP))}
          {"\n\n"}
          <span
            onClick={() => setMiddleExpanded(true)}
            title={(() => {
              // 中段 preview：从折叠区抽首 20 / 末 20 字，让用户 hover 时
              // 看到"折掉的开头 / 结尾"判断是否值得展开。中段段长不到 40
              // 字直接显完整。换行换成 ⏎ 让 tooltip 单行紧凑。
              const middle = content.slice(HEAD_KEEP, content.length - TAIL_KEEP);
              const flat = middle.replace(/\n/g, " ⏎ ");
              const preview =
                flat.length <= 40
                  ? flat
                  : `${flat.slice(0, 20)} … ${flat.slice(-20)}`;
              return `折叠中段 ${hiddenCount} 字\n\n中段首末预览：\n${preview}\n\n点此展开剩余字数（也可用下方按钮）`;
            })()}
            style={{
              cursor: "pointer",
              color: "var(--pet-color-accent)",
              textDecoration: "underline",
              textDecorationStyle: "dotted",
              textUnderlineOffset: 2,
            }}
          >
            …〔折叠中段 {hiddenCount} 字 · 点此展开〕…
          </span>
          {"\n\n"}
          {renderSegment(content.slice(-TAIL_KEEP))}
        </>
      ) : (
        renderSegment(content)
      ))}
      {isLong && !highlightKeyword && (
        <button
          type="button"
          onClick={() => setMiddleExpanded((v) => !v)}
          style={{
            marginTop: 6,
            fontSize: 11,
            padding: "2px 8px",
            border: "1px solid var(--pet-color-border)",
            borderRadius: 4,
            background: "transparent",
            color: "var(--pet-color-accent)",
            cursor: "pointer",
            fontFamily: "inherit",
            alignSelf: "flex-start",
          }}
          title={
            middleExpanded
              ? "折回前 500 字 + 末 300 字（中段隐藏）"
              : `当前折叠中段 ${content.length - HEAD_KEEP - TAIL_KEEP} 字，点击展开全部 ${content.length} 字`
          }
        >
          {middleExpanded
            ? `↕ 折回中段 (${content.length} 字)`
            : `↕ 展开全部 (${content.length} 字)`}
        </button>
      )}
    </div>
  );
  return (
    <div className="pet-chat-row" data-item-idx={itemIdx} style={wrapperStyle}>
      {/* user 右对齐 → 按钮在 bubble 左侧；assistant 左对齐 → 按钮在 bubble 右侧。
          assistant 行额外挂三键 reaction 在复制按钮内侧（让 reaction 离 bubble
          更近，复制按钮在最外侧）。 */}
      {role === "user" ? (
        <div style={{ display: "flex", alignItems: "flex-end", gap: 6 }}>
          {markButton}
          {button}
          {bubble}
        </div>
      ) : (
        <div style={{ display: "flex", alignItems: "flex-end", gap: 6 }}>
          {bubble}
          {reactionRow}
          {button}
          {markButton}
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
          <mark style={{ background: "var(--pet-tint-yellow-bg)", color: "var(--pet-tint-yellow-fg)", padding: "0 1px", borderRadius: 2 }}>
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
