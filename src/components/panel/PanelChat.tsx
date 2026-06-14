import { useState } from "react";
import { useChat } from "../../hooks/useChat";
import { ChatThread } from "../ChatThread";
import { ChatInput } from "../ChatInput";
import { Button } from "../ui/Button";
import { ChevronDown, ChevronRight, PlusIcon, TrashIcon } from "../Icons";

export function PanelChat() {
  const {
    items,
    isLoading,
    currentResponse,
    currentToolCalls,
    loaded,
    sessionId,
    sessionTitle,
    sessionList,
    sendMessage,
    newSession,
    switchSession,
    deleteSession,
  } = useChat();

  const [showSessionList, setShowSessionList] = useState(false);

  if (!loaded) {
    return <div className="flex h-full items-center justify-center text-[14px] text-slate-400">加载中...</div>;
  }

  return (
    <div className="flex h-full flex-col bg-slate-100">
      {/* Session header bar */}
      <div className="flex shrink-0 items-center gap-2 border-b border-slate-200/70 bg-white px-4 py-2">
        <button
          className="flex min-w-0 flex-1 items-center gap-1.5"
          onClick={() => setShowSessionList(!showSessionList)}
        >
          <span className="truncate text-[13px] font-semibold text-slate-800">{sessionTitle}</span>
          {showSessionList ? (
            <ChevronDown className="h-4 w-4 shrink-0 text-slate-400" />
          ) : (
            <ChevronRight className="h-4 w-4 shrink-0 text-slate-400" />
          )}
        </button>
        <Button variant="ghost" size="sm" onClick={() => { newSession(); setShowSessionList(false); }} title="新建会话">
          <PlusIcon className="h-4 w-4" />
          新会话
        </Button>
      </div>

      {/* Session list dropdown */}
      {showSessionList && (
        <div className="max-h-60 shrink-0 overflow-y-auto border-b border-slate-200/70 bg-white">
          {sessionList.length === 0 ? (
            <div className="py-3 text-center text-[12px] text-slate-400">暂无历史会话</div>
          ) : (
            [...sessionList].reverse().map((s) => (
              <div
                key={s.id}
                className={`flex items-center gap-2 border-b border-slate-100 px-3 py-2 ${
                  s.id === sessionId ? "bg-sky-50" : ""
                }`}
              >
                <button
                  className="min-w-0 flex-1 text-left"
                  onClick={() => { switchSession(s.id); setShowSessionList(false); }}
                >
                  <div className={`truncate text-[13px] text-slate-800 ${s.id === sessionId ? "font-semibold" : ""}`}>
                    {s.title}
                  </div>
                  <div className="text-[11px] text-slate-400">{s.updated_at.split("T")[0]}</div>
                </button>
                <button
                  onClick={(e) => { e.stopPropagation(); deleteSession(s.id); }}
                  title="删除会话"
                  className="flex h-7 w-7 items-center justify-center rounded-md text-slate-400 transition-colors hover:bg-red-50 hover:text-red-500"
                >
                  <TrashIcon className="h-4 w-4" />
                </button>
              </div>
            ))
          )}
        </div>
      )}

      {/* Message list */}
      <ChatThread
        items={items}
        currentToolCalls={currentToolCalls}
        streaming={currentResponse}
        loading={isLoading}
        className="flex-1 p-4"
        emptyHint="开始聊天吧~"
      />

      {/* Input bar */}
      <div className="shrink-0 border-t border-slate-200/70 bg-white px-4 py-3">
        <ChatInput onSend={sendMessage} isLoading={isLoading} />
      </div>
    </div>
  );
}
