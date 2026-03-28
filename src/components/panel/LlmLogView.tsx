import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

interface LlmLogEntry {
  round: number;
  request_time: string;
  first_token_time: string | null;
  done_time: string;
  first_token_latency_ms: number | null;
  total_latency_ms: number;
  request: {
    model: string;
    messages: Array<{ role: string; content: unknown }>;
    tools?: unknown[];
  };
  response: {
    text: string;
    tool_calls: Array<{ function: { name: string; arguments: string } }>;
  };
}

export function LlmLogView() {
  const [entries, setEntries] = useState<LlmLogEntry[]>([]);
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const fetchLogs = async () => {
    try {
      const lines = await invoke<string[]>("get_llm_logs", { limit: 200 });
      const parsed = lines
        .map((l) => {
          try {
            const raw = JSON.parse(l);
            // Normalize old format: "ts" -> "request_time"
            if (!raw.request_time && raw.ts) raw.request_time = raw.ts;
            if (!raw.done_time) raw.done_time = raw.request_time ?? "";
            if (raw.first_token_latency_ms == null) raw.first_token_latency_ms = null;
            if (raw.total_latency_ms == null) raw.total_latency_ms = 0;
            return raw as LlmLogEntry;
          } catch { return null; }
        })
        .filter((e): e is LlmLogEntry => e !== null && !!e.request_time);
      setEntries(parsed);
    } catch (e) {
      console.error("Failed to fetch LLM logs:", e);
    }
  };

  useEffect(() => {
    fetchLogs();
    const timer = setInterval(fetchLogs, 2000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [entries.length]);

  const formatTime = (ts: string | undefined | null) => {
    if (!ts) return "—";
    const t = ts.split("T")[1];
    return t ? t.slice(0, 8) : ts;
  };

  const lastUserMsg = (entry: LlmLogEntry): string => {
    const msgs = entry.request.messages;
    for (let i = msgs.length - 1; i >= 0; i--) {
      if (msgs[i].role === "user") {
        const c = msgs[i].content;
        const text = typeof c === "string" ? c : JSON.stringify(c);
        return text.length > 80 ? text.slice(0, 80) + "..." : text;
      }
    }
    return "(no user message)";
  };

  const toolCallNames = (entry: LlmLogEntry): string[] =>
    entry.response.tool_calls.map((tc) => tc.function?.name).filter(Boolean);

  const toggle = (idx: number) => {
    setExpandedIdx(expandedIdx === idx ? null : idx);
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Toolbar */}
      <div style={{ display: "flex", gap: "8px", padding: "10px 16px", borderBottom: "1px solid #e2e8f0", background: "#fff" }}>
        <button onClick={fetchLogs} style={toolBtnStyle}>刷新</button>
        <span style={{ flex: 1 }} />
        <span style={{ fontSize: "12px", color: "#94a3b8", alignSelf: "center" }}>
          {entries.length} 条记录
        </span>
      </div>

      {/* Log entries */}
      <div ref={scrollRef} style={{ flex: 1, overflowY: "auto", padding: "8px 12px", background: "#f8fafc" }}>
        {entries.length === 0 ? (
          <div style={{ color: "#94a3b8", textAlign: "center", marginTop: "40px", fontSize: "13px" }}>
            暂无 LLM 日志。发送聊天消息后会产生记录。
          </div>
        ) : (
          entries.map((entry, i) => {
            const isExpanded = expandedIdx === i;
            const tcNames = toolCallNames(entry);
            return (
              <div key={i} style={{ marginBottom: "6px", borderRadius: "8px", border: "1px solid #e2e8f0", background: "#fff", overflow: "hidden" }}>
                {/* Summary row */}
                <div
                  onClick={() => toggle(i)}
                  style={{
                    display: "flex", alignItems: "center", gap: "10px",
                    padding: "10px 14px", cursor: "pointer",
                    userSelect: "none",
                  }}
                >
                  <span style={{ color: "#94a3b8", fontSize: "11px", fontFamily: "monospace", whiteSpace: "nowrap" }}>
                    {formatTime(entry.request_time)}
                  </span>
                  <span style={badgeStyle("#e0f2fe", "#0284c7")}>{entry.request.model}</span>
                  <span style={badgeStyle("#f0fdf4", "#16a34a")}>
                    R{entry.round}
                  </span>
                  {entry.first_token_latency_ms != null && (
                    <span style={badgeStyle("#fefce8", "#ca8a04")}>
                      TTFT {entry.first_token_latency_ms}ms
                    </span>
                  )}
                  <span style={badgeStyle("#faf5ff", "#9333ea")}>
                    {entry.total_latency_ms}ms
                  </span>
                  {tcNames.length > 0 && (
                    <span style={badgeStyle("#fff7ed", "#ea580c")}>
                      🔧 {tcNames.join(", ")}
                    </span>
                  )}
                  <span style={{ flex: 1, fontSize: "12px", color: "#475569", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {lastUserMsg(entry)}
                  </span>
                  <span style={{ color: "#94a3b8", fontSize: "12px" }}>{isExpanded ? "▼" : "▶"}</span>
                </div>

                {/* Expanded detail */}
                {isExpanded && (
                  <div style={{ borderTop: "1px solid #f1f5f9", padding: "12px 14px" }}>
                    {/* Timing */}
                    <DetailSection title="⏱ 时间">
                      <Row label="请求时间" value={entry.request_time} />
                      <Row label="首 Token" value={entry.first_token_time ?? "—"} />
                      <Row label="完成时间" value={entry.done_time} />
                      <Row label="首 Token 延迟" value={entry.first_token_latency_ms != null ? `${entry.first_token_latency_ms} ms` : "—"} />
                      <Row label="总耗时" value={`${entry.total_latency_ms} ms`} />
                    </DetailSection>

                    {/* Request messages */}
                    <DetailSection title="📤 请求消息">
                      {entry.request.messages.map((msg, j) => (
                        <div key={j} style={{ marginBottom: "6px" }}>
                          <span style={roleBadge(msg.role)}>{msg.role}</span>
                          <pre style={preStyle}>
                            {typeof msg.content === "string"
                              ? msg.content
                              : JSON.stringify(msg.content, null, 2)}
                          </pre>
                        </div>
                      ))}
                    </DetailSection>

                    {/* Response */}
                    <DetailSection title="📥 响应">
                      {entry.response.text && (
                        <div style={{ marginBottom: "6px" }}>
                          <span style={roleBadge("assistant")}>assistant</span>
                          <pre style={preStyle}>{entry.response.text}</pre>
                        </div>
                      )}
                      {entry.response.tool_calls.length > 0 && (
                        <div>
                          <div style={{ fontSize: "12px", color: "#64748b", marginBottom: "4px" }}>Tool Calls:</div>
                          <pre style={preStyle}>
                            {JSON.stringify(entry.response.tool_calls, null, 2)}
                          </pre>
                        </div>
                      )}
                    </DetailSection>
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

function DetailSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: "12px" }}>
      <div style={{ fontSize: "12px", fontWeight: 600, color: "#334155", marginBottom: "6px" }}>{title}</div>
      {children}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ display: "flex", gap: "8px", fontSize: "12px", marginBottom: "2px", fontFamily: "monospace" }}>
      <span style={{ color: "#94a3b8", minWidth: "100px" }}>{label}</span>
      <span style={{ color: "#1e293b" }}>{value}</span>
    </div>
  );
}

function roleBadge(role: string): React.CSSProperties {
  const colors: Record<string, [string, string]> = {
    system: ["#f0fdf4", "#16a34a"],
    user: ["#e0f2fe", "#0284c7"],
    assistant: ["#faf5ff", "#9333ea"],
    tool: ["#fff7ed", "#ea580c"],
  };
  const [bg, fg] = colors[role] ?? ["#f1f5f9", "#475569"];
  return {
    display: "inline-block",
    padding: "1px 8px",
    borderRadius: "4px",
    background: bg,
    color: fg,
    fontSize: "11px",
    fontWeight: 600,
    marginBottom: "4px",
  };
}

const badgeStyle = (bg: string, color: string): React.CSSProperties => ({
  padding: "1px 8px",
  borderRadius: "4px",
  background: bg,
  color,
  fontSize: "11px",
  fontWeight: 600,
  whiteSpace: "nowrap",
});

const preStyle: React.CSSProperties = {
  margin: "2px 0 0",
  padding: "8px 10px",
  borderRadius: "6px",
  background: "#f8fafc",
  border: "1px solid #f1f5f9",
  fontSize: "12px",
  lineHeight: "1.6",
  fontFamily: "'SF Mono', 'Menlo', 'Monaco', monospace",
  whiteSpace: "pre-wrap",
  wordBreak: "break-all",
  maxHeight: "300px",
  overflowY: "auto",
  color: "#334155",
};

const toolBtnStyle: React.CSSProperties = {
  padding: "6px 14px",
  borderRadius: "6px",
  border: "1px solid #e2e8f0",
  background: "#fff",
  color: "#475569",
  fontSize: "13px",
  cursor: "pointer",
  fontWeight: 500,
};
