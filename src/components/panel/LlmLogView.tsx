import { useState, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { EmptyState } from "./EmptyState";

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

const LIMIT_STEP = 50;
const LIMIT_INITIAL = 50;

export function LlmLogView() {
  const [entries, setEntries] = useState<LlmLogEntry[]>([]);
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  // 当前拉取窗口大小（后端返 last N 条）。初始 50；点"加载更早"+50。
  // 用 state 让"加载更早"按钮 disabled 态能跟随；用 ref 让 2s polling
  // 能拿到最新值而不必把整个 fetchLogs 包进 useCallback 串依赖。
  const [limit, setLimit] = useState(LIMIT_INITIAL);
  const limitRef = useRef(limit);
  useEffect(() => {
    limitRef.current = limit;
  }, [limit]);
  // 是否已加载到日志文件起点：当返回行数 < limit，说明老日志全在了，
  // "加载更早"按钮 disabled。新写入仍会让 limit 行数追上 ≥ limit；不会
  // 死锁在 atFileStart=true（fetchLogs 会重新评估）。
  const [atFileStart, setAtFileStart] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  // 多 chip 过滤：model / tool 各自一组 Set。空 Set = 不过滤该维度；非空 =
  // 任一命中即通过（OR 语义，与 PanelTasks tag 筛 chip 模式一致）。
  // 客户端 derive，原始 entries 不动 —— polling 仍按 limit 全量重抓，过滤
  // 仅影响渲染层。
  const [modelFilter, setModelFilter] = useState<Set<string>>(new Set());
  const [toolFilter, setToolFilter] = useState<Set<string>>(new Set());
  /// 从 entries 派生 distinct (model, count) 与 (tool, count) 列表，给 chip
  /// 行渲染用。频次降序、字典序升序破平。tool 维度从 response.tool_calls
  /// 全集统计（一条 entry 多 tool 时各计一次）。
  const { distinctModels, distinctTools } = useMemo(() => {
    const m = new Map<string, number>();
    const t = new Map<string, number>();
    for (const e of entries) {
      if (e.request?.model) {
        m.set(e.request.model, (m.get(e.request.model) ?? 0) + 1);
      }
      for (const tc of e.response.tool_calls) {
        const name = tc.function?.name;
        if (name) t.set(name, (t.get(name) ?? 0) + 1);
      }
    }
    const sort = (a: [string, number], b: [string, number]) =>
      b[1] - a[1] || a[0].localeCompare(b[0]);
    return {
      distinctModels: [...m.entries()].sort(sort),
      distinctTools: [...t.entries()].sort(sort),
    };
  }, [entries]);
  /// 过滤后的 entries。两个维度都是 OR-within / AND-between：model 命中
  /// （或未启用 model filter）且 tool 命中（或未启用）才显。
  const visibleEntries = useMemo(() => {
    if (modelFilter.size === 0 && toolFilter.size === 0) return entries;
    return entries.filter((e) => {
      if (modelFilter.size > 0 && !modelFilter.has(e.request?.model ?? "")) {
        return false;
      }
      if (toolFilter.size > 0) {
        const names = e.response.tool_calls
          .map((tc) => tc.function?.name)
          .filter((n): n is string => !!n);
        if (!names.some((n) => toolFilter.has(n))) return false;
      }
      return true;
    });
  }, [entries, modelFilter, toolFilter]);
  const toggleSet = (
    set: Set<string>,
    setter: (s: Set<string>) => void,
    value: string,
  ) => {
    const next = new Set(set);
    if (next.has(value)) next.delete(value);
    else next.add(value);
    setter(next);
  };

  const fetchLogs = async (overrideLimit?: number) => {
    try {
      const curLimit = overrideLimit ?? limitRef.current;
      const lines = await invoke<string[]>("get_llm_logs", { limit: curLimit });
      setAtFileStart(lines.length < curLimit);
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
        .filter((e): e is LlmLogEntry => e !== null && !!e.request_time)
        .reverse();
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

  /// "加载更早"按钮：limit 累加 50 + 立刻重抓一次。loadingMore 防双击。
  /// atFileStart 时按钮 disabled，不会进这里。新 limit 直接传给 fetchLogs
  /// 避免等 limitRef 同步的 race（setState 调度 → effect commit → ref 更
  /// 新之间有一帧空窗）。
  const handleLoadMore = async () => {
    if (loadingMore || atFileStart) return;
    const nextLimit = limit + LIMIT_STEP;
    setLoadingMore(true);
    setLimit(nextLimit);
    try {
      await fetchLogs(nextLimit);
    } finally {
      setLoadingMore(false);
    }
  };

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = 0;
    }
  }, [entries.length]);

  // filter 变化时清掉 expandedIdx：visibleEntries 索引会重排，留着旧 idx
  // 会让一条无关行误显展开态。清干净让用户自己再点开关注的行。
  useEffect(() => {
    setExpandedIdx(null);
  }, [modelFilter, toolFilter]);

  // 启动时拉一次 api_base 供 cURL 复制用。失败兜底 OpenAI 公网域名让命令
  // 仍可拷出（用户改 endpoint 时自己改字符串就行）。
  const [apiBaseForCurl, setApiBaseForCurl] = useState("https://api.openai.com/v1");
  useEffect(() => {
    (async () => {
      try {
        const s = await invoke<{ api_base: string }>("get_settings");
        if (s.api_base && s.api_base.trim()) setApiBaseForCurl(s.api_base.trim());
      } catch (e) {
        console.error("get_settings (for cURL) failed:", e);
      }
    })();
  }, []);

  /// cURL 反馈 toast：刚被点过"复制 cURL"的 entry idx，1.5s 自清。
  const [copiedCurlIdx, setCopiedCurlIdx] = useState<number | null>(null);
  const buildCurlCommand = (entry: LlmLogEntry): string => {
    // 拼 OpenAI 兼容 chat completions request body。messages 原样塞回；tools
    // 也带上（重放时能复现 tool calling 路径）。stream 不带 — 用户在外部
    // 工具 debug 时大多想看一次完整响应；如需 streaming 自己加 --no-buffer。
    const body: {
      model: string;
      messages: Array<{ role: string; content: unknown }>;
      tools?: unknown[];
    } = {
      model: entry.request.model,
      messages: entry.request.messages,
    };
    if (entry.request.tools && entry.request.tools.length > 0) {
      body.tools = entry.request.tools;
    }
    const json = JSON.stringify(body, null, 2);
    // body 在单引号里塞，需要把 body 内的 ' 转 '\'' （bash 标准转义）
    const escaped = json.replace(/'/g, `'\\''`);
    // Authorization 用 env 占位避免泄漏 key（用户拷到终端前 export 即可）
    const base = apiBaseForCurl.replace(/\/$/, "");
    return [
      `curl ${base}/chat/completions \\`,
      `  -H 'Content-Type: application/json' \\`,
      `  -H "Authorization: Bearer $OPENAI_API_KEY" \\`,
      `  -d '${escaped}'`,
    ].join("\n");
  };
  const handleCopyCurl = async (idx: number, entry: LlmLogEntry) => {
    const cmd = buildCurlCommand(entry);
    try {
      await navigator.clipboard.writeText(cmd);
      setCopiedCurlIdx(idx);
      window.setTimeout(
        () => setCopiedCurlIdx((cur) => (cur === idx ? null : cur)),
        1500,
      );
    } catch (e) {
      console.error("copy curl failed:", e);
    }
  };

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
      <div style={{ padding: "10px 16px 6px", borderBottom: "1px solid var(--pet-color-border)", background: "var(--pet-color-card)" }}>
        <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
          <button onClick={() => void fetchLogs()} style={toolBtnStyle}>刷新</button>
          {(modelFilter.size > 0 || toolFilter.size > 0) && (
            <button
              onClick={() => {
                setModelFilter(new Set());
                setToolFilter(new Set());
              }}
              style={{
                ...toolBtnStyle,
                fontSize: 12,
                padding: "4px 10px",
                color: "var(--pet-tint-blue-fg)",
                borderColor: "color-mix(in srgb, var(--pet-tint-blue-fg) 35%, transparent)",
              }}
              title="清掉所有 chip 过滤"
            >
              清过滤
            </button>
          )}
          <span style={{ flex: 1 }} />
          <span
            style={{ fontSize: "12px", color: "var(--pet-color-muted)", alignSelf: "center" }}
            title={`当前窗口 last ${limit} 条；底部按钮可加载更早 ${LIMIT_STEP} 条。`}
          >
            显示 {visibleEntries.length} / 全集 {entries.length} / 窗口 {limit}
            {atFileStart && " · 已到底"}
          </span>
        </div>
        {/* model / tool chip 行：派生自当前 entries 集，频次降序。空集 = 不
            显示对应行避免视觉占位。 */}
        {(distinctModels.length > 0 || distinctTools.length > 0) && (
          <div style={{ marginTop: 6, display: "flex", flexDirection: "column", gap: 4 }}>
            {distinctModels.length > 0 && (
              <div style={{ display: "flex", alignItems: "center", gap: 4, flexWrap: "wrap" }}>
                <span style={{ fontSize: 10, color: "var(--pet-color-muted)", marginRight: 4 }}>model</span>
                {distinctModels.map(([model, count]) => {
                  const active = modelFilter.has(model);
                  return (
                    <button
                      key={model}
                      type="button"
                      onClick={() => toggleSet(modelFilter, setModelFilter, model)}
                      style={{
                        fontSize: 11,
                        padding: "1px 8px",
                        borderRadius: 10,
                        border: "1px solid",
                        borderColor: active ? "var(--pet-tint-blue-fg)" : "var(--pet-color-border)",
                        background: active ? "var(--pet-tint-blue-bg)" : "var(--pet-color-card)",
                        color: active ? "var(--pet-tint-blue-fg)" : "var(--pet-color-muted)",
                        cursor: "pointer",
                        fontWeight: active ? 600 : 400,
                      }}
                      title={active ? `点击取消 model=${model} 过滤` : `点击只看 model=${model}`}
                    >
                      {active ? "✓ " : ""}{model} <span style={{ color: "var(--pet-color-muted)" }}>({count})</span>
                    </button>
                  );
                })}
              </div>
            )}
            {distinctTools.length > 0 && (
              <div style={{ display: "flex", alignItems: "center", gap: 4, flexWrap: "wrap" }}>
                <span style={{ fontSize: 10, color: "var(--pet-color-muted)", marginRight: 4 }}>tool</span>
                {distinctTools.map(([tool, count]) => {
                  const active = toolFilter.has(tool);
                  return (
                    <button
                      key={tool}
                      type="button"
                      onClick={() => toggleSet(toolFilter, setToolFilter, tool)}
                      style={{
                        fontSize: 11,
                        padding: "1px 8px",
                        borderRadius: 10,
                        border: "1px solid",
                        borderColor: active ? "var(--pet-tint-orange-fg)" : "var(--pet-color-border)",
                        background: active ? "var(--pet-tint-orange-bg)" : "var(--pet-color-card)",
                        color: active ? "var(--pet-tint-orange-fg)" : "var(--pet-color-muted)",
                        cursor: "pointer",
                        fontWeight: active ? 600 : 400,
                      }}
                      title={active ? `点击取消 tool=${tool} 过滤` : `点击只看含 tool=${tool} 的 round`}
                    >
                      {active ? "✓ " : ""}🔧 {tool} <span style={{ color: "var(--pet-color-muted)" }}>({count})</span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Log entries */}
      <div ref={scrollRef} style={{ flex: 1, overflowY: "auto", padding: "8px 12px", background: "var(--pet-color-bg)" }}>
        {entries.length === 0 ? (
          <EmptyState
            icon="📜"
            title="暂无 LLM 日志"
            hint="发送聊天消息后会产生记录。"
          />
        ) : visibleEntries.length === 0 ? (
          <EmptyState
            icon="🔍"
            title="当前过滤无命中"
            hint='点上方"清过滤"或扩大窗口（"加载更早"）。'
          />
        ) : (
          visibleEntries.map((entry, i) => {
            const isExpanded = expandedIdx === i;
            const tcNames = toolCallNames(entry);
            return (
              <div key={i} style={{ marginBottom: "6px", borderRadius: "8px", border: "1px solid #e2e8f0", background: "var(--pet-color-card)", overflow: "hidden" }}>
                {/* Summary row */}
                <div
                  onClick={() => toggle(i)}
                  style={{
                    display: "flex", alignItems: "center", gap: "10px",
                    padding: "10px 14px", cursor: "pointer",
                    userSelect: "none",
                  }}
                >
                  <span style={{ color: "var(--pet-color-muted)", fontSize: "11px", fontFamily: "monospace", whiteSpace: "nowrap" }}>
                    {formatTime(entry.request_time)}
                  </span>
                  <span style={badgeStyle("var(--pet-tint-blue-bg)", "var(--pet-tint-blue-fg)")}>{entry.request.model}</span>
                  <span style={badgeStyle("var(--pet-tint-green-bg)", "var(--pet-tint-green-fg)")}>
                    R{entry.round}
                  </span>
                  {entry.first_token_latency_ms != null && (
                    <span style={badgeStyle("var(--pet-tint-yellow-bg)", "var(--pet-tint-yellow-fg)")}>
                      TTFT {entry.first_token_latency_ms}ms
                    </span>
                  )}
                  <span style={badgeStyle("var(--pet-tint-purple-bg)", "var(--pet-tint-purple-fg)")}>
                    {entry.total_latency_ms}ms
                  </span>
                  {tcNames.length > 0 && (
                    <span style={badgeStyle("var(--pet-tint-orange-bg)", "var(--pet-tint-orange-fg)")}>
                      🔧 {tcNames.join(", ")}
                    </span>
                  )}
                  <span style={{ flex: 1, fontSize: "12px", color: "var(--pet-color-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {lastUserMsg(entry)}
                  </span>
                  <span style={{ color: "var(--pet-color-muted)", fontSize: "12px" }}>{isExpanded ? "▼" : "▶"}</span>
                </div>

                {/* Expanded detail */}
                {isExpanded && (
                  <div style={{ borderTop: "1px solid #f1f5f9", padding: "12px 14px" }}>
                    {/* 复制 cURL 按钮：把当前 entry 的 request 拼成 OpenAI
                        chat completions curl 命令到剪贴板。api_key 用
                        $OPENAI_API_KEY 占位避免泄漏；用户拷出去前 export 即可。 */}
                    <div style={{ display: "flex", justifyContent: "flex-end", marginBottom: 10 }}>
                      <button
                        type="button"
                        onClick={() => void handleCopyCurl(i, entry)}
                        style={{
                          ...toolBtnStyle,
                          fontSize: 11,
                          padding: "3px 9px",
                          color: copiedCurlIdx === i ? "var(--pet-tint-green-fg)" : "var(--pet-color-muted)",
                          borderColor: copiedCurlIdx === i ? "color-mix(in srgb, var(--pet-tint-green-fg) 40%, transparent)" : "var(--pet-color-border)",
                        }}
                        title="把 request 转成 OpenAI 兼容 chat completions 的 curl 命令复制到剪贴板。$OPENAI_API_KEY 是 env 占位，拷到终端前先 export。"
                      >
                        {copiedCurlIdx === i ? "✓ 已复制" : "📋 复制 cURL"}
                      </button>
                    </div>
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
                          <div style={{ fontSize: "12px", color: "var(--pet-color-muted)", marginBottom: "4px" }}>Tool Calls:</div>
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
        {/* 底部"加载更早"按钮：list 非空 + 未到文件起点时显。点击 limit
            += 50 后立刻重抓；polling 后续也用新 limit。 */}
        {entries.length > 0 && (
          <div style={{ textAlign: "center", padding: "8px 0 16px" }}>
            {atFileStart ? (
              <span style={{ fontSize: 12, color: "var(--pet-color-muted)" }}>
                · 已加载日志文件起点 ·
              </span>
            ) : (
              <button
                type="button"
                onClick={() => void handleLoadMore()}
                disabled={loadingMore}
                style={{
                  ...toolBtnStyle,
                  cursor: loadingMore ? "default" : "pointer",
                  opacity: loadingMore ? 0.6 : 1,
                }}
                title={`把窗口扩到 last ${limit + LIMIT_STEP} 条；老条目会出现在底部`}
              >
                {loadingMore ? "加载中…" : `加载更早 ${LIMIT_STEP} 条`}
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function DetailSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: "12px" }}>
      <div style={{ fontSize: "12px", fontWeight: 600, color: "var(--pet-color-fg)", marginBottom: "6px" }}>{title}</div>
      {children}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ display: "flex", gap: "8px", fontSize: "12px", marginBottom: "2px", fontFamily: "monospace" }}>
      <span style={{ color: "var(--pet-color-muted)", minWidth: "100px" }}>{label}</span>
      <span style={{ color: "var(--pet-color-fg)" }}>{value}</span>
    </div>
  );
}

function roleBadge(role: string): React.CSSProperties {
  const colors: Record<string, [string, string]> = {
    system: ["var(--pet-tint-green-bg)", "var(--pet-tint-green-fg)"],
    user: ["var(--pet-tint-blue-bg)", "var(--pet-tint-blue-fg)"],
    assistant: ["var(--pet-tint-purple-bg)", "var(--pet-tint-purple-fg)"],
    tool: ["var(--pet-tint-orange-bg)", "var(--pet-tint-orange-fg)"],
  };
  const [bg, fg] = colors[role] ?? ["var(--pet-color-bg)", "var(--pet-color-muted)"];
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
  background: "var(--pet-color-bg)",
  border: "1px solid #f1f5f9",
  fontSize: "12px",
  lineHeight: "1.6",
  fontFamily: "'SF Mono', 'Menlo', 'Monaco', monospace",
  whiteSpace: "pre-wrap",
  wordBreak: "break-all",
  maxHeight: "300px",
  overflowY: "auto",
  color: "var(--pet-color-fg)",
};

const toolBtnStyle: React.CSSProperties = {
  padding: "6px 14px",
  borderRadius: "6px",
  border: "1px solid #e2e8f0",
  background: "var(--pet-color-card)",
  color: "var(--pet-color-muted)",
  fontSize: "13px",
  cursor: "pointer",
  fontWeight: 500,
};
