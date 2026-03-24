import { useState, useCallback, useEffect, useRef } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";

interface ChatMessage {
  role: "user" | "assistant" | "system" | "tool";
  content: string;
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
  const prevPrompt = useRef(systemPrompt);

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

  const sendMessage = useCallback(
    async (content: string) => {
      const userMsg: ChatMessage = { role: "user", content };
      const updatedMessages = [...messages, userMsg];
      setMessages(updatedMessages);
      setIsLoading(true);
      setCurrentResponse("");
      setToolStatus("");

      const onEvent = new Channel<StreamEvent>();
      let accumulated = "";

      onEvent.onmessage = (event: StreamEvent) => {
        if (event.event === "chunk") {
          accumulated += event.data.text;
          setCurrentResponse(accumulated);
          setToolStatus("");
        } else if (event.event === "toolStart") {
          setToolStatus(`🔧 ${event.data.name}...`);
          // Reset accumulated for next LLM round
          accumulated = "";
          setCurrentResponse("");
        } else if (event.event === "toolResult") {
          setToolStatus(`✅ ${event.data.name} done`);
        } else if (event.event === "done") {
          setMessages((prev) => [
            ...prev,
            { role: "assistant", content: accumulated },
          ]);
          setCurrentResponse("");
          setToolStatus("");
          setIsLoading(false);
        } else if (event.event === "error") {
          setCurrentResponse(`出错了: ${event.data.message}`);
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
    [messages],
  );

  const lastAssistantMsg = [...messages]
    .reverse()
    .find((m) => m.role === "assistant");
  const displayMessage =
    toolStatus || currentResponse || lastAssistantMsg?.content || "";
  const showBubble = isLoading || !!lastAssistantMsg;

  return {
    messages,
    currentResponse,
    toolStatus,
    isLoading,
    sendMessage,
    displayMessage,
    showBubble,
  };
}
