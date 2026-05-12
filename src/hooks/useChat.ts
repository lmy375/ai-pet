import { useState, useCallback, useEffect, useRef } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { extractText, type MessageContent } from "../utils/messageContent";

interface ChatMessage {
  role: "user" | "assistant" | "system" | "tool";
  /// 多模态：从 PanelChat 保存的 session 里加载时 user message 的 content 可能
  /// 是 OpenAI compatible parts 数组。assistant / system / 实时输入路径仍是
  /// string —— 但联合类型简化跨路径。文本提取走 extractText。
  content: MessageContent;
  /// 消息时间戳（ISO 字符串）。新发的 user / assistant message 都会印一个；
  /// 老 session 加载回来时这个字段可能缺，消费方按 `?` 守护退回"?"显示。
  /// 后端 ChatMessage 没有 deny_unknown_fields，多带这个字段 JSON 不破坏
  /// 序列化往返。
  ts?: string;
}

interface ChatItem {
  type: "user" | "assistant" | "tool" | "error";
  content: string;
  /// 多模态：用户附带的图片 data URL 数组。与 PanelChat 的 ChatItem.images
  /// 同 shape，让两路径写出来的 session.items 渲染兼容。
  images?: string[];
}

interface SessionIndex {
  active_id: string;
  sessions: { id: string }[];
}

interface Session {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  messages: any[];
  items: ChatItem[];
}

type StreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "toolStart"; data: { name: string; arguments: string } }
  | { event: "toolResult"; data: { name: string; result: string } }
  | { event: "done"; data: Record<string, never> }
  | { event: "error"; data: { message: string } };

