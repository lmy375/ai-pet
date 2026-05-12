import { Component, ErrorInfo, ReactNode, useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { PanelSettings } from "./components/panel/PanelSettings";
import { PanelChat } from "./components/panel/PanelChat";
import { PanelMemory } from "./components/panel/PanelMemory";
import { PanelPersona } from "./components/panel/PanelPersona";
import { PanelTasks } from "./components/panel/PanelTasks";
import { KeyboardHelpOverlay } from "./components/panel/KeyboardHelpOverlay";
import {
  applyTheme,
  getStoredTheme,
  setStoredTheme,
  getStoredAccent,
  setStoredAccent,
  type Accent,
  type Theme,
} from "./theme";

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
  /// PanelChat 双击 `「title」` ref → 请求切到「任务」tab + 让 PanelTasks 把焦点
  /// 落到该 title。state 提到这里是为了跨 tab-switch 的 mount 续传：
  /// PanelTasks 在 activeTab 切换那一刻才挂载，挂载后 useEffect 读 prop 即消费。
  /// 消费后 PanelTasks 调 onConsumeFocus 清空 → 用户再点别处不会被重复滚回。
  const [pendingTaskFocusTitle, setPendingTaskFocusTitle] = useState<string | null>(null);
  const requestFocusTask = (title: string) => {
    setPendingTaskFocusTitle(title);
    setActiveTab("任务");
  };
  const [overdueCount, setOverdueCount] = useState<number>(0);
  const [showKeyboardHelp, setShowKeyboardHelp] = useState(false);
  // 主题：迭代 1 仅框架级 surface 切换（顶层 bg / tab bar）。组件内部
  // inline color 留给后续迭代按 panel 逐步迁移到 CSS var。启动时从
  // localStorage 读偏好并 apply，避免 light flash。
  const [theme, setTheme] = useState<Theme>(() => {
    const t = getStoredTheme();
    applyTheme(t, getStoredAccent());
    return t;
  });
  const toggleTheme = () => {
    const next: Theme = theme === "light" ? "dark" : "light";
    applyTheme(next, getStoredAccent());
    setStoredTheme(next);
    setTheme(next);
    // 跨 webview 广播：让桌面宠物 / 调试窗口也立即切主题。各 window 的
    // listener 自己 setStoredTheme + applyTheme 持久化。
    void emit("theme-change", next);
  };

  // 监听其它 window（如设置页内联 toggle 后续会有）发出的主题切换。本 window
  // 已经是 emit 方时也会收到（Tauri emit 行为）—— 用 next !== theme 守护避
  // 免 setTheme(next) 触发不必要的 re-apply。
  useEffect(() => {
    const pTheme = listen<string>("theme-change", (event) => {
      const next = event.payload === "dark" ? "dark" : "light";
      setTheme((cur) => {
        if (cur === next) return cur;
        applyTheme(next, getStoredAccent());
        setStoredTheme(next);
        return next;
      });
    });
    const pAccent = listen<string>("accent-change", (event) => {
      const valid: Accent[] = ["default", "green", "purple", "orange", "rose"];
      const raw = event.payload as Accent;
      const next = valid.includes(raw) ? raw : "default";
      if (getStoredAccent() === next) return;
      setStoredAccent(next);
      applyTheme(getStoredTheme(), next);
    });
    return () => {
      pTheme.then((un) => un());
      pAccent.then((un) => un());
    };
  }, []);

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
      {/* Panel-global 视觉抛光：注入一层 base CSS 给各 panel 页提供更精致
          的默认（字体 smoothing、scrollbar、focus ring、tab bar hover 暖底、
          按钮 hover 微浮、双层 focus halo、selection 高亮）。各页继续走自己
          的 inline 样式 + 既有 .pet-* 局部规则；这里只补缺省。 */}
      <style>{`
        html, body, #root {
          font-family: -apple-system, BlinkMacSystemFont, "SF Pro Display",
            "PingFang SC", "Helvetica Neue", "Segoe UI Variable", system-ui, sans-serif;
          -webkit-font-smoothing: antialiased;
          -moz-osx-font-smoothing: grayscale;
          text-rendering: optimizeLegibility;
        }
        /* 文本选区与 accent 呼应。22% alpha 在 light / dark 都柔和（浏览器默
           认蓝过饱和、暗黑下尤其刺眼）。 */
        ::selection {
          background: color-mix(in srgb, var(--pet-color-accent) 28%, transparent);
        }
        /* placeholder 用 muted token —— 部分浏览器默认偏深灰，跟整体 muted
           节奏不一致。 */
        input::placeholder, textarea::placeholder {
          color: var(--pet-color-muted);
          opacity: 0.85;
        }
        /* 共享 scrollbar：inactive 更淡，hover 加深；轨道两侧留 2px 空隙，
           不顶到 panel 边缘看着更"贵气"。 */
        .pet-panel-scroll::-webkit-scrollbar,
        div::-webkit-scrollbar {
          width: 10px;
          height: 10px;
        }
        div::-webkit-scrollbar-thumb {
          background: rgba(148, 163, 184, 0.22);
          border: 2px solid transparent;
          background-clip: padding-box;
          border-radius: 8px;
          transition: background 140ms ease-out;
        }
        div::-webkit-scrollbar-thumb:hover {
          background: rgba(148, 163, 184, 0.55);
          background-clip: padding-box;
        }
        div::-webkit-scrollbar-thumb:active {
          background: rgba(100, 116, 139, 0.75);
          background-clip: padding-box;
        }
        div::-webkit-scrollbar-track {
          background: transparent;
        }
        /* input / textarea / select focus：双层 halo（外柔 + 内强）让手感更
           "软"。border 用 !important 覆盖 inline 默认 border 色。 */
        input:focus, textarea:focus, select:focus {
          outline: none;
          border-color: var(--pet-color-accent) !important;
          box-shadow:
            0 0 0 3px color-mix(in srgb, var(--pet-color-accent) 22%, transparent),
            inset 0 0 0 1px color-mix(in srgb, var(--pet-color-accent) 45%, transparent);
        }
        /* Tab bar：active 走 accent 字 + accent 底纹（极浅 12% alpha 暖底）。
           原 borderBottom 2px 仍由 inline 控制 active 指示；hover 用 tint
           blue bg 而非纯 page bg，让"我在指向哪个 tab" 更明显。 */
        .pet-panel-tab {
          transition: color 140ms ease-out, background-color 140ms ease-out,
            border-color 140ms ease-out;
        }
        .pet-panel-tab:hover:not([data-active="true"]) {
          background: color-mix(in srgb, var(--pet-color-accent) 8%, transparent);
          color: var(--pet-color-fg);
        }
        .pet-panel-tab:focus-visible {
          outline: 2px solid var(--pet-color-accent);
          outline-offset: -4px;
        }
        /* 迭代 7：active 指示器从"整条 2px 下边线"换成"居中圆角短条"。
           inline borderBottom 仍占 2px transparent 保布局；::after 用
           position:absolute 浮在底缘做视觉指示，accent halo 暖光呼应迭代
           1 的 shadow token 语言。 */
        .pet-panel-tab[data-active="true"]::after {
          content: "";
          position: absolute;
          left: 50%;
          bottom: -1px;
          transform: translateX(-50%);
          width: 28px;
          height: 3px;
          border-radius: 2px;
          background: var(--pet-color-accent);
          box-shadow: 0 0 8px color-mix(in srgb, var(--pet-color-accent) 50%, transparent);
        }
        /* hover inactive tab 时让 ::after 提前预告：浅短条 + 略短宽度，
           click 后会涨成 active 全长。视觉连续提示"这里能成 active"。 */
        .pet-panel-tab:hover:not([data-active="true"])::after {
          content: "";
          position: absolute;
          left: 50%;
          bottom: -1px;
          transform: translateX(-50%);
          width: 16px;
          height: 3px;
          border-radius: 2px;
          background: color-mix(in srgb, var(--pet-color-accent) 50%, transparent);
        }
        /* 全局 button：平滑 transition + hover 时 -0.5px 微浮 + 轻投影，
           active 按下回落。已有 transition / transform 的 inline style 优
           先级更高自然覆盖，所以仅作用到"裸 inline button"。 */
        button {
          transition: background-color 120ms ease-out, color 120ms ease-out,
            border-color 120ms ease-out, transform 80ms ease-out,
            box-shadow 140ms ease-out, opacity 120ms ease-out;
        }
        button:not(:disabled):not(.pet-panel-tab):hover {
          transform: translateY(-0.5px);
          box-shadow: var(--pet-shadow-sm);
        }
        button:not(:disabled):not(.pet-panel-tab):active {
          transform: translateY(0);
          box-shadow: none;
        }
        button:disabled {
          opacity: 0.55;
          cursor: not-allowed;
        }
        button:focus-visible {
          outline: 2px solid var(--pet-color-accent);
          outline-offset: 2px;
        }
        /* utility class：希望某个容器看起来像"卡片"时挂这个 class。inline
           style 已用 background:card 的 div 加这条就立刻多一层精致感。border
           +shadow 组合，不动既有 padding / radius —— 各调用方自己决定。 */
        .pet-card {
          background: var(--pet-color-card);
          border: 1px solid var(--pet-color-border);
          border-radius: 10px;
          box-shadow: var(--pet-shadow-sm);
        }
      `}</style>
      {/* Tab bar */}
      <div style={{ display: "flex", borderBottom: "1px solid var(--pet-color-border)", background: "var(--pet-color-card)", flexShrink: 0 }}>
        {TABS.map((tab) => {
          const showOverdueBadge = tab === "任务" && overdueCount > 0;
          const isActive = activeTab === tab;
          return (
            <button
              key={tab}
              className="pet-panel-tab"
              data-active={isActive}
              onClick={() => setActiveTab(tab)}
              style={{
                flex: 1,
                padding: "13px 0 11px",
                border: "none",
                // 总是 2px transparent 留位，让 active / hover 切换时垂直布局不抖。
                // 视觉指示器（圆角短条）由 CSS `[data-active="true"]::after` 渲染。
                borderBottom: "2px solid transparent",
                background: "transparent",
                color: isActive ? "var(--pet-color-accent)" : "var(--pet-color-muted)",
                fontWeight: isActive ? 600 : 500,
                fontSize: "13.5px",
                letterSpacing: 0.2,
                cursor: isActive ? "default" : "pointer",
                position: "relative",
              }}
            >
              {tab}
              {showOverdueBadge && (
                <span
                  title={`已过期未完成 ${overdueCount} 条 — 切到「任务」标签查看（仅 pending / error 且 due 已过的计入；done / cancelled 不计）`}
                  style={{
                    position: "absolute",
                    top: "6px",
                    right: "calc(50% - 28px)",
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
                    boxShadow: "0 1px 4px rgba(220, 38, 38, 0.35)",
                  }}
                >
                  {overdueCount > 9 ? "9+" : overdueCount}
                </span>
              )}
            </button>
          );
        })}
        <button
          className="pet-panel-tab"
          onClick={() => setShowKeyboardHelp(true)}
          style={{
            padding: "13px 12px 11px",
            border: "none",
            borderBottom: "2px solid transparent",
            background: "transparent",
            color: "var(--pet-color-muted)",
            fontSize: "13.5px",
            cursor: "pointer",
          }}
          title="键盘快捷键速查（也可按 ?）"
        >
          ?
        </button>
        <button
          className="pet-panel-tab"
          onClick={toggleTheme}
          style={{
            padding: "13px 12px 11px",
            border: "none",
            borderBottom: "2px solid transparent",
            background: "transparent",
            color: "var(--pet-color-muted)",
            fontSize: "13.5px",
            cursor: "pointer",
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
          className="pet-panel-tab"
          onClick={openDebugWindow}
          style={{
            padding: "13px 14px 11px",
            border: "none",
            borderBottom: "2px solid transparent",
            background: "transparent",
            color: "var(--pet-color-muted)",
            fontWeight: 500,
            fontSize: "13px",
            cursor: "pointer",
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
          {activeTab === "聊天" && (
            <PanelChat
              onRequestTab={setActiveTab}
              onRequestFocusTask={requestFocusTask}
            />
          )}
          {activeTab === "任务" && (
            <PanelTasks
              pendingFocusTitle={pendingTaskFocusTitle}
              onConsumeFocus={() => setPendingTaskFocusTitle(null)}
            />
          )}
          {activeTab === "记忆" && (
            <PanelMemory onRequestFocusTask={requestFocusTask} />
          )}
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
