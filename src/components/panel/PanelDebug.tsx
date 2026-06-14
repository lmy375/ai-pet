import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../ui/Button";
import { RefreshIcon, TrashIcon } from "../Icons";

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
      await invoke("open_devtools");
    } catch (e) {
      console.error("Cannot open devtools:", e);
      alert("无法打开 DevTools。请使用右键菜单 → Inspect Element。");
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex shrink-0 items-center gap-2 border-b border-slate-200/70 bg-white px-4 py-2.5">
        <Button variant="ghost" size="sm" onClick={fetchLogs}>
          <RefreshIcon className="h-4 w-4" />
          刷新
        </Button>
        <Button variant="ghost" size="sm" onClick={handleClear}>
          <TrashIcon className="h-4 w-4" />
          清空
        </Button>
        <Button size="sm" className="!bg-amber-500 hover:!bg-amber-600" onClick={handleOpenDevTools}>
          DevTools
        </Button>
        <span className="flex-1" />
        <span className="text-[12px] text-slate-400">{logs.length} 条日志</span>
      </div>

      {/* Log output */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto bg-slate-900 px-4 py-3 font-mono text-[12px] leading-[1.7] text-slate-200"
      >
        {logs.length === 0 ? (
          <div className="mt-10 text-center text-slate-500">暂无日志。聊天和操作会产生日志。</div>
        ) : (
          logs.map((line, i) => (
            <div key={i} className="break-all">
              <span className="text-slate-400">{line.slice(0, 14)}</span>
              <span className={line.includes("ERROR") ? "text-red-400" : line.includes("WARN") ? "text-amber-400" : "text-slate-200"}>
                {line.slice(14)}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
