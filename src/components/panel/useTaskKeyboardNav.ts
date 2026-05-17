import { RefObject, useEffect, useRef } from "react";

/**
 * 任务页键盘导航独立 hook（从 PanelTasks 抽出）。统一管理 ⌘F / `/` 聚焦
 * 搜索、`n` 展开新建表单、↑↓ 移动焦点、空格 toggle 选中、Enter 展开详情、
 * Delete / Backspace 触发取消弹层。
 *
 * 用 ref 持最新的依赖（visibleTasks / toggleSelect / handleToggleExpand /
 * handleCancelOpen），让 keydown 监听器只挂一次，避免 visibleTasks 变化
 * 时 re-subscribe 的窗口竞态。所有 setter / ref 由调用方传入，hook 本身
 * 不持有业务状态。
 */
interface TaskItemLike {
  title: string;
  status: "pending" | "done" | "error" | "cancelled";
  /** 是否被 owner 标 `[pinned]`。`p` 单键快捷反转此值。后端缺省 → false（兼容老 session）。 */
  pinned?: boolean;
}

export interface UseTaskKeyboardNavArgs<T extends TaskItemLike> {
  visibleTasks: T[];
  toggleSelect: (title: string) => void;
  handleToggleExpand: (title: string) => Promise<void>;
  handleCancelOpen: (title: string) => void;
  /** d 快捷键：在 pending / error 行触发标 done。其它状态跳过。 */
  handleMarkDone: (title: string) => Promise<void>;
  /** r 快捷键：仅 error 行触发 retry。其它状态跳过。 */
  handleRetry: (title: string) => Promise<void>;
  /** p 快捷键：反转焦点行 pinned。pin 与 status 正交，done / cancelled 也响应（与桌面右键菜单 + bulk pin 同语义）。 */
  handleTogglePinned: (title: string, nextPinned: boolean) => Promise<void>;
  /** ⌘D / Ctrl+D 快捷键：复制焦点行 title 到剪贴板。键盘党 quick-grab，
   * 不必走右键 ctx menu。无焦点（focusedIdx===null）时跳过让默认行为透传。 */
  handleCopyTitle: (title: string) => void;
  /** ⌘R / Ctrl+R 快捷键：立即刷新 task list — 免等 30s tick。owner 想看
   * 后端刚发生的变更（LLM 新建 / 状态切换）即时反映。preventDefault 吃浏
   * 览器默认"刷新页面"行为。 */
  handleReload: () => void;
  /** ⌘/ / Ctrl+/ 快捷键：弹快捷键速查 modal。owner 健忘 / 新用户发现快
   * 捷键入口；跨 input context 工作（与 ⌘F / ⌘K / ⌘R 同放最前），让在
   * 搜索 / 创建表单输入时也能拉起 cheatsheet。 */
  handleShowShortcutHelp: () => void;
  searchInputRef: RefObject<HTMLInputElement | null>;
  titleInputRef: RefObject<HTMLInputElement | null>;
  setCreateFormExpanded: (v: boolean) => void;
  setFocusedIdx: (updater: (prev: number | null) => number | null) => void;
}

