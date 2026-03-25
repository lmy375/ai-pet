import { useState } from "react";

interface Props {
  name: string;
  arguments: string;
  result?: string;
  isRunning?: boolean;
}

export function ToolCallBlock({ name, arguments: args, result, isRunning }: Props) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div
      style={{
        margin: "4px 0",
        borderRadius: "8px",
        border: "1px solid #e2e8f0",
        background: "#f8fafc",
        fontSize: "13px",
        overflow: "hidden",
      }}
    >
      {/* Header - always visible */}
      <div
        onClick={() => setExpanded(!expanded)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: "6px",
          padding: "8px 12px",
          cursor: "pointer",
          userSelect: "none",
          color: "#475569",
        }}
      >
        <span style={{ fontSize: "11px", transition: "transform 0.2s", transform: expanded ? "rotate(90deg)" : "rotate(0)" }}>
          ▶
        </span>
        <span style={{ fontSize: "12px" }}>
          {isRunning ? "⏳" : result ? "✅" : "🔧"}
        </span>
        <span style={{ fontWeight: 600, color: "#0ea5e9" }}>{name}</span>
        {isRunning && (
          <span style={{ color: "#94a3b8", fontSize: "12px" }}>执行中...</span>
        )}
      </div>

      {/* Details - collapsible */}
      {expanded && (
        <div style={{ borderTop: "1px solid #e2e8f0" }}>
          {/* Arguments */}
          <div style={{ padding: "8px 12px" }}>
            <div style={{ fontSize: "11px", color: "#94a3b8", marginBottom: "4px", fontWeight: 600 }}>
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
              <div style={{ fontSize: "11px", color: "#94a3b8", marginBottom: "4px", fontWeight: 600 }}>
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
