import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/// 应用日志独立 tab。从 PanelDebug 抽出，让"应用日志" tab 不再既显
/// stats 又显日志窗，分流后两个 tab 各司其职。本组件自维护轮询 + 过滤
/// + 跟随最新滚动。
///
/// 数据源：`get_logs` Tauri 命令读 LogStore（in-memory ring）。1s 轮询，
/// 与既有 PanelDebug 节奏一致。filter chips（ERROR / WARN / INFO 多选）
/// 默认空 = 显示全部，与既有语义对齐。

const POLL_MS = 1000;
type LogLevel = "ERROR" | "WARN" | "INFO";

const multiSelectChipStyle = (active: boolean, accent: string): React.CSSProperties => ({
  fontSize: 11,
  padding: "2px 8px",
  borderRadius: 10,
  border: `1px solid ${active ? accent : "var(--pet-color-border)"}`,
  background: active ? accent : "var(--pet-color-card)",
  color: active ? "#fff" : "var(--pet-color-fg)",
  cursor: "pointer",
  whiteSpace: "nowrap",
  userSelect: "none",
  fontFamily: "'SF Mono', 'Menlo', monospace",
});

export function PanelDebugLogs() {
  const [logs, setLogs] = useState<string[]>([]);
  const [logLevels, setLogLevels] = useState<Set<LogLevel>>(() => new Set());
  const [followTail, setFollowTail] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);
  /// 🔍 关键字 substring 过滤：case-insensitive 子串命中。与 level
  /// chips 是 AND 关系（叠加），让 owner 在长日志（千行）里精确定位
  /// "task title / error reason / event 名" 等。空 → 不过滤。
  /// 不持久化 — debug 临时操作，关 tab 复位。
  const [keyword, setKeyword] = useState("");

  // 轮询 logs 列表。1s 节奏让"刚发生的事件"几乎实时反映在视图里，对
  // debug 场景体感够新鲜；过频则全是 IPC 噪音。
  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      try {
        const next = await invoke<string[]>("get_logs");
        if (!cancelled) setLogs(next);
      } catch (e) {
        console.error("get_logs failed:", e);
      }
    };
    void tick();
    const id = window.setInterval(tick, POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, []);

  // 跟随最新：仅 followTail=true 时自动滚到底；用户向上读旧 log 时
  // (followTail=false) 不动视口。
  useEffect(() => {
    if (followTail && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs, followTail]);

  const filteredLogs = useMemo(() => {
    const kwLower = keyword.trim().toLowerCase();
    const levelActive = logLevels.size > 0;
    if (!levelActive && kwLower.length === 0) return logs;
    return logs.filter((line) => {
      if (levelActive) {
        const lvl: LogLevel = line.includes("ERROR")
          ? "ERROR"
          : line.includes("WARN")
            ? "WARN"
            : "INFO";
        if (!logLevels.has(lvl)) return false;
      }
      if (kwLower.length > 0 && !line.toLowerCase().includes(kwLower)) {
        return false;
      }
      return true;
    });
  }, [logs, logLevels, keyword]);

  const logLevelCounts = useMemo(() => {
    const counts: Record<LogLevel, number> = { ERROR: 0, WARN: 0, INFO: 0 };
    for (const line of logs) {
      if (line.includes("ERROR")) counts.ERROR += 1;
      else if (line.includes("WARN")) counts.WARN += 1;
      else counts.INFO += 1;
    }
    return counts;
  }, [logs]);

  const handleClear = async () => {
    try {
      await invoke("clear_logs");
      setLogs([]);
    } catch (e) {
      console.error("clear_logs failed:", e);
    }
  };

  /// 📜 把当前可见 logs（应用 level + keyword 过滤后）拼成 markdown
  /// 表复制 — 与 PanelDebug 既有 snapshot（全状态 key:value dump）互补。
  /// snapshot 是"我现在这台机器状态如何"的截面；raw logs 是"最近发生了
  /// 什么"的时间线。粘到 GitHub issue / Notion 都能渲成表格。
  ///
  /// 列：时间（前 14 字符 = MM-DD HH:MM:SS）/ level（ERROR / WARN / INFO） /
  /// 消息（去掉时间前缀剩余文本）。消息内 `|` 转义为 `\|` 防表格断裂。
  /// 复制 filteredLogs（owner 已经做的过滤继续生效） — 比 raw logs 更
  /// 聚焦 owner 想 share 的子集。空 → friendly hint。
  const handleCopyMarkdown = async () => {
    if (filteredLogs.length === 0) {
      console.log("📜 暂无可复制日志");
      return;
    }
    const headerLines: string[] = [
      `## 应用日志 · ${filteredLogs.length} 条${
        filteredLogs.length < logs.length ? ` / ${logs.length} total` : ""
      }`,
    ];
    if (logLevels.size > 0 || keyword.trim().length > 0) {
      const parts: string[] = [];
      if (logLevels.size > 0) {
        parts.push(`level=${Array.from(logLevels).join(",")}`);
      }
      if (keyword.trim().length > 0) {
        parts.push(`kw=「${keyword.trim()}」`);
      }
      headerLines.push(`- filter: ${parts.join(" · ")}`);
    }
    headerLines.push("");
    headerLines.push("| time | level | message |");
    headerLines.push("| --- | --- | --- |");
    for (const line of filteredLogs) {
      const ts = line.slice(0, 14);
      const rest = line.slice(14).trimStart();
      const lvl = rest.startsWith("ERROR")
        ? "ERROR"
        : rest.startsWith("WARN")
          ? "WARN"
          : "INFO";
      // 去 level token 后剩余文本作 message；防 pipe 撑断表格 → 转义
      const msg = rest
        .replace(/^(ERROR|WARN|INFO)\s*/, "")
        .replace(/\|/g, "\\|");
      headerLines.push(`| ${ts} | ${lvl} | ${msg} |`);
    }
    const md = headerLines.join("\n");
    try {
      await navigator.clipboard.writeText(md);
      console.log(`📜 已复制 ${filteredLogs.length} 条日志（markdown 表格式）`);
    } catch (e) {
      console.error("copy raw logs failed:", e);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* 工具栏：刷新（手动 trigger 一次轮询周期外的同步）/ 清空。 */}
      <div
        style={{
          display: "flex",
          gap: 8,
          padding: "10px 16px",
          borderBottom: "1px solid var(--pet-color-border)",
          background: "var(--pet-color-card)",
          flexShrink: 0,
        }}
      >
        <button
          onClick={async () => {
            try {
              const next = await invoke<string[]>("get_logs");
              setLogs(next);
            } catch (e) {
              console.error("get_logs failed:", e);
            }
          }}
          style={{
            padding: "6px 12px",
            borderRadius: 6,
            border: "1px solid var(--pet-color-border)",
            background: "var(--pet-color-bg)",
            color: "var(--pet-color-fg)",
            fontSize: 12,
            cursor: "pointer",
          }}
        >
          刷新
        </button>
        <button
          onClick={handleClear}
          style={{
            padding: "6px 12px",
            borderRadius: 6,
            border: "1px solid var(--pet-color-border)",
            background: "var(--pet-color-bg)",
            color: "var(--pet-color-fg)",
            fontSize: 12,
            cursor: "pointer",
          }}
        >
          清空
        </button>
        <button
          onClick={handleCopyMarkdown}
          disabled={filteredLogs.length === 0}
          title={
            filteredLogs.length === 0
              ? "暂无可复制日志"
              : `把当前可见的 ${filteredLogs.length} 条日志拼成 markdown 表复制到剪贴板。列：时间 / level / 消息。可直接粘到 GitHub issue / Notion 渲成表格。与「📸 抓快照 A」全状态 dump 互补 — 那个是截面，本按钮是时间线。`
          }
          style={{
            padding: "6px 12px",
            borderRadius: 6,
            border: "1px solid var(--pet-color-border)",
            background: "var(--pet-color-bg)",
            color: filteredLogs.length === 0 ? "var(--pet-color-muted)" : "var(--pet-color-fg)",
            fontSize: 12,
            cursor: filteredLogs.length === 0 ? "not-allowed" : "pointer",
            opacity: filteredLogs.length === 0 ? 0.5 : 1,
          }}
        >
          📜 复制 markdown
        </button>
        <span style={{ marginLeft: "auto", alignSelf: "center", fontSize: 12, color: "var(--pet-color-muted)" }}>
          {logs.length} 条日志
        </span>
      </div>

      {/* level chip 过滤行：accent 配色与日志体内 ERROR 红 / WARN 黄 / INFO 灰对应。 */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "6px 16px",
          borderBottom: "1px solid var(--pet-color-border)",
          background: "var(--pet-color-bg)",
          flexWrap: "wrap",
          flexShrink: 0,
        }}
      >
        <span style={{ fontSize: 10, color: "var(--pet-color-muted)" }}>level:</span>
        <button
          type="button"
          onClick={() => setLogLevels(new Set())}
          style={multiSelectChipStyle(logLevels.size === 0, "var(--pet-color-muted)")}
          title="显示全部级别。点击清空多选过滤。"
        >
          全部 {logs.length}
        </button>
        {(["ERROR", "WARN", "INFO"] as const).map((lvl) => {
          const accent = lvl === "ERROR" ? "var(--pet-tint-red-fg)" : lvl === "WARN" ? "#f59e0b" : "var(--pet-color-muted)";
          const active = logLevels.has(lvl);
          return (
            <button
              key={lvl}
              type="button"
              onClick={() => {
                setLogLevels((prev) => {
                  const next = new Set(prev);
                  if (next.has(lvl)) next.delete(lvl);
                  else next.add(lvl);
                  return next;
                });
              }}
              style={multiSelectChipStyle(active, accent)}
              title={
                active
                  ? `再次点击移出过滤集合（当前: ${lvl}）`
                  : `加入到只看的 level 集合（多选）：${lvl}`
              }
            >
              {lvl} {logLevelCounts[lvl]}
            </button>
          );
        })}
        {/* 🔍 keyword substring filter：case-insensitive 子串命中，与
            level chips AND 叠加。Esc 清空。输入框右侧 ✕ 按钮一键清。 */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 2,
            marginLeft: 8,
            position: "relative",
          }}
        >
          <span style={{ fontSize: 10, color: "var(--pet-color-muted)" }}>
            🔍
          </span>
          <input
            type="text"
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                setKeyword("");
              }
            }}
            placeholder="过滤关键字（substring）…"
            style={{
              fontSize: 11,
              padding: "2px 22px 2px 6px",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 4,
              background: "var(--pet-color-card)",
              color: "var(--pet-color-fg)",
              fontFamily: "'SF Mono', 'Menlo', monospace",
              width: 180,
              boxSizing: "border-box",
            }}
          />
          {keyword.length > 0 && (
            <button
              type="button"
              onClick={() => setKeyword("")}
              title="清空关键字（Esc 等价）"
              style={{
                position: "absolute",
                right: 4,
                top: "50%",
                transform: "translateY(-50%)",
                width: 16,
                height: 16,
                padding: 0,
                border: "none",
                background: "transparent",
                color: "var(--pet-color-muted)",
                cursor: "pointer",
                fontSize: 12,
                lineHeight: 1,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              ✕
            </button>
          )}
        </div>
        {(logLevels.size > 0 || keyword.trim().length > 0) && (
          <span
            style={{
              fontSize: 10,
              color: "var(--pet-color-muted)",
              marginLeft: "auto",
              fontFamily: "'SF Mono', 'Menlo', monospace",
            }}
          >
            显示 {filteredLogs.length} / {logs.length}
          </span>
        )}
        <button
          type="button"
          onClick={() => {
            setFollowTail(true);
            const el = scrollRef.current;
            if (el) el.scrollTop = el.scrollHeight;
          }}
          title={
            followTail
              ? "当前跟随最新日志。向上滚读旧 log 时自动脱离。"
              : "已脱离最新（向上滚读旧 log 触发）。点击重新跟随 + 滚到底。"
          }
          style={{
            fontSize: "10px",
            padding: "1px 6px",
            border: "1px solid var(--pet-color-border)",
            borderRadius: 4,
            background: followTail ? "var(--pet-color-card)" : "var(--pet-color-bg)",
            color: followTail ? "var(--pet-color-fg)" : "var(--pet-color-muted)",
            cursor: "pointer",
            marginLeft:
              logLevels.size > 0 || keyword.trim().length > 0 ? 8 : "auto",
            whiteSpace: "nowrap",
          }}
        >
          {followTail ? "📌 跟随最新" : "📌 已脱离"}
        </button>
      </div>

      {/* Log output：黑底 monospace，与系统 Console 风格一致。 */}
      <div
        ref={scrollRef}
        onScroll={() => {
          // 阈值 8px 给浮点偏差 buffer。程序设 scrollTop 也会触发本回调，
          // distFromBottom=0 → setFollowTail(true) 与目标一致。
          const el = scrollRef.current;
          if (!el) return;
          const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
          setFollowTail(distFromBottom <= 8);
        }}
        style={{
          flex: 1,
          overflowY: "auto",
          padding: "12px 16px",
          fontFamily: "'SF Mono', 'Menlo', 'Monaco', monospace",
          fontSize: "12px",
          lineHeight: "1.7",
          background: "#0f172a",
          color: "#e2e8f0",
        }}
      >
        {filteredLogs.length === 0 ? (
          <div style={{ color: "#64748b", textAlign: "center", marginTop: "40px" }}>
            {logs.length === 0
              ? "暂无日志。聊天和操作会产生日志。"
              : keyword.trim().length > 0 && logLevels.size > 0
                ? `「${keyword.trim()}」+ 当前 level 过滤无匹配日志`
                : keyword.trim().length > 0
                  ? `「${keyword.trim()}」无匹配日志`
                  : "当前 level 过滤无匹配日志"}
          </div>
        ) : (
          filteredLogs.map((line, i) => (
            <div key={i} style={{ wordBreak: "break-all" }}>
              <span style={{ color: "#94a3b8" }}>{line.slice(0, 14)}</span>
              <span
                style={{
                  color: line.includes("ERROR") ? "#f87171" : line.includes("WARN") ? "#fbbf24" : "#e2e8f0",
                }}
              >
                {line.slice(14)}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