export function useChat(systemPrompt: string) {
  const [messages, setMessages] = useState<ChatMessage[]>([
    { role: "system", content: systemPrompt },
  ]);
  const [currentResponse, setCurrentResponse] = useState("");
  const [toolStatus, setToolStatus] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const sessionIdRef = useRef<string>("");
  const itemsRef = useRef<ChatItem[]>([]);
  const prevPrompt = useRef(systemPrompt);
  // 取消支持：cancel() 翻 cancelledRef → 后续 onEvent chunk/done/error 全 noop；
  // accumulatedRef 保留 sendMessage 内部累积的 streaming 文本，cancel 时把它
  // finalize 成 assistant message（不丢已读到的半截内容）。
  // 注意是 soft cancel：后端 reqwest stream 仍在跑，token 仍消耗；只是 UX 立
  // 即响应不再吞 chunk。后端真取消要扩 cancellation token，留给后续。
  const cancelledRef = useRef(false);
  const accumulatedRef = useRef("");
  const updatedMessagesRef = useRef<ChatMessage[]>([]);

  // Load active session on mount
  useEffect(() => {
    (async () => {
      try {
        const index = await invoke<SessionIndex>("list_sessions");
        if (index.sessions.length > 0) {
          // Always use the newest (last created) session
          const latest = index.sessions[index.sessions.length - 1];
          const session = await invoke<Session>("load_session", { id: latest.id });
          sessionIdRef.current = session.id;
          itemsRef.current = session.items || [];
          if (session.messages && session.messages.length > 0) {
            setMessages(session.messages as ChatMessage[]);
          }
        } else {
          const session = await invoke<Session>("create_session");
          sessionIdRef.current = session.id;
          itemsRef.current = [];
        }
      } catch (e) {
        console.error("Failed to load session for pet chat:", e);
      }
    })();
  }, []);

  // Reset conversation when system prompt changes
  useEffect(() => {
    if (prevPrompt.current !== systemPrompt) {
      prevPrompt.current = systemPrompt;
      setMessages([{ role: "system", content: systemPrompt }]);
      setCurrentResponse("");
      setToolStatus("");
      setIsLoading(false);
    }
  }, [systemPrompt]);

  // Listen for proactive (pet-initiated) messages — backend already persisted them.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await listen<{ text: string; timestamp: string }>(
        "proactive-message",
        (event) => {
          const text = event.payload.text;
          if (!text) return;
          // proactive payload 自带 timestamp（后端 chrono::Local.to_rfc3339）；
          // 优先用它，缺失才 fallback now。
          const ts = event.payload.timestamp || new Date().toISOString();
          const assistantMsg: ChatMessage = { role: "assistant", content: text, ts };
          setMessages((prev) => [...prev, assistantMsg]);
          itemsRef.current = [...itemsRef.current, { type: "assistant", content: text }];
        },
      );
    })();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const saveSession = useCallback(
    async (msgs: ChatMessage[], items: ChatItem[]) => {
      if (!sessionIdRef.current) return;
      const firstUser = items.find((i) => i.type === "user");
      const title = firstUser
        ? firstUser.content.slice(0, 20) + (firstUser.content.length > 20 ? "..." : "")
        : "新会话";
      try {
        await invoke("save_session", {
          session: {
            id: sessionIdRef.current,
            title,
            created_at: "",
            updated_at: new Date().toISOString(),
            messages: msgs,
            items,
          },
        });
      } catch (e) {
        console.error("Failed to save session:", e);
      }
    },
    [],
  );

  const sendMessage = useCallback(
    async (content: string, images?: string[]) => {
      // 多模态：images 非空时 OpenAI compatible parts 数组；否则裸字符串保持
      // 与原路径与测试断言一致。
      const hasImages = !!images && images.length > 0;
      const messageContent: MessageContent = hasImages
        ? [
            ...(content ? [{ type: "text" as const, text: content }] : []),
            ...images!.map((url) => ({
              type: "image_url" as const,
              image_url: { url },
            })),
          ]
        : content;
      const userMsg: ChatMessage = {
        role: "user",
        content: messageContent,
        ts: new Date().toISOString(),
      };
      const updatedMessages = [...messages, userMsg];
      setMessages(updatedMessages);
      setIsLoading(true);
      setCurrentResponse("");
      setToolStatus("");
      // 新一轮发送 → 复位取消标志 + accumulated ref，避免上一轮 cancel 残留。
      cancelledRef.current = false;
      accumulatedRef.current = "";
      updatedMessagesRef.current = updatedMessages;

      // Track items for session saving — images 携带到 ChatItem 供 ChatMini /
      // PanelChat 跨视图统一渲染。
      itemsRef.current = [
        ...itemsRef.current,
        { type: "user", content, ...(hasImages ? { images } : {}) },
      ];

      const onEvent = new Channel<StreamEvent>();

      onEvent.onmessage = (event: StreamEvent) => {
        // soft cancel：cancel() 翻了 cancelledRef，后续 chunk / tool / done / error
        // 全 noop。partial 已在 cancel 时 finalize；这里只是吞掉残留事件。
        if (cancelledRef.current) return;
        if (event.event === "chunk") {
          accumulatedRef.current += event.data.text;
          setCurrentResponse(accumulatedRef.current);
          setToolStatus("");
        } else if (event.event === "toolStart") {
          accumulatedRef.current = "";
          setCurrentResponse("");
        } else if (event.event === "toolResult") {
          setToolStatus(`✅ ${event.data.name} done`);
        } else if (event.event === "done") {
          const accumulated = accumulatedRef.current;
          if (accumulated.trim()) {
            const assistantMsg: ChatMessage = {
              role: "assistant",
              content: accumulated,
              ts: new Date().toISOString(),
            };
            const newMsgs = [...updatedMessages, assistantMsg];
            setMessages(newMsgs);
            itemsRef.current = [...itemsRef.current, { type: "assistant", content: accumulated }];
            saveSession(newMsgs, itemsRef.current);
          }
          setCurrentResponse("");
          setToolStatus("");
          setIsLoading(false);
        } else if (event.event === "error") {
          setCurrentResponse(`出错了: ${event.data.message}`);
          itemsRef.current = [...itemsRef.current, { type: "error", content: event.data.message }];
          saveSession(updatedMessages, itemsRef.current);
          setToolStatus("");
          setIsLoading(false);
        }
      };

      try {
        await invoke("chat", {
          messages: updatedMessages,
          onEvent,
        });
      } catch (err) {
        setCurrentResponse(`出错了: ${err}`);
        setToolStatus("");
        setIsLoading(false);
      }
    },
    [messages, saveSession],
  );

  /// soft cancel：标记 cancelledRef → 后续 onEvent 事件 noop；把已累积的
  /// streaming 文本 finalize 为 assistant message + 标 "[已取消]" 后缀让用户
  /// 知道是不完整回复；isLoading 立即 false。后端 stream 仍在跑（API quota 仍
  /// 算），但用户视觉立刻得到响应。要真停后端得扩 cancellation token。
  const cancel = useCallback(() => {
    if (!isLoading) return;
    cancelledRef.current = true;
    const accumulated = accumulatedRef.current;
    if (accumulated.trim()) {
      const tagged = `${accumulated.trim()}\n\n[已取消]`;
      const assistantMsg: ChatMessage = {
        role: "assistant",
        content: tagged,
        ts: new Date().toISOString(),
      };
      const newMsgs = [...updatedMessagesRef.current, assistantMsg];
      setMessages(newMsgs);
      itemsRef.current = [
        ...itemsRef.current,
        { type: "assistant", content: tagged },
      ];
      saveSession(newMsgs, itemsRef.current);
    }
    setCurrentResponse("");
    setToolStatus("");
    setIsLoading(false);
    accumulatedRef.current = "";
  }, [isLoading, saveSession]);

  /// 让外部（App.tsx 的桌面 /image 路由）直接 push 一条 assistant message + 图片
  /// 到聊天历史与 session item 里，绕过 LLM。content 是显示文本（前缀 emoji 等
  /// caller 自定义）；images 非空时 ChatMini 会渲缩略图（multimodal extractor 仍
  /// 能从字符串 content + images 字段拼出渲染视图）。
  const appendAssistant = useCallback(
    (content: string, images?: string[]) => {
      const ts = new Date().toISOString();
      const assistantMsg: ChatMessage = {
        role: "assistant",
        content,
        ts,
      };
      let newMsgs: ChatMessage[] = [];
      setMessages((prev) => {
        newMsgs = [...prev, assistantMsg];
        return newMsgs;
      });
      const hasImg = !!images && images.length > 0;
      itemsRef.current = [
        ...itemsRef.current,
        { type: "assistant", content, ...(hasImg ? { images } : {}) },
      ];
      // 用 setTimeout 0 让 setMessages 的状态更新先 commit；saveSession 拿到的
      // newMsgs 是闭包内最新引用（functional updater 同步赋值 → newMsgs 即时
      // 可用）。
      void saveSession(newMsgs, itemsRef.current);
    },
    [saveSession],
  );

  const lastAssistantMsg = [...messages]
    .reverse()
    .find((m) => m.role === "assistant");
  // 桌面气泡只显文本：assistant 实时路径恒为 string，但联合类型让 TS
  // 要求 narrowing —— extractText 同时安全处理 string / 多模态 array。
  const displayMessage = currentResponse || (lastAssistantMsg ? extractText(lastAssistantMsg.content) : "");
  const showBubble = isLoading || !!lastAssistantMsg;

  return {
    messages,
    currentResponse,
    toolStatus,
    isLoading,
    sendMessage,
    cancel,
    appendAssistant,
    displayMessage,
    showBubble,
  };
}
