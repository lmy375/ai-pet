import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function PanelDebug() {
  const [logs, setLogs] = useState<string[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchLogs = async () => {
    try {
      const result = await invoke<string[]>("get_logs");
      setLogs(result);
    } catch (e) {
      console.error("Failed to fetch logs:", e);
    }
  };

  useEffect(() => {
    fetchLogs();
    intervalRef.current = setInterval(fetchLogs, 1000);
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, []);

  // Auto-scroll
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs]);

  const handleClear = async () => {
    await invoke("clear_logs");
    setLogs([]);
  };

  const handleOpenDevTools = async () => {
    try {
      // Open devtools for the current webview
      const win = getCurrentWindow();
      await (win as any).emit("open-devtools");
      // Use internal API
      await invoke("plugin:webview|internal_toggle_devtools", {});
    } catch {
      // Fallback: try the webview API directly
      try {
        await (getCurrentWindow() as any).openDevtools();
      } catch (e) {
        console.error("Cannot open devtools:", e);
        alert("无法打开 DevTools。请使用右键菜单 → Inspect Element。");
      }
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Toolbar */}
      <div style={{ display: "flex", gap: "8px", padding: "12px 16px", borderBottom: "1px solid #e2e8f0", background: "#fff" }}>
        <button onClick={fetchLogs} style={toolBtnStyle}>刷新</button>
        <button onClick={handleClear} style={toolBtnStyle}>清空</button>
        <button onClick={handleOpenDevTools} style={{ ...toolBtnStyle, background: "#f59e0b", color: "#fff" }}>
          DevTools
        </button>
        <span style={{ flex: 1 }} />
        <span style={{ fontSize: "12px", color: "#94a3b8", alignSelf: "center" }}>
          {logs.length} 条日志
        </span>
      </div>

      {/* Log output */}
      <div
        ref={scrollRef}
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
        {logs.length === 0 ? (
          <div style={{ color: "#64748b", textAlign: "center", marginTop: "40px" }}>
            暂无日志。聊天和操作会产生日志。
          </div>
        ) : (
          logs.map((line, i) => (
            <div key={i} style={{ wordBreak: "break-all" }}>
              <span style={{ color: "#94a3b8" }}>{line.slice(0, 14)}</span>
              <span style={{ color: line.includes("ERROR") ? "#f87171" : line.includes("WARN") ? "#fbbf24" : "#e2e8f0" }}>
                {line.slice(14)}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

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
