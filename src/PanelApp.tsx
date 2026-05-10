import { Component, ErrorInfo, ReactNode, useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PanelSettings } from "./components/panel/PanelSettings";
import { PanelChat } from "./components/panel/PanelChat";
import { PanelMemory } from "./components/panel/PanelMemory";
import { PanelPersona } from "./components/panel/PanelPersona";
import { PanelTasks } from "./components/panel/PanelTasks";
import { KeyboardHelpOverlay } from "./components/panel/KeyboardHelpOverlay";
import { applyTheme, getStoredTheme, setStoredTheme, type Theme } from "./theme";

/**
 * Tab-level error boundary so一个 tab 渲染异常时不会让整个 panel 变白屏 ——
 * 报错文本直接显示在内容区，便于用户截图反馈 + 开发期定位。reset 把 key
 * 改一下让 React 卸载/重挂错的子树。
 */
class TabErrorBoundary extends Component<
  { children: ReactNode; tabKey: string },
  { error: Error | null; info: ErrorInfo | null }
> {
  state: { error: Error | null; info: ErrorInfo | null } = { error: null, info: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("PanelApp tab boundary caught:", error, info);
    this.setState({ error, info });
  }

  componentDidUpdate(prev: { tabKey: string }) {
    if (prev.tabKey !== this.props.tabKey && this.state.error) {
      this.setState({ error: null, info: null });
    }
  }

  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            padding: 24,
            color: "var(--pet-tint-orange-fg)",
            background: "var(--pet-tint-orange-bg)",
            margin: 16,
            borderRadius: 8,
            fontFamily: "system-ui, sans-serif",
            fontSize: 13,
            lineHeight: 1.6,
            overflow: "auto",
            maxHeight: "100%",
            boxSizing: "border-box",
          }}
        >
          <div style={{ fontWeight: 600, marginBottom: 8 }}>
            「{this.props.tabKey}」页渲染出错
          </div>
          <div style={{ marginBottom: 8 }}>{String(this.state.error.message ?? this.state.error)}</div>
          {this.state.error.stack && (
            <pre
              style={{
                fontSize: 11,
                whiteSpace: "pre-wrap",
                background: "rgba(0,0,0,0.05)",
                padding: 8,
                borderRadius: 4,
                overflowX: "auto",
              }}
            >
              {this.state.error.stack}
            </pre>
          )}
          <button
            type="button"
            onClick={() => this.setState({ error: null, info: null })}
            style={{
              marginTop: 12,
              padding: "6px 12px",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 4,
              background: "var(--pet-color-card)",
              color: "var(--pet-color-fg)",
              cursor: "pointer",
              fontSize: 12,
            }}
          >
            重试渲染
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

const TABS = ["设置", "聊天", "任务", "记忆", "人格"] as const;
type Tab = (typeof TABS)[number];

/// 「任务」标签头红点徽章的轮询周期。任务过期是分钟级精度，30s 抓得住。
/// 比这更频繁是浪费 IPC，更稀疏会让用户切回标签时仍看到陈旧数字。
const OVERDUE_POLL_MS = 30_000;

export function PanelApp() {
  // 从桌面宠物的聊天按钮进面板时直接停在「聊天」tab —— 主入口语义和
  // 按钮 title「打开聊天面板」对齐。其它入口（调试 ↗ / 任务标签外链）
  // 会在自己的处理器里覆盖这个初值。
  const [activeTab, setActiveTab] = useState<Tab>("聊天");
  const [overdueCount, setOverdueCount] = useState<number>(0);
  const [showKeyboardHelp, setShowKeyboardHelp] = useState(false);
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

  // 全局 `?` 唤起键盘快捷键帮助层。tagName 守卫挡掉输入控件 focus 时的
  // ?（用户可能在搜索框里输入 ?）；Shift+/ 也命中（中英键盘 ? 实际是
  // Shift+/）。Esc 由 KeyboardHelpOverlay 自己处理。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "?") return;
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      e.preventDefault();
      setShowKeyboardHelp((v) => !v);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

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
          onClick={() => setShowKeyboardHelp(true)}
          style={{
            padding: "12px 10px",
            border: "none",
            borderBottom: "2px solid transparent",
            background: "transparent",
            color: "var(--pet-color-muted)",
            fontSize: "14px",
            cursor: "pointer",
            transition: "all 0.2s",
          }}
          title="键盘快捷键速查（也可按 ?）"
        >
          ?
        </button>
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

      {/* Tab content. ErrorBoundary 包一层，渲染异常不会让整个 panel 白屏。 */}
      <div style={{ flex: 1, overflow: "hidden" }}>
        <TabErrorBoundary tabKey={activeTab}>
          {activeTab === "设置" && <PanelSettings />}
          {activeTab === "聊天" && <PanelChat onRequestTab={setActiveTab} />}
          {activeTab === "任务" && <PanelTasks />}
          {activeTab === "记忆" && <PanelMemory />}
          {activeTab === "人格" && <PanelPersona />}
        </TabErrorBoundary>
      </div>
      <KeyboardHelpOverlay
        visible={showKeyboardHelp}
        onClose={() => setShowKeyboardHelp(false)}
      />
    </div>
  );
}
