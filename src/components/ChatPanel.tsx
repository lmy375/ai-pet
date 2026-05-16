import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ImageLightbox } from "./common/ImageLightbox";
import { readSentHistory, pushSentHistory } from "./chatHistoryStore";

interface Props {
  onSend: (message: string, images?: string[]) => void;
  isLoading: boolean;
}

const PANEL_STYLES = `
.pet-chat-input:focus {
  border-color: var(--pet-color-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--pet-color-accent) 22%, transparent);
}
`;

/// 桌面输入框 placeholder 轮播文案。第一条保留功能性提示（可粘贴 / 拖入
/// 图片）让新用户初见时能学到能力；后续几条 conversational 风让放置态
/// 输入框少点"待机寡淡感"——与 ChatMini idle fade 一起把"宠物在等你"的
/// 陪伴感做出来。仅在 input 为空且非流式时定时轮换。
///
/// 30 秒一换：长到不打扰阅读 / 思考，短到放置一会儿就能看到下一句。
const CHAT_INPUT_PLACEHOLDERS: string[] = [
  "说点什么…（可粘贴 / 拖入图片）",
  "今天感觉怎么样？",
  "想聊点啥？",
  "需要帮忙做什么？",
  "随便聊聊，我陪着 🐾",
];
const CHAT_INPUT_PLACEHOLDER_ROTATE_MS = 30_000;

