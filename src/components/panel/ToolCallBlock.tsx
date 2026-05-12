import { useMemo, useState } from "react";
import { ImageLightbox } from "../common/ImageLightbox";
import { ImageThumb } from "../common/ImageThumb";

interface Props {
  name: string;
  arguments: string;
  result?: string;
  isRunning?: boolean;
}

/// 解析 tool result JSON 里的 `_attachments` 字段（give_image 等约定字段）。
/// 后端 strip_tool_attachments 已经把它从下一轮 LLM 上下文里去掉，但 send_tool_result
/// 给前端时仍带；前端识别后渲染缩略图。非 JSON / 没字段 → 空数组，调用方按"无图"处理。
function extractAttachments(result?: string): string[] {
  if (!result) return [];
  try {
    const v = JSON.parse(result);
    if (v && typeof v === "object" && Array.isArray(v._attachments)) {
      return v._attachments.filter((u: unknown): u is string => typeof u === "string");
    }
  } catch {}
  return [];
}

export function ToolCallBlock({ name, arguments: args, result, isRunning }: Props) {
  const [expanded, setExpanded] = useState(false);
  const [lightboxSrc, setLightboxSrc] = useState<string | null>(null);
  const attachments = useMemo(() => extractAttachments(result), [result]);

  return (
    <div
      style={{
        margin: "4px 0",
        borderRadius: "8px",
        border: "1px solid var(--pet-color-border)",
        background: "var(--pet-color-bg)",
        fontSize: "13px",
        overflow: "hidden",
        boxShadow: "var(--pet-shadow-sm)",
      }}
    >
      {/* Header - always visible */}
      <div
        onClick={() => setExpanded(!expanded)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: "8px",
          padding: "10px 14px",
          cursor: "pointer",
          userSelect: "none",
          color: "var(--pet-color-muted)",
        }}
      >
        <span style={{ fontSize: "11px", transition: "transform 0.2s", transform: expanded ? "rotate(90deg)" : "rotate(0)", display: "inline-block" }}>
          ▶
        </span>
        <span style={{ fontSize: "12px" }}>
          {isRunning ? "⏳" : result ? "✅" : "🔧"}
        </span>
        <span style={{ fontWeight: 600, color: "var(--pet-color-accent)" }}>{name}</span>
        {isRunning && (
          <span style={{ color: "var(--pet-color-muted)", fontSize: "12px" }}>执行中...</span>
        )}
        {!isRunning && attachments.length > 0 && (
          <span style={{ color: "var(--pet-color-muted)", fontSize: "11px", marginLeft: 4 }}>
            ({attachments.length} 张图)
          </span>
        )}
      </div>
      {/* 附件图片：折叠态也显，让 give_image 等工具的产出第一眼可见。
          点 header 仍可展开看完整 args / result（已 strip 掉 base64 的简短版）。 */}
      {attachments.length > 0 && (
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: 6,
            padding: "6px 12px 10px",
          }}
        >
          {attachments.map((src, i) => (
            <ImageThumb key={i} src={src} onOpen={() => setLightboxSrc(src)} />
          ))}
          <ImageLightbox src={lightboxSrc} onClose={() => setLightboxSrc(null)} />
        </div>
      )}

      {/* Details - collapsible */}
      {expanded && (
        <div style={{ borderTop: "1px solid var(--pet-color-border)" }}>
          {/* Arguments */}
          <div style={{ padding: "8px 12px" }}>
            <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginBottom: "4px", fontWeight: 600, letterSpacing: 0.2 }}>
              参数
            </div>
            <pre
              style={{
                margin: 0,
                padding: "8px",
                background: "#1e293b",
                color: "#e2e8f0",
                borderRadius: "6px",
                fontSize: "12px",
                lineHeight: "1.5",
                maxHeight: "200px",
                overflowY: "auto",
                whiteSpace: "pre-wrap",
                wordBreak: "break-all",
              }}
            >
              {formatJson(args)}
            </pre>
          </div>

          {/* Result */}
          {result && (
            <div style={{ padding: "0 12px 8px" }}>
              <div style={{ fontSize: "11px", color: "var(--pet-color-muted)", marginBottom: "4px", fontWeight: 600, letterSpacing: 0.2 }}>
                返回值
              </div>
              <pre
                style={{
                  margin: 0,
                  padding: "8px",
                  background: "#1e293b",
                  color: "#a7f3d0",
                  borderRadius: "6px",
                  fontSize: "12px",
                  lineHeight: "1.5",
                  maxHeight: "300px",
                  overflowY: "auto",
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-all",
                }}
              >
                {formatJson(result)}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function formatJson(str: string): string {
  try {
    return JSON.stringify(JSON.parse(str), null, 2);
  } catch {
    return str;
  }
}