export function useTaskKeyboardNav<T extends TaskItemLike>(
  args: UseTaskKeyboardNavArgs<T>,
) {
  const {
    visibleTasks,
    toggleSelect,
    handleToggleExpand,
    handleCancelOpen,
    handleMarkDone,
    handleRetry,
    handleTogglePinned,
    handleCopyTitle,
    handleReload,
    handleShowShortcutHelp,
    searchInputRef,
    titleInputRef,
    setCreateFormExpanded,
    setFocusedIdx,
  } = args;

  const visibleTasksRef = useRef(visibleTasks);
  useEffect(() => {
    visibleTasksRef.current = visibleTasks;
  }, [visibleTasks]);
  const toggleSelectRef = useRef(toggleSelect);
  useEffect(() => {
    toggleSelectRef.current = toggleSelect;
  }, [toggleSelect]);
  const handleToggleExpandRef = useRef(handleToggleExpand);
  useEffect(() => {
    handleToggleExpandRef.current = handleToggleExpand;
  }, [handleToggleExpand]);
  const handleCancelOpenRef = useRef(handleCancelOpen);
  useEffect(() => {
    handleCancelOpenRef.current = handleCancelOpen;
  }, [handleCancelOpen]);
  const handleMarkDoneRef = useRef(handleMarkDone);
  useEffect(() => {
    handleMarkDoneRef.current = handleMarkDone;
  }, [handleMarkDone]);
  const handleRetryRef = useRef(handleRetry);
  useEffect(() => {
    handleRetryRef.current = handleRetry;
  }, [handleRetry]);
  const handleCopyTitleRef = useRef(handleCopyTitle);
  useEffect(() => {
    handleCopyTitleRef.current = handleCopyTitle;
  }, [handleCopyTitle]);
  const handleReloadRef = useRef(handleReload);
  useEffect(() => {
    handleReloadRef.current = handleReload;
  }, [handleReload]);
  const handleShowShortcutHelpRef = useRef(handleShowShortcutHelp);
  useEffect(() => {
    handleShowShortcutHelpRef.current = handleShowShortcutHelp;
  }, [handleShowShortcutHelp]);
  const handleTogglePinnedRef = useRef(handleTogglePinned);
  useEffect(() => {
    handleTogglePinnedRef.current = handleTogglePinned;
  }, [handleTogglePinned]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // ⌘F / Ctrl+F / ⌘K / Ctrl+K 永远聚焦搜索框，不论当前在哪个输入控件 ——
      // 与 mac 浏览器 / Finder / Notion 的"⌘F = 搜索"以及 Slack / Linear /
      // Cursor 的"⌘K = 全局搜索"直觉一致。tagName 守卫**之后**就拦不到 input
      // 内的 ⌘F / ⌘K 了，所以放最前。
      if (
        (e.metaKey || e.ctrlKey) &&
        (e.key.toLowerCase() === "f" || e.key.toLowerCase() === "k")
      ) {
        e.preventDefault();
        const el = searchInputRef.current;
        if (el) {
          el.focus();
          el.select();
        }
        return;
      }
      // ⌘R / Ctrl+R 立即刷新 task list — 免等 30s tick；与 ⌘F / ⌘K 同
      // 跨 input context 工作（owner 在搜索 / 创建表单输入时也想能按 ⌘R
      // 看后端变化）。preventDefault 吃浏览器默认"刷新整页"行为（Tauri
      // webview 通常会真重载导致 panel state 全丢，必须拦）。
      if (
        (e.metaKey || e.ctrlKey) &&
        e.key.toLowerCase() === "r" &&
        !e.shiftKey &&
        !e.altKey
      ) {
        e.preventDefault();
        handleReloadRef.current();
        return;
      }
      // ⌘/ / Ctrl+/ 弹快捷键速查 modal — 新用户发现 + 老用户健忘时一键
      // 看全表。跨 input context 工作（与 ⌘R 同放最前 — cheatsheet 是
      // 通用 affordance 不该被 input focus 限制）。preventDefault 吃浏
      // 览器默认（macOS Safari 可能 Help 菜单聚焦；Tauri webview 无害
      // 但兜底）。
      if (
        (e.metaKey || e.ctrlKey) &&
        e.key === "/" &&
        !e.shiftKey &&
        !e.altKey
      ) {
        e.preventDefault();
        handleShowShortcutHelpRef.current();
        return;
      }
      // 用户在 search / 创建表单 / 取消原因等输入里打字、或 button 聚焦时按
      // Enter 激活按钮，方向键 / 空格 / Enter 都不应被 keydown 监听截获 ——
      // tagName 守卫足够过滤所有交互控件（含 BUTTON 让 Enter 走原生 click）。
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || tag === "BUTTON") return;
      // 单键 "/" 聚焦搜索框 —— 与 GitHub / Linear / VS Code 命令面板直觉
      // 一致。在 tagName 守卫之**后**避免拦截 input 内输入 "/"；只接 plain
      // 单击（无 modifier）让 ⌘/ 等系统快捷键仍能传递。
      if (
        e.key === "/" &&
        !e.metaKey &&
        !e.ctrlKey &&
        !e.altKey &&
        !e.shiftKey
      ) {
        e.preventDefault();
        const el = searchInputRef.current;
        if (el) {
          el.focus();
          el.select();
        }
        return;
      }
      // "n" 快捷键 — 展开创建表单 + focus 标题输入。tagName 守卫已经挡掉
      // INPUT / TEXTAREA / SELECT / BUTTON，这里安全。setTimeout 0 等
      // setCreateFormExpanded(true) 触发的 React commit 完，input 才挂上 ref。
      if (
        e.key === "n" &&
        !e.metaKey &&
        !e.ctrlKey &&
        !e.altKey &&
        !e.shiftKey
      ) {
        e.preventDefault();
        setCreateFormExpanded(true);
        setTimeout(() => {
          const el = titleInputRef.current;
          if (el) {
            el.focus();
            el.select();
          }
        }, 0);
        return;
      }
      const list = visibleTasksRef.current;
      // j / k vim-style 移焦点：与 ↑↓ 同语义。plain key 无 modifier — 与
      // 既有 d / r / p / n 单键 plain-key 集群一致。tagName 守卫已挡 INPUT
      // / TEXTAREA / SELECT / BUTTON 焦点时不触发。
      const isVimDown =
        e.key === "j" &&
        !e.metaKey &&
        !e.ctrlKey &&
        !e.altKey &&
        !e.shiftKey;
      const isVimUp =
        e.key === "k" &&
        !e.metaKey &&
        !e.ctrlKey &&
        !e.altKey &&
        !e.shiftKey;
      if (e.key === "ArrowDown" || isVimDown) {
        if (list.length === 0) return;
        e.preventDefault();
        setFocusedIdx((prev) => (prev === null ? 0 : Math.min(prev + 1, list.length - 1)));
      } else if (e.key === "ArrowUp" || isVimUp) {
        if (list.length === 0) return;
        e.preventDefault();
        setFocusedIdx((prev) => (prev === null ? 0 : Math.max(0, prev - 1)));
      } else if (e.key === "Home") {
        // Home → 跳第一条；与 ↑↓ 不同，focusedIdx === null 时也直接启动焦点
        // （Home/End 语义明确，不像 Enter 容易误触）。
        if (list.length === 0) return;
        e.preventDefault();
        setFocusedIdx(() => 0);
      } else if (e.key === "End") {
        if (list.length === 0) return;
        e.preventDefault();
        setFocusedIdx(() => list.length - 1);
      } else if (e.key === " " || e.code === "Space") {
        // 空格 toggle 当前焦点行的选中。focusedIdx === null 时空格不做事
        // （让用户先 ↑↓ 启动焦点模式）。
        setFocusedIdx((prev) => {
          if (prev === null) return null;
          const item = list[prev];
          if (!item) return prev;
          e.preventDefault();
          toggleSelectRef.current(item.title);
          return prev;
        });
      } else if (e.key === "Enter") {
        // Enter 切换当前焦点行的"展开详情"——与点击行 header 等价。同空格门
        // 槛：focusedIdx === null 时不响应（避免 Enter 在普通页面误触）。
        setFocusedIdx((prev) => {
          if (prev === null) return null;
          const item = list[prev];
          if (!item) return prev;
          e.preventDefault();
          // handleToggleExpand 是 async（涉 invoke），fire-and-forget；与
          // 鼠标 onClick 路径同语义。
          void handleToggleExpandRef.current(item.title);
          return prev;
        });
      } else if (e.key === "Delete" || e.key === "Backspace") {
        // Delete / Backspace 触发既有"取消 reason 输入"内联弹层（等价于点
        // 行内取消按钮）。仅 pending / error 行响应（终态行不响应，cancel
        // 已结束任务无意义）。autoFocus 让焦点立刻跳到 reason 输入框。
        setFocusedIdx((prev) => {
          if (prev === null) return null;
          const item = list[prev];
          if (!item) return prev;
          if (item.status !== "pending" && item.status !== "error") return prev;
          e.preventDefault();
          handleCancelOpenRef.current(item.title);
          return prev;
        });
      } else if (
        e.key === "d" &&
        !e.metaKey &&
        !e.ctrlKey &&
        !e.altKey &&
        !e.shiftKey
      ) {
        // d = 标 done，无需鼠标定位。pending / error 行响应；终态行不动
        // （后端命令也会拒绝，前端守卫一致让快捷键反馈即时）。
        setFocusedIdx((prev) => {
          if (prev === null) return null;
          const item = list[prev];
          if (!item) return prev;
          if (item.status !== "pending" && item.status !== "error") return prev;
          e.preventDefault();
          void handleMarkDoneRef.current(item.title);
          return prev;
        });
      } else if (
        e.key === "r" &&
        !e.metaKey &&
        !e.ctrlKey &&
        !e.altKey &&
        !e.shiftKey
      ) {
        // r = 触发 retry。仅 error 行响应（pending 不需要重试，retry
        // 后端也会拒）。与既有 Delete=取消 reason 同模式 fire-and-forget。
        setFocusedIdx((prev) => {
          if (prev === null) return null;
          const item = list[prev];
          if (!item) return prev;
          if (item.status !== "error") return prev;
          e.preventDefault();
          void handleRetryRef.current(item.title);
          return prev;
        });
      } else if (
        e.key === "p" &&
        !e.metaKey &&
        !e.ctrlKey &&
        !e.altKey &&
        !e.shiftKey
      ) {
        // p = 切换焦点行 pinned（owner 标"关键任务"）。与 d / r 同 fire-and-forget
        // 模式；与桌面右键菜单「📌 钉住 / 📌 取消钉住」对偶。pin 与 status 正交
        // → done / cancelled 行也响应（与既有右键菜单 / bulk pin 同放宽语义）。
        setFocusedIdx((prev) => {
          if (prev === null) return null;
          const item = list[prev];
          if (!item) return prev;
          e.preventDefault();
          void handleTogglePinnedRef.current(item.title, !item.pinned);
          return prev;
        });
      } else if (
        (e.metaKey || e.ctrlKey) &&
        e.key.toLowerCase() === "d" &&
        !e.altKey &&
        !e.shiftKey
      ) {
        // ⌘D / Ctrl+D = 复制焦点行 title 到剪贴板。键盘党 quick-grab —
        // 与右键 ctx menu「📋 复制 raw_description」对偶但更轻量（仅 title）。
        // 仅 focusedIdx 非空时拦截 / preventDefault；无焦点时透传默认行为
        // （macOS 在 webview 里 ⌘D 默认无害）。tagName 守卫之后 = 不在输入
        // 框工作（避免覆盖系统 ⌘D），与 d / r / p 单键行为对齐。
        setFocusedIdx((prev) => {
          if (prev === null) return null;
          const item = list[prev];
          if (!item) return prev;
          e.preventDefault();
          handleCopyTitleRef.current(item.title);
          return prev;
        });
      } else if (
        (e.metaKey || e.ctrlKey) &&
        e.key.toLowerCase() === "e" &&
        !e.altKey &&
        !e.shiftKey
      ) {
        // ⌘E / Ctrl+E = 展开 / 折叠焦点行 detail。与 Enter plain-key 同
        // 语义（既有 Enter 在守卫之后已 toggle expand），但用 modifier 让
        // 键盘党有"明确意图"按键 — 也方便记 ⌘E = Expand。focusedIdx 非
        // 空时拦截；无焦点透传默认行为（macOS ⌘E = Use Selection for
        // Find，webview 内通常无害）。
        setFocusedIdx((prev) => {
          if (prev === null) return null;
          const item = list[prev];
          if (!item) return prev;
          e.preventDefault();
          void handleToggleExpandRef.current(item.title);
          return prev;
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // 故意不依赖 visibleTasks / toggleSelect 等 —— 改用 ref 让监听器只挂
    // 一次，避免每次 visibleTasks 变化都 re-subscribe 的窗口竞态。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // visibleTasks 缩短（搜索 / 批量动作后任务消失）→ clamp focusedIdx 防越界
  useEffect(() => {
    setFocusedIdx((prev) => {
      if (prev === null) return null;
      if (visibleTasks.length === 0) return null;
      if (prev >= visibleTasks.length) return visibleTasks.length - 1;
      return prev;
    });
  }, [visibleTasks.length, setFocusedIdx]);
}
