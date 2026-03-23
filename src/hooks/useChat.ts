import { useState, useCallback } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";

interface ChatMessage {
  role: "user" | "assistant" | "system";
  content: string;
}

type StreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "done"; data: Record<string, never> }
  | { event: "error"; data: { message: string } };

export function useChat() {
  const [messages, setMessages] = useState<ChatMessage[]>([
    {
      role: "system",
      content:
        "你是一个可爱的二次元少女 AI 宠物，性格活泼开朗。请用简短可爱的方式回复，偶尔使用颜文字。回复控制在50字以内。",
    },
  ]);
  const [currentResponse, setCurrentResponse] = useState("");
  const [isLoading, setIsLoading] = useState(false);

  const sendMessage = useCallback(
    async (content: string) => {
      const userMsg: ChatMessage = { role: "user", content };
      const updatedMessages = [...messages, userMsg];
      setMessages(updatedMessages);
      setIsLoading(true);
      setCurrentResponse("");

      const onEvent = new Channel<StreamEvent>();
      let accumulated = "";

      onEvent.onmessage = (event: StreamEvent) => {
        if (event.event === "chunk") {
          accumulated += event.data.text;
          setCurrentResponse(accumulated);
        } else if (event.event === "done") {
          setMessages((prev) => [
            ...prev,
            { role: "assistant", content: accumulated },
          ]);
          setCurrentResponse("");
          setIsLoading(false);
        } else if (event.event === "error") {
          setCurrentResponse(`出错了: ${event.data.message}`);
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
        setIsLoading(false);
      }
    },
    [messages],
  );

  const lastAssistantMsg = [...messages]
    .reverse()
    .find((m) => m.role === "assistant");
  const displayMessage = currentResponse || lastAssistantMsg?.content || "";
  const showBubble = isLoading || !!lastAssistantMsg;

  return {
    messages,
    currentResponse,
    isLoading,
    sendMessage,
    displayMessage,
    showBubble,
  };
}
