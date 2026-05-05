import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PanelSettings } from "./components/panel/PanelSettings";
import { PanelChat } from "./components/panel/PanelChat";
import { PanelMemory } from "./components/panel/PanelMemory";
import { PanelPersona } from "./components/panel/PanelPersona";
import { PanelTasks } from "./components/panel/PanelTasks";
import { applyTheme, getStoredTheme, setStoredTheme, type Theme } from "./theme";

const TABS = ["设置", "聊天", "任务", "记忆", "人格"] as const;
type Tab = (typeof TABS)[number];

/// 「任务」标签头红点徽章的轮询周期。任务过期是分钟级精度，30s 抓得住。
/// 比这更频繁是浪费 IPC，更稀疏会让用户切回标签时仍看到陈旧数字。
const OVERDUE_POLL_MS = 30_000;

export function PanelApp() {
  const [activeTab, setActiveTab] = useState<Tab>("设置");
  const [overdueCount, setOverdueCount] = useState<number>(0);
  // 主题：迭代 1 仅框架级 surface 切换（顶层 bg / tab bar）。组件内部
  // inline color 留给后续迭代按 panel 逐步迁移到 CSS var。启动时从
  // localStorage 读偏好并 apply，避免 light flash。
  const [theme, setTheme] = useState<Theme>(() => {
    const t = getStoredTheme();
    applyTheme(t);
    return t;
  });
  const toggleTheme = () => {
    const next: Theme = theme === "light" ? "dark" : "light";
    applyTheme(next);
    setStoredTheme(next);
    setTheme(next);
  };

  const openDebugWindow = () => {
    invoke("open_debug").catch(console.error);
  };

  const fetchOverdue = useCallback(async () => {
    try {
      const n = await invoke<number>("task_overdue_count");
      setOverdueCount(n);
    } catch (e) {
      console.error("task_overdue_count failed:", e);
    }
  }, []);

  // 启动时拉一次 + 30s 周期 polling。切到「任务」标签也立刻 refetch
  // 一次（让用户在 PanelTasks 里 retry / cancel / 改 due 后切回主面板时
  // 徽章 N 同步），其它标签的操作不会改任务故无需 trigger。
  useEffect(() => {
    fetchOverdue();
    const id = window.setInterval(fetchOverdue, OVERDUE_POLL_MS);
    return () => window.clearInterval(id);
  }, [fetchOverdue]);
  useEffect(() => {
    if (activeTab === "任务") fetchOverdue();
  }, [activeTab, fetchOverdue]);

  return (
    <div style={{ width: "100%", height: "100vh", display: "flex", flexDirection: "column", background: "var(--pet-color-bg)" }}>
      {/* Tab bar */}
      <div style={{ display: "flex", borderBottom: "1px solid var(--pet-color-border)", background: "var(--pet-color-card)", flexShrink: 0 }}>
        {TABS.map((tab) => {
          const showOverdueBadge = tab === "任务" && overdueCount > 0;
          return (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              style={{
                flex: 1,
                padding: "12px 0",
                border: "none",
                borderBottom: activeTab === tab ? "2px solid var(--pet-color-accent)" : "2px solid transparent",
                background: "transparent",
                color: activeTab === tab ? "var(--pet-color-accent)" : "var(--pet-color-muted)",
                fontWeight: activeTab === tab ? 600 : 400,
                fontSize: "14px",
                cursor: "pointer",
                transition: "all 0.2s",
                position: "relative",
              }}
            >
              {tab}
              {showOverdueBadge && (
                <span
                  title={`已过期未完成 ${overdueCount} 条 — 切到「任务」标签查看（仅 pending / error 且 due 已过的计入；done / cancelled 不计）`}
                  style={{
                    position: "absolute",
                    top: "4px",
                    right: "calc(50% - 30px)",
                    minWidth: 16,
                    height: 16,
                    padding: "0 4px",
                    background: "#dc2626",
                    color: "#fff",
                    fontSize: "10px",
                    fontWeight: 700,
                    borderRadius: 8,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    lineHeight: 1,
                    boxShadow: "0 1px 3px rgba(0,0,0,0.2)",
                  }}
                >
                  {overdueCount > 9 ? "9+" : overdueCount}
                </span>
              )}
            </button>
          );
        })}
        <button
          onClick={toggleTheme}
          style={{
            padding: "12px 12px",
            border: "none",
            borderBottom: "2px solid transparent",
            background: "transparent",
            color: "var(--pet-color-muted)",
            fontSize: "14px",
            cursor: "pointer",
            transition: "all 0.2s",
          }}
          title={
            theme === "light"
              ? "切到深色主题（夜间不刺眼）"
              : "切到浅色主题"
          }
        >
          {theme === "light" ? "🌙" : "☀️"}
        </button>
        <button
          onClick={openDebugWindow}
          style={{
            padding: "12px 16px",
            border: "none",
            borderBottom: "2px solid transparent",
            background: "transparent",
            color: "var(--pet-color-muted)",
            fontWeight: 400,
            fontSize: "14px",
            cursor: "pointer",
            transition: "all 0.2s",
          }}
          title="在新窗口中打开调试日志"
        >
          调试 ↗
        </button>
      </div>

      {/* Tab content */}
      <div style={{ flex: 1, overflow: "hidden" }}>
        {activeTab === "设置" && <PanelSettings />}
        {activeTab === "聊天" && <PanelChat onRequestTab={setActiveTab} />}
        {activeTab === "任务" && <PanelTasks />}
        {activeTab === "记忆" && <PanelMemory />}
        {activeTab === "人格" && <PanelPersona />}
      </div>
    </div>
  );
}
