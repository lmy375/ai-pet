import type { ReactNode } from "react";

interface Props {
  role: "user" | "assistant";
  error?: boolean;
  children: ReactNode;
}

/** iOS Messages-style bubble: user = accent blue (right), assistant = gray (left). */
export function MessageBubble({ role, error = false, children }: Props) {
  const isUser = role === "user";
  const tone = error
    ? "bg-red-50 text-red-600 rounded-bl-md"
    : isUser
      ? "bg-accent text-white rounded-br-md"
      : "bg-slate-200 text-slate-900 rounded-bl-md";

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={`max-w-[80%] whitespace-pre-wrap break-words rounded-2xl px-3.5 py-2 text-[14px] leading-relaxed ${tone}`}
      >
        {children}
      </div>
    </div>
  );
}