/// 桌面宠物输入框。作为 flex column 里的第三段、永远紧贴底部。**不再使用
/// position:absolute** —— 既往多次出现 absolute-bottom 与 ChatMini 重叠的
/// bug，本组件保持普通 flex item，由 App 容器通过 flex column 自然堆叠
/// (Live2D / ChatMini / ChatPanel) 即可。
export function ChatPanel({ onSend, isLoading }: Props) {
  const [input, setInput] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  /// shell 风历史栈（最新在前）。submit 后 push；ArrowUp / ArrowDown 召回。
  const [sentHistory, setSentHistory] = useState<string[]>(readSentHistory);
  /// 当前历史浏览游标（0 = 最新）。null = 不在历史模式，按 ↑↓ 走 textarea
  /// 默认光标移动。用 ref 而非 state：仅影响下一次 keydown / change 判断，
  /// 无需 re-render，且避免 setInput 与 setCursor 在同一帧的时序歧义。
  const historyCursorRef = useRef<number | null>(null);
  /// 上次 recall 写入 input 的值。用来判断"用户是否在召回后又手动编辑"——
  /// onChange 里如果新值 ≠ recalledValueRef，说明用户在改字，跳出历史模式。
  const recalledValueRef = useRef<string | null>(null);
  // 多模态：粘贴 / 拖拽进来的图片 data URL，发送时与文本拼成 multipart。
  const [pendingImages, setPendingImages] = useState<string[]>([]);
  // 拖拽态高亮 + 子元素 enter/leave 抖动防抖计数。
  const [dragActive, setDragActive] = useState(false);
  const dragDepthRef = useRef(0);
  // 守门拒绝（非多模态模型）的 transient 错误文案；3s 自清，避免长期占位。
  const [errorToast, setErrorToast] = useState("");
  const errorToastTimerRef = useRef<number | null>(null);
  // 缩略图点开 lightbox 大图（"发前能看清"）；与 PanelChat 不同 —— 这里 44×44 太
  // 小不挂 hover 📋 复制（已有 ✕ 删除占角）。
  const [lightboxSrc, setLightboxSrc] = useState<string | null>(null);
  /// placeholder 轮播 idx：仅 input 空 + 非 loading 时按 30s 轮换。useState
  /// 让 React 拿到值 re-render textarea 的 placeholder attr。loading / 有
  /// 输入时跳过 setInterval —— 用户在打字 / 流式中 placeholder 看不到，省
  /// re-render。
  const [placeholderIdx, setPlaceholderIdx] = useState(0);
  const inputEmpty = input.length === 0;
  useEffect(() => {
    if (!inputEmpty || isLoading) return;
    const id = window.setInterval(() => {
      setPlaceholderIdx((i) => (i + 1) % CHAT_INPUT_PLACEHOLDERS.length);
    }, CHAT_INPUT_PLACEHOLDER_ROTATE_MS);
    return () => window.clearInterval(id);
  }, [inputEmpty, isLoading]);

  const showErrorToast = useCallback((msg: string) => {
    setErrorToast(msg);
    if (errorToastTimerRef.current !== null) {
      window.clearTimeout(errorToastTimerRef.current);
    }
    errorToastTimerRef.current = window.setTimeout(() => {
      setErrorToast("");
      errorToastTimerRef.current = null;
    }, 3000);
  }, []);

  const ingestImageBlobs = useCallback((blobs: Blob[]) => {
    for (const blob of blobs) {
      const reader = new FileReader();
      reader.onload = () => {
        const url = reader.result;
        if (typeof url === "string") {
          setPendingImages((prev) => [...prev, url]);
        }
      };
      reader.readAsDataURL(blob);
    }
  }, []);

  useEffect(() => {
    const el = textareaRef.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, 80) + "px";
    }
  }, [input]);

  // 监听 App.tsx window-level 拖图事件：用户把图拖到 ChatPanel 外（如
  // Live2D / ChatMini 区）时，App 已经把 FileReader 解出的 data URL 推过
  // 来；这里 push 到 pendingImages 让发送时一并 multipart。与 inner onDrop
  // 互斥（App 那边读 defaultPrevented 守门）。
  useEffect(() => {
    const onExternalDrop = (e: Event) => {
      const ce = e as CustomEvent<string[]>;
      const urls = ce.detail;
      if (!Array.isArray(urls) || urls.length === 0) return;
      setPendingImages((prev) => [...prev, ...urls]);
    };
    window.addEventListener("pet-pending-image-drop", onExternalDrop);
    return () => {
      window.removeEventListener("pet-pending-image-drop", onExternalDrop);
    };
  }, []);

  // ChatMini bubble 上"💭 针对这条问"按钮派发的 CustomEvent。把 excerpt
  // 拼到输入框的前缀，让用户接着敲"...上次说的那个..."有锚点。已有内容
  // 时插到最前（用户敲的字在锚点后），让锚点先入眼。
  useEffect(() => {
    const onRespondTo = (e: Event) => {
      const ce = e as CustomEvent<string>;
      const excerpt = ce.detail;
      if (typeof excerpt !== "string" || !excerpt) return;
      const prefix = `关于「${excerpt}」`;
      setInput((prev) => (prev ? `${prefix} ${prev}` : `${prefix} `));
      // 让 textarea 聚焦 + 光标到末尾，用户可以直接续写问题
      window.setTimeout(() => {
        const el = textareaRef.current;
        if (!el) return;
        el.focus();
        const len = el.value.length;
        try {
          el.setSelectionRange(len, len);
        } catch {
          // ignore
        }
      }, 0);
    };
    window.addEventListener("pet-mini-respond-to", onRespondTo);
    return () => {
      window.removeEventListener("pet-mini-respond-to", onRespondTo);
    };
  }, []);

  // ChatMini 选区浮 toolbar 的"🔄 改写"按钮派发的 CustomEvent。把选中
  // 文字以 `请改写：\n\n[text]` prefill 到输入框，把光标停在末尾让用户能
  // 在发送前修改意图（"请改写得更精炼" / "请翻译" 等）。已有内容时覆盖
  // —— 改写动作语义足够强，prefix-保留旧文会污染请求。
  useEffect(() => {
    const onRewriteSel = (e: Event) => {
      const ce = e as CustomEvent<string>;
      const text = ce.detail;
      if (typeof text !== "string" || !text.trim()) return;
      setInput(`请改写：\n\n${text}`);
      window.setTimeout(() => {
        const el = textareaRef.current;
        if (!el) return;
        el.focus();
        const len = el.value.length;
        try {
          el.setSelectionRange(len, len);
        } catch {
          // ignore
        }
      }, 0);
    };
    window.addEventListener("pet-mini-rewrite-selection", onRewriteSel);
    return () => {
      window.removeEventListener("pet-mini-rewrite-selection", onRewriteSel);
    };
  }, []);

  // ⌘L / Ctrl+L 全局聚焦 textarea。类似 terminal cmd+L 的"快速回输入框"
  // 反射；浏览器 ⌘L 默认是 focus address bar，但 Tauri WKWebView 没地址
  // 栏所以可自由占用。preventDefault 防止偶发默认。
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      if (e.key !== "l" && e.key !== "L") return;
      // 修饰键不能有 alt / shift（避免与未来其它快捷冲突）
      if (e.altKey || e.shiftKey) return;
      e.preventDefault();
      const el = textareaRef.current;
      if (!el) return;
      el.focus();
      // 把光标移到末尾 + 滚到末尾，让用户立即可继续敲（兼容空 input
      // 与已有内容两种情况）
      const len = el.value.length;
      try {
        el.setSelectionRange(len, len);
      } catch {
        // 极端 browser quirk：某些 type 的 input 不支持 setSelectionRange；
        // focus 已生效，光标位置退化到默认即可
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const submit = useCallback(async () => {
    const trimmed = input.trim();
    const hasImages = pendingImages.length > 0;
    if (!hasImages && !trimmed) return;
    if (isLoading) return;
    if (hasImages) {
      // 守门：非多模态模型时拒绝并提示。守门走后端 settings 真值，让用户切
      // model 后立刻生效（不缓存）。
      try {
        const ok = await invoke<boolean>("is_current_model_multimodal");
        if (!ok) {
          showErrorToast(`当前模型不支持图片输入，已忽略 ${pendingImages.length} 张图`);
          setPendingImages([]);
          return;
        }
      } catch (e) {
        showErrorToast(`检测多模态能力失败：${e}`);
        return;
      }
    }
    onSend(trimmed, hasImages ? pendingImages : undefined);
    setPendingImages([]);
    setInput("");
    // shell 风历史栈：dedup + move-to-front + cap，写盘后同步本地 state。
    // 与 PanelChat（大面板聊天框）共用同一份 localStorage，跨窗口召回。
    setSentHistory(pushSentHistory(trimmed));
    historyCursorRef.current = null;
    recalledValueRef.current = null;
  }, [input, isLoading, pendingImages, onSend, showErrorToast]);

  /// ↑/↓ 在合适状态下拦截作历史召回。其余按键交由 textarea 默认。
  /// ↑：仅 `input === ""` 或处于历史浏览模式（recalledValue === 当前 input）时拦截
  /// ↓：仅处于历史浏览模式时拦截
  /// 这样既不打扰多行编辑场景，又给"清空后回溯"留下肌肉记忆通道。
  const handleHistoryNav = (e: React.KeyboardEvent, direction: "up" | "down"): boolean => {
    if (e.shiftKey || e.metaKey || e.ctrlKey || e.altKey) return false;
    const inHistoryMode =
      historyCursorRef.current !== null && recalledValueRef.current === input;
    if (direction === "up") {
      if (input !== "" && !inHistoryMode) return false;
      if (sentHistory.length === 0) return false;
      const next =
        historyCursorRef.current === null
          ? 0
          : Math.min(historyCursorRef.current + 1, sentHistory.length - 1);
      historyCursorRef.current = next;
      const value = sentHistory[next];
      recalledValueRef.current = value;
      setInput(value);
      return true;
    }
    // down
    if (!inHistoryMode) return false;
    const cur = historyCursorRef.current ?? 0;
    const next = cur - 1;
    if (next < 0) {
      historyCursorRef.current = null;
      recalledValueRef.current = null;
      setInput("");
    } else {
      historyCursorRef.current = next;
      const value = sentHistory[next];
      recalledValueRef.current = value;
      setInput(value);
    }
    return true;
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void submit();
      return;
    }
    if (e.key === "ArrowUp" && handleHistoryNav(e, "up")) {
      e.preventDefault();
      return;
    }
    if (e.key === "ArrowDown" && handleHistoryNav(e, "down")) {
      e.preventDefault();
      return;
    }
  };

  return (
    <>
      <style>{PANEL_STYLES}</style>
      <div
        onMouseDown={(e) => e.stopPropagation()}
        onDragEnter={(e) => {
          if (!Array.from(e.dataTransfer.types ?? []).includes("Files")) return;
          e.preventDefault();
          dragDepthRef.current += 1;
          setDragActive(true);
        }}
        onDragOver={(e) => {
          if (!Array.from(e.dataTransfer.types ?? []).includes("Files")) return;
          e.preventDefault();
          e.dataTransfer.dropEffect = "copy";
        }}
        onDragLeave={(e) => {
          if (!Array.from(e.dataTransfer.types ?? []).includes("Files")) return;
          dragDepthRef.current = Math.max(0, dragDepthRef.current - 1);
          if (dragDepthRef.current === 0) setDragActive(false);
        }}
        onDrop={(e) => {
          if (!Array.from(e.dataTransfer.types ?? []).includes("Files")) return;
          e.preventDefault();
          dragDepthRef.current = 0;
          setDragActive(false);
          const files = e.dataTransfer.files;
          if (!files || files.length === 0) return;
          const blobs: Blob[] = [];
          for (let i = 0; i < files.length; i++) {
            const f = files[i];
            if (f.type.startsWith("image/")) blobs.push(f);
          }
          if (blobs.length === 0) return;
          ingestImageBlobs(blobs);
        }}
        style={{
          padding: "8px 12px 12px",
          flexShrink: 0,
          display: "flex",
          flexDirection: "column",
          gap: "6px",
          position: "relative",
        }}
      >
        {dragActive && (
          <div
            style={{
              position: "absolute",
              inset: 0,
              zIndex: 20,
              background: "color-mix(in srgb, var(--pet-color-accent) 22%, transparent)",
              border: "2px dashed var(--pet-color-accent)",
              borderRadius: 12,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              pointerEvents: "none",
              color: "var(--pet-color-accent)",
              fontSize: 12,
              fontWeight: 600,
            }}
          >
            📎 松开把图片加到下一条消息
          </div>
        )}
        {errorToast && (
          <div
            style={{
              fontSize: 11,
              color: "var(--pet-tint-red-fg)",
              background: "var(--pet-tint-red-bg)",
              border: "1px solid var(--pet-tint-red-fg)",
              borderRadius: 8,
              padding: "4px 8px",
              alignSelf: "stretch",
            }}
          >
            {errorToast}
          </div>
        )}
        {pendingImages.length > 0 && (
          <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
            {pendingImages.map((src, i) => (
              <div key={i} style={{ position: "relative" }}>
                <img
                  src={src}
                  alt=""
                  onClick={() => setLightboxSrc(src)}
                  title="点击查看大图"
                  style={{
                    width: 44,
                    height: 44,
                    objectFit: "cover",
                    borderRadius: 4,
                    display: "block",
                    cursor: "zoom-in",
                  }}
                />
                <button
                  type="button"
                  title="移除这张图"
                  aria-label="remove image"
                  onClick={() =>
                    setPendingImages((prev) => prev.filter((_, j) => j !== i))
                  }
                  style={{
                    position: "absolute",
                    top: -5,
                    right: -5,
                    width: 16,
                    height: 16,
                    borderRadius: "50%",
                    border: "none",
                    background: "rgba(15,23,42,0.78)",
                    color: "#fff",
                    fontSize: 10,
                    lineHeight: 1,
                    cursor: "pointer",
                    padding: 0,
                  }}
                >
                  ✕
                </button>
              </div>
            ))}
          </div>
        )}
        <textarea
          ref={textareaRef}
          className="pet-chat-input"
          value={input}
          onChange={(e) => {
            const v = e.target.value;
            setInput(v);
            // 用户手敲编辑（与 recall 写入值不同）→ 跳出历史浏览模式，下次
            // ↑ 从最新一条开始而非接着往后翻。recall 路径走 setInput 不触发
            // onChange，所以这里只在真实键盘输入 / 粘贴时生效。
            if (
              historyCursorRef.current !== null &&
              v !== recalledValueRef.current
            ) {
              historyCursorRef.current = null;
              recalledValueRef.current = null;
            }
          }}
          onKeyDown={handleKeyDown}
          onPaste={(e) => {
            const items = e.clipboardData?.items;
            if (!items) return;
            const blobs: Blob[] = [];
            for (let i = 0; i < items.length; i++) {
              const it = items[i];
              if (it.kind === "file" && it.type.startsWith("image/")) {
                const f = it.getAsFile();
                if (f) blobs.push(f);
              }
            }
            if (blobs.length === 0) return;
            e.preventDefault();
            ingestImageBlobs(blobs);
          }}
          placeholder={
            isLoading
              ? "宠物正在回复中..."
              : CHAT_INPUT_PLACEHOLDERS[placeholderIdx]
          }
          rows={1}
          style={{
            padding: "9px 14px",
            borderRadius: "20px",
            border: "1px solid var(--pet-color-border)",
            background: "var(--pet-color-card)",
            backdropFilter: "blur(8px)",
            fontSize: "14px",
            outline: "none",
            color: "var(--pet-color-fg)",
            resize: "none",
            lineHeight: "1.4",
            fontFamily: "inherit",
            overflow: "hidden",
            boxSizing: "border-box",
            transition: "border-color 150ms ease-out, box-shadow 150ms ease-out",
          }}
        />
      </div>
      <ImageLightbox src={lightboxSrc} onClose={() => setLightboxSrc(null)} />
    </>
  );
}
