import { Component, ErrorInfo, ReactNode, useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { PanelSettings } from "./components/panel/PanelSettings";
import { PanelChat } from "./components/panel/PanelChat";
import { PanelMemory } from "./components/panel/PanelMemory";
import { PanelPersona } from "./components/panel/PanelPersona";
import { PanelTasks } from "./components/panel/PanelTasks";
import { KeyboardHelpOverlay } from "./components/panel/KeyboardHelpOverlay";
import { usePollingState } from "./hooks/usePollingState";
import { useTabKeyboardShortcut } from "./hooks/useTabKeyboardShortcut";
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
  /// 跨窗口 deeplink 传过来的 due filter。pet 窗 🔴 逾期 pill 点击时写
  /// localStorage `pet-panel-deeplink`，本组件 mount + storage 事件两路径消费 →
  /// 切到「任务」tab + 推 filter 到 PanelTasks。consume 后由 PanelTasks 回调清空。
  const [pendingDueFilter, setPendingDueFilter] = useState<
    "all" | "today" | "overdue" | "createdToday" | null
  >(null);
  /// 桌面 ChatMini 右键菜单"在 Panel 中定位本条"用：把消息文本片段
  /// （≤ 80 字 excerpt）通过同一 deeplink 通道送来，PanelChat 收到后反向扫
  /// items 找最近 substr 命中，scrollIntoView + 1.5s 高亮。consume 后由
  /// PanelChat 回调清空。
  const [pendingChatMatch, setPendingChatMatch] = useState<string | null>(null);
  /// ChatMini bubble 右键「🔍 search this session」触发：把消息文本片段
  /// （≤ 80 字 keyword）通过 deeplink.chatSearch.keyword 字段送来，
  /// PanelChat 收到后开 search bar + scope=current + 填 query 让 owner
  /// 看本会话所有命中（与 chatMatch 单点定位互补：那个滚到 1 处，本入
  /// 口开搜索循环）。consume 后由 PanelChat 回调清空。
  const [pendingChatSearch, setPendingChatSearch] = useState<string | null>(
    null,
  );
  /// PanelTasks detail 编辑器选段 "🧠 ask LLM about selection" 按钮触发：把
  /// 选段封装成 "关于「<excerpt>」..." 预填到 PanelChat textarea + 切到聊天
  /// tab。PanelChat 内 effect 消费后 setPendingChatPrefill(null)。
  const [pendingChatPrefill, setPendingChatPrefill] = useState<string | null>(
    null,
  );
  /// 桌面 ChatMini "💾 转 task" 按钮 → 跨窗口 deeplink → PanelTasks quickAdd
  /// modal 预填 body。PanelTasks 在 mount 后 effect 读 prop → setBody + setTitle
  /// (default = body 前 30 字) + setQuickAddOpen(true) → 调 onConsume 清空。
  const [pendingQuickAddBody, setPendingQuickAddBody] = useState<string | null>(
    null,
  );
  const requestChatPrefillFromSelection = (text: string) => {
    const trimmed = text.trim();
    if (!trimmed) return;
    // 50 字以内的 selection 直接全文做 prefix；> 50 字截断 + "…" 防 prefix
    // 比正文还长。换行归一空格让 prefix 单行。
    const excerpt = trimmed.replace(/\s+/g, " ").slice(0, 50);
    const ellipsis = trimmed.length > 50 ? "…" : "";
    setPendingChatPrefill(`关于「${excerpt}${ellipsis}」 `);
    setActiveTab("聊天");
  };
  const consumePanelDeeplink = useCallback(() => {
    let raw: string | null = null;
    try {
      raw = localStorage.getItem("pet-panel-deeplink");
    } catch {
      return;
    }
    if (!raw) return;
    try {
      localStorage.removeItem("pet-panel-deeplink");
    } catch {
      // 即便清不掉也不阻塞 —— TTL 守门保证 stale 值不会反复触发
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      return;
    }
    if (!parsed || typeof parsed !== "object") return;
    const p = parsed as {
      tab?: unknown;
      dueFilter?: unknown;
      ts?: unknown;
      chatMatch?: unknown;
      taskFocusTitle?: unknown;
      quickAddBody?: unknown;
    };
    // TTL: 10s 内才认；防过期 deeplink 在用户后续手动打开 panel 时误触发
    if (typeof p.ts !== "number" || Date.now() - p.ts > 10_000) return;
    if (typeof p.tab === "string" && (TABS as readonly string[]).includes(p.tab)) {
      setActiveTab(p.tab as Tab);
    }
    if (
      p.dueFilter === "all" ||
      p.dueFilter === "today" ||
      p.dueFilter === "overdue" ||
      p.dueFilter === "createdToday"
    ) {
      setPendingDueFilter(p.dueFilter);
    }
    // taskFocusTitle：来自 ChatMini 双击「title」ref；走既有 requestFocusTask
    // pipeline（切「任务」tab + setPendingTaskFocusTitle → PanelTasks 消费）。
    // 与 tab/dueFilter 字段同 deeplink 体可并存。
    if (typeof p.taskFocusTitle === "string" && p.taskFocusTitle.trim()) {
      requestFocusTask(p.taskFocusTitle.trim());
    }
    // quickAddBody：来自 ChatMini "💾 转 task" 按钮；推到 PanelTasks 让其
    // 在 mount 时 setBody + setTitle (前 30 字 default) + setQuickAddOpen(true)。
    if (typeof p.quickAddBody === "string" && p.quickAddBody.trim()) {
      setPendingQuickAddBody(p.quickAddBody.trim());
      setActiveTab("任务");
    }
    if (
      p.chatMatch &&
      typeof p.chatMatch === "object" &&
      typeof (p.chatMatch as { excerpt?: unknown }).excerpt === "string"
    ) {
      const excerpt = (p.chatMatch as { excerpt: string }).excerpt.trim();
      if (excerpt) {
        // chat match 隐含切到「聊天」tab —— 即便 caller 没传 tab 字段。
        // 已显式传 tab 的不动（保留 caller 意图）。
        if (typeof p.tab !== "string") setActiveTab("聊天");
        setPendingChatMatch(excerpt);
      }
    }
    // chatSearch：来自 ChatMini 右键菜单「🔍 search this session」。与
    // chatMatch 同 deeplink body 字段并存（caller 二选一即可）。
    const ps = parsed as { chatSearch?: unknown };
    if (
      ps.chatSearch &&
      typeof ps.chatSearch === "object" &&
      typeof (ps.chatSearch as { keyword?: unknown }).keyword === "string"
    ) {
      const keyword = (ps.chatSearch as { keyword: string }).keyword.trim();
      if (keyword) {
        if (typeof p.tab !== "string") setActiveTab("聊天");
        setPendingChatSearch(keyword);
      }
    }
  }, []);
  // 已开 panel：pet 窗 setItem 触发 storage 事件 → 立即消费
  useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key !== "pet-panel-deeplink") return;
      if (!e.newValue) return;
      consumePanelDeeplink();
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, [consumePanelDeeplink]);
  // 未开 panel：open_panel 后 PanelApp 首次 mount → 这里读
  useEffect(() => {
    consumePanelDeeplink();
  }, [consumePanelDeeplink]);
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

  // ⌘1 – ⌘5（含 Ctrl 等价）跳到 N 号 tab —— Chrome / Slack / Linear 都有的
  // 肌肉记忆，与 DebugApp 共享 useTabKeyboardShortcut hook。
  useTabKeyboardShortcut(TABS, setActiveTab);

  // overdueCount 30s 自动轮询走 usePollingState；切到「任务」tab 也立即
  // refetch 一次（用户在 PanelTasks 里 retry / cancel / 改 due 后切回主面板
  // 时徽章 N 同步），其它 tab 的操作不改任务故无需 trigger。
  const { data: overdueCount, refresh: refreshOverdue } = usePollingState(
    () => invoke<number>("task_overdue_count"),
    OVERDUE_POLL_MS,
    0,
  );
  useEffect(() => {
    if (activeTab === "任务") void refreshOverdue();
  }, [activeTab, refreshOverdue]);

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
    <div
      className="pet-panel-root"
      style={{
        width: "100%",
        height: "100vh",
        display: "flex",
        flexDirection: "column",
        background: "var(--pet-color-bg)",
      }}
    >
      {/* Panel-global 视觉抛光。注入 base CSS：字体 smoothing、scrollbar、
          focus ring、tab bar、按钮交互、卡片 / chip / 输入控件 utility class、
          以及面板底层的微弱径向 accent 光晕做视觉锚。各页继续走自己的 inline
          样式；本块只提供"补齐 + opt-in"。 */}
      <style>{`
        html, body, #root {
          font-family: -apple-system, BlinkMacSystemFont, "SF Pro Display",
            "PingFang SC", "Helvetica Neue", "Segoe UI Variable", system-ui, sans-serif;
          -webkit-font-smoothing: antialiased;
          -moz-osx-font-smoothing: grayscale;
          text-rendering: optimizeLegibility;
        }
        /* 面板底层环境光：左上 / 右下两团极淡 accent 光晕给整面板一点"温度",
           不让大片素白(或纯深)显得冷漠。alpha 5-7% 既能感觉到色相,又不抢
           内容；fixed 让滚动时背景不动。 */
        .pet-panel-root::before {
          content: "";
          position: fixed;
          inset: 0;
          pointer-events: none;
          background:
            radial-gradient(circle at 12% 0%,
              color-mix(in srgb, var(--pet-color-accent) 6%, transparent) 0%,
              transparent 38%),
            radial-gradient(circle at 100% 100%,
              color-mix(in srgb, var(--pet-color-accent) 5%, transparent) 0%,
              transparent 42%);
          z-index: 0;
        }
        .pet-panel-root > * { position: relative; z-index: 1; }
        /* 文本选区与 accent 呼应。28% alpha 在 light / dark 都柔和。 */
        ::selection {
          background: color-mix(in srgb, var(--pet-color-accent) 28%, transparent);
        }
        /* placeholder 用 muted token —— 部分浏览器默认偏深灰，跟整体 muted
           节奏不一致。 */
        input::placeholder, textarea::placeholder {
          color: var(--pet-color-muted);
          opacity: 0.7;
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
        /* Tab bar：玻璃感底色（accent ~3% + card 主体）+ 底部微渐 hairline；
           原 borderBottom 1px 仍保布局, 这里靠 background 做层次。 */
        .pet-panel-tabbar {
          background:
            linear-gradient(180deg,
              color-mix(in srgb, var(--pet-color-accent) 4%, var(--pet-color-card)) 0%,
              var(--pet-color-card) 100%);
          backdrop-filter: saturate(140%);
        }
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
        /* active 指示器从"整条 2px 下边线"换成"居中圆角短条 + halo"。
           inline borderBottom 仍占 2px transparent 保布局；::after 用
           position:absolute 浮在底缘做视觉指示。 */
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
          box-shadow: 0 0 10px color-mix(in srgb, var(--pet-color-accent) 60%, transparent);
        }
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
        /* 全局 button：平滑 transition + hover 时 -0.5px 微浮 + 轻投影。 */
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
        /* —— Card utilities ———————————————————————————————————————————
           .pet-card        — 基础卡片（border + sm shadow）
           .pet-card-elev   — 进阶卡片：顶部 accent 极淡渐变 + md shadow,
                              用于 PanelPersona / PanelSettings 的 section
                              （比 .pet-card 更"高级"一档）。
           inline style 已用 background:card 的 div 加 class 后立即多一层精致
           感，不动既有 padding / radius。 */
        .pet-card {
          background: var(--pet-color-card);
          border: 1px solid var(--pet-color-border);
          border-radius: 10px;
          box-shadow: var(--pet-shadow-sm);
        }
        .pet-card-elev {
          position: relative;
          background:
            linear-gradient(180deg,
              color-mix(in srgb, var(--pet-color-accent) 3%, var(--pet-color-card)) 0%,
              var(--pet-color-card) 60%);
          border: 1px solid var(--pet-color-border);
          border-radius: 12px;
          box-shadow: var(--pet-shadow-sm);
          transition: box-shadow 200ms ease-out, border-color 200ms ease-out;
        }
        .pet-card-elev::before {
          content: "";
          position: absolute;
          left: 14px;
          right: 14px;
          top: -1px;
          height: 1px;
          background: linear-gradient(90deg,
            transparent,
            color-mix(in srgb, var(--pet-color-accent) 60%, transparent) 50%,
            transparent);
          opacity: 0.85;
        }
        .pet-card-elev:hover {
          box-shadow: var(--pet-shadow-md);
        }
        /* —— Chip ————————————————————————————————————————————————
           小色块标签；调用方挂 inline color/background 即可（让既有 tint 体系
           生效），class 提供圆角 / padding / 字距统一节奏。 */
        .pet-chip {
          display: inline-flex;
          align-items: center;
          gap: 4px;
          padding: 2px 8px;
          border-radius: 999px;
          font-size: 11px;
          font-weight: 500;
          letter-spacing: 0.2px;
          line-height: 1.4;
          white-space: nowrap;
        }
        /* —— Divider ———————————————————————————————————————————————
           柔和虚线分割，比 1px solid border 更不显沉重。 */
        .pet-divider {
          height: 1px;
          background: linear-gradient(90deg,
            transparent,
            var(--pet-color-border) 15%,
            var(--pet-color-border) 85%,
            transparent);
          border: none;
          margin: 12px 0;
        }
        /* —— Generic list-row hover ————————————————————————————————
           .pet-row-hover 给"列表里能点的行"加一致的交互：hover 微微高亮 +
           accent 边色，避免每个 panel 自己写一段 CSS。 */
        .pet-row-hover {
          transition: background-color 140ms ease-out, border-color 180ms ease-out,
            box-shadow 180ms ease-out;
        }
        .pet-row-hover:hover {
          background: color-mix(in srgb, var(--pet-color-accent) 4%, var(--pet-color-card)) !important;
          border-color: color-mix(in srgb, var(--pet-color-accent) 35%, var(--pet-color-border)) !important;
          box-shadow: var(--pet-shadow-sm);
        }
        /* Reduce motion safety：把所有的 transform / shadow transition 退化 */
        @media (prefers-reduced-motion: reduce) {
          .pet-panel-root *, .pet-panel-tab, button, .pet-row-hover, .pet-card-elev {
            transition: none !important;
            animation: none !important;
          }
        }
      `}</style>
      {/* Tab bar */}
      <div
        className="pet-panel-tabbar"
        style={{
          display: "flex",
          borderBottom: "1px solid var(--pet-color-border)",
          flexShrink: 0,
        }}
      >
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
              pendingChatMatch={pendingChatMatch}
              onConsumePendingChatMatch={() => setPendingChatMatch(null)}
              pendingChatSearch={pendingChatSearch}
              onConsumePendingChatSearch={() => setPendingChatSearch(null)}
              pendingChatPrefill={pendingChatPrefill}
              onConsumePendingChatPrefill={() =>
                setPendingChatPrefill(null)
              }
            />
          )}
          {activeTab === "任务" && (
            <PanelTasks
              pendingFocusTitle={pendingTaskFocusTitle}
              onConsumeFocus={() => setPendingTaskFocusTitle(null)}
              pendingDueFilter={pendingDueFilter}
              onConsumePendingDueFilter={() => setPendingDueFilter(null)}
              onAskLLMAbout={requestChatPrefillFromSelection}
              pendingQuickAddBody={pendingQuickAddBody}
              onConsumePendingQuickAddBody={() =>
                setPendingQuickAddBody(null)
              }
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
