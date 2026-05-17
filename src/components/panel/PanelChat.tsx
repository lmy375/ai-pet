import { Fragment, useState, useRef, useEffect, useCallback, useMemo } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { monthKeyFromIso, monthLabelOf } from "../../utils/monthGroup";
import { ToolCallBlock } from "./ToolCallBlock";
import { TaskProposalCard, parseTaskProposal } from "./TaskProposalCard";
import { SlashCommandMenu } from "./SlashCommandMenu";
import { ImagePromptHistoryMenu } from "./ImagePromptHistoryMenu";
import {
  extractCommandPrefix,
  filterCommandsByPrefix,
  formatHelpText,
  formatImageHelpText,
  parseSlashCommand,
  readImagePrompts,
  recordImagePrompt,
  attachThumbToImagePrompt,
  type ImagePromptEntry,
  recordSlashCommandUsage,
  clearSlashScores,
  computeSnoozeUntil,
  type SlashAction,
  type SlashCommand,
} from "./slashCommands";
import {
  bubbleStyle,
  CopyableMessage,
  exportSessionAsMarkdown,
  SearchResultRow,
  type ChatItem,
  type SearchHit,
  type ToolCall,
} from "./panelChatBits";
import { ImageLightbox } from "../common/ImageLightbox";
import { EmptyState } from "./EmptyState";
import { matchTaskByQuery, formatMultiHitMessage } from "./taskSlashHelpers";
import { formatRelativeAgeBuckets } from "../../utils/formatRelativeAge";
import { useSearchHistory } from "../../hooks/useSearchHistory";
import { readSentHistory, pushSentHistory } from "../chatHistoryStore";

type StreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "toolStart"; data: { name: string; arguments: string } }
  | { event: "toolResult"; data: { name: string; result: string } }
  | { event: "done"; data: Record<string, never> }
  | { event: "error"; data: { message: string } };

interface SessionMeta {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  /** R93: 会话内可见 chat item 总数（user / assistant / tool / error；不
   *  含 system）。`?` 守卫覆盖：旧前端读到迁移前 index.json 时为 undefined，
   *  此时 dropdown 隐藏 "(N 条)"，避免误显 "(0 条)" 让用户以为是空会话。 */
  item_count?: number;
  /** 钉住的会话永远排在列表前；后端 set_session_pinned 切换。 */
  pinned?: boolean;
}

interface SessionIndex {
  active_id: string;
  sessions: SessionMeta[];
}

interface Session {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  messages: any[];
  items: ChatItem[];
}

interface PanelChatProps {
  /// 让 panel chat 通过 `/tasks` 等 slash 命令请求父组件切 tab。父组件传入
  /// `setActiveTab` 即可。可选 —— 不传则 `/tasks` 走 Unknown 反馈路径。
  onRequestTab?: (tab: "设置" | "聊天" | "任务" | "记忆" | "人格") => void;
  /// 双击消息正文里的 `「task title」` ref token → 请求父组件切到「任务」
  /// tab 并把焦点落到该 title 的卡片上。可选 —— 不传则 ref token 仍可
  /// hover 显 status，但双击 noop。
  onRequestFocusTask?: (title: string) => void;
  /// 跨窗口 deeplink 跳到当前 session 内某条消息的载荷（excerpt 文本）。
  /// 桌面 ChatMini 右键菜单"在 Panel 中定位本条"会把消息文本前 80 字写到
  /// localStorage `pet-panel-deeplink.chatMatch.excerpt`，PanelApp 消费后
  /// 切到聊天 tab 并把 excerpt 推到本 prop。PanelChat 反向扫 items 找最近
  /// 子串命中 → setPendingScroll(idx) 走既有 scrollIntoView + 高亮。
  pendingChatMatch?: string | null;
  /// 命中（或找不到也算消费）后回调清空 pendingChatMatch，避免 stale
  /// 值在用户后续切 session / 滚动后又触发。
  onConsumePendingChatMatch?: () => void;
  /// PanelTasks 「🧠 ask LLM about selection」 按钮触发：把封装好的
  /// "关于「<excerpt>」 " 串推到 textarea 让 owner 立刻问。挂载 / 更新
  /// 时 effect 消费 → setInput + focus + 清空 prop。
  pendingChatPrefill?: string | null;
  onConsumePendingChatPrefill?: () => void;
}

/// 把 data URL 图片用 canvas 缩到 maxSize 短边内、JPEG 0.7 质量重编码，
/// 用作 /image 历史菜单缩略图（5 条 cap × ~6KB = ~30KB localStorage 占用）。
/// 失败 reject 让调用者吞掉错误（缩略图缺失是 graceful degrade，不应阻塞
/// 主流程）。
function makeImagePromptThumb(dataUrl: string, maxSize = 64): Promise<string> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      try {
        const ratio = Math.min(maxSize / img.width, maxSize / img.height, 1);
        const w = Math.max(1, Math.round(img.width * ratio));
        const h = Math.max(1, Math.round(img.height * ratio));
        const canvas = document.createElement("canvas");
        canvas.width = w;
        canvas.height = h;
        const ctx = canvas.getContext("2d");
        if (!ctx) {
          reject(new Error("no 2d ctx"));
          return;
        }
        ctx.drawImage(img, 0, 0, w, h);
        resolve(canvas.toDataURL("image/jpeg", 0.7));
      } catch (e) {
        reject(e);
      }
    };
    img.onerror = () => reject(new Error("image load failed"));
    img.src = dataUrl;
  });
}

/// ⇧+点击复制时拼在 payload 顶部的本地时间戳：YYYY-MM-DD HH:MM（分钟粒度
/// 即可，归档 / share 场景看不到秒）。toISOString 走 UTC 会偏移，所以用
/// 本地 getter 手拼。
function formatLocalStamp(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

// monthKeyFromIso / monthLabelOf 已提到 `src/utils/monthGroup.ts` 共享给
// session 下拉 + 跨会话搜索 + PanelMemory items 三处复用。

/// `@` 提及触发判定：cursor 前方是否有"未被空白打断的 @<chars>"段。命中
/// 返回 `{ start, query }`（start = `@` 在 input 中的索引，query = @ 后到
/// cursor 之间的字符串），否则 null。
/// 触发条件：
///   1. `@` 前必须是 input 起始或空白 / 换行 / 全角中文标点（避免邮件地
///      址等 inline `@` 误触）；
///   2. `@` 与 cursor 之间不含空白 / 换行 / 角引号 `「」`（角引号是任务
///      ref token 的边界，遇到说明上一段 ref 已完成）；
///   3. `@` 后允许空 query（用户刚敲完 `@`，菜单仍开始过滤全集）。
function extractMentionContext(
  input: string,
  cursorPos: number,
): { start: number; query: string } | null {
  if (cursorPos <= 0 || cursorPos > input.length) return null;
  // 从 cursor 往前找最近的 `@`；遇到空白 / 角引号即放弃。
  let i = cursorPos - 1;
  while (i >= 0) {
    const ch = input[i];
    if (ch === "@") {
      const before = i > 0 ? input[i - 1] : "";
      // 前一字符必须是 boundary（起始 / ASCII 空白 / 换行 / 全角空格 / 中文标点）。
      // 这里只放行最常见的几种：start of string、ASCII whitespace、全角空格。
      if (
        i === 0 ||
        before === " " ||
        before === "\n" ||
        before === "\t" ||
        before === "　"
      ) {
        return { start: i, query: input.slice(i + 1, cursorPos) };
      }
      return null;
    }
    if (
      ch === " " ||
      ch === "\n" ||
      ch === "\t" ||
      ch === "　" ||
      ch === "「" ||
      ch === "」"
    ) {
      return null;
    }
    i -= 1;
  }
  return null;
}

/// `@` picker / ⌘K picker 共享的 char-order 子序列 fuzzy 匹配。query 每
/// 个字符按顺序在 target 里找下一处出现位置；找全 = 命中，返回 score
/// （span × 100 + firstMatch，越紧凑 / 越靠前越优）；找不全 null。空
/// query 返 0 让全集通过。
function fuzzyMatchTaskTitle(query: string, target: string): number | null {
  if (query.length === 0) return 0;
  let qi = 0;
  let firstMatch = -1;
  let lastMatch = -1;
  for (let ti = 0; ti < target.length && qi < query.length; ti++) {
    if (target[ti] === query[qi]) {
      if (firstMatch < 0) firstMatch = ti;
      lastMatch = ti;
      qi += 1;
    }
  }
  if (qi !== query.length) return null;
  return (lastMatch - firstMatch + 1) * 100 + firstMatch;
}

/// PanelChat 输入框 "📋 prompt 模板" 下拉的预填项。每条 = 一种常见
/// chat 触发模板，引导用户写出 LLM 易理解的 form。label 是 dropdown 显
/// 示文案，text 是 prefill 到 input 的字符串。与 PanelTasks 的
/// TASK_TEMPLATES 平行，但目标是 chat prompt 而非 queue 任务。
const CHAT_PROMPT_TEMPLATES: Array<{ label: string; text: string }> = [
  {
    label: "🪞 复盘",
    text: "我刚做完 [事项]，能帮我复盘 3 点：\n1. 做得好的\n2. 可以更好的\n3. 下次怎么改",
  },
  {
    label: "❓ 提问",
    text: "我想问 [topic]。能不能从入门到核心要点 3-5 条讲清楚？",
  },
  {
    label: "📝 写笔记",
    text: "把我们刚才聊的 [topic] 要点提炼成 3-5 条 bullet，写到 memory 里（category: ai_insights）。",
  },
  {
    label: "🛠 派任务",
    text: "帮我派一条任务：标题 [简短动作]，描述 [明确产物 + 范围]，可选 priority / due / tags。",
  },
];

/// "↩️ 快速 follow-up" 短回应模板：对话中接 assistant 后用。与 prompt
/// templates 分开 —— 这些是简短回应，前提是 chat 已有交流（items.length > 0）。
/// 不在空 chat 显，避免新会话首条就发这种空话。
const CHAT_FOLLOWUP_TEMPLATES: Array<{ label: string; text: string }> = [
  { label: "👌 明白了", text: "明白了，谢谢。" },
  { label: "🔍 再细说", text: "能再细说一下吗？尤其是 [关键点]。" },
  { label: "🔄 换个例子", text: "换个例子试试？这个还不太具体。" },
];

/**
 * 给定 `items[userItemIdx]`（必须 type==='user'）算它在 `messages`（LLM-facing
 * 数组，含 system）中对应的 index。
 *
 * 设计：items 与 messagesRef 不是逐位对齐的 —— 一个 chat turn 可能在 items
 * 里写出多条 assistant（toolStart 把 accumulated 中转写一条），但
 * messagesRef 只在 "done" 时 push 一次最终 assistant。所以不能用"items 偏移
 * +1"硬算。
 *
 * 但 user 是逐位 1:1 的：sendMessage 一次同时给 items 和 messagesRef 各推
 * 一条 user。所以"items 第 K 个 user"对应"messages 第 K 个 user 角色 msg"。
 * 算法：扫一遍 items[0..userItemIdx] 数 user 计数 K；再扫 messages 找第 K
 * 个 role==='user'，返回其下标。不命中（不一致 / 边界）返 null —— caller
 * 应当退回拒绝编辑，避免错误截断。
 *
 * 导出便于将来加 unit test（vitest 落地时直接覆盖）。
 */
/// 粗略估算 token 数：CJK 字符 ~1 token/字，非 CJK 非空白字符 ~1 token/4 字。
/// 与各 LLM 实际 tokenizer 都对不齐（GPT-4o BPE / Claude / Qwen 都不同），
/// 但作为"我打了多长"的感知 chip 足够：±25% 误差不影响"30 vs 3000"的决策。
/// 仅算 input 串，不计 system / history —— chip 是"当下这条" 直觉，不是
/// 实际 LLM context 计费器（那要后端按真实 tokenizer 算，太重）。
///
/// 导出便于将来加 vitest pin。CJK 范围覆盖最常见 CJK Unified Ideographs +
/// 日文假名 + 韩文音节；其它语种归 "non-CJK"（含 ASCII / 拉丁扩展 / 阿拉
/// 伯文等）—— 边界 OK 因为这是 token 估算，不是字符分类。
export function estimateInputTokens(s: string): number {
  if (!s) return 0;
  let cjk = 0;
  let other = 0;
  for (const ch of s) {
    const code = ch.codePointAt(0) ?? 0;
    // CJK Unified Ideographs (4E00–9FFF) + 假名 (3040–30FF) + Hangul (AC00–D7AF)
    const isCJK =
      (code >= 0x4e00 && code <= 0x9fff) ||
      (code >= 0x3040 && code <= 0x30ff) ||
      (code >= 0xac00 && code <= 0xd7af);
    if (isCJK) {
      cjk++;
    } else if (!/\s/.test(ch)) {
      other++;
    }
  }
  return Math.ceil(cjk + other / 4);
}

export function findMessageIndexForUserItem(
  items: ChatItem[],
  messages: Array<{ role?: string }>,
  userItemIdx: number,
): number | null {
  if (userItemIdx < 0 || userItemIdx >= items.length) return null;
  if (items[userItemIdx]?.type !== "user") return null;
  let userOrdinal = 0;
  for (let i = 0; i <= userItemIdx; i++) {
    if (items[i]?.type === "user") userOrdinal++;
  }
  if (userOrdinal === 0) return null;
  let seen = 0;
  for (let i = 0; i < messages.length; i++) {
    if (messages[i]?.role === "user") {
      seen++;
      if (seen === userOrdinal) return i;
    }
  }
  return null;
}

export function PanelChat({
  onRequestTab,
  onRequestFocusTask,
  pendingChatMatch,
  onConsumePendingChatMatch,
  pendingChatPrefill,
  onConsumePendingChatPrefill,
}: PanelChatProps = {}) {
  const [items, setItems] = useState<ChatItem[]>([]);
  const [input, setInput] = useState("");
  /// IM 风消息编辑/重发：双击 user bubble → editingItemIdx = i + draft 预填
  /// 当前内容；Enter 提交（截断 items[i+1:] + messagesRef 对应位 + sendMessage
  /// 重发，得到新的 assistant 回复）；Esc 取消；Shift+Enter 换行。流式中
  /// （isLoading）禁止进入编辑 —— 截断对正在跑的 chat 会把 messagesRef 改
  /// 掉但 invoke 仍引用旧数据，行为模糊。
  const [editingItemIdx, setEditingItemIdx] = useState<number | null>(null);
  const [editingDraft, setEditingDraft] = useState("");
  /// 用户自定义 chat 模板（与 module-level CHAT_PROMPT_TEMPLATES 内置三段
  /// 互补）。localStorage `pet-chat-custom-templates` → Array<{label, text}>。
  /// cap 10 防 storage 膨胀；FIFO push 老的挤出。
  const CUSTOM_TEMPLATES_CAP = 10;
  const [customChatTemplates, setCustomChatTemplates] = useState<
    Array<{ label: string; text: string }>
  >(() => {
    try {
      const raw = window.localStorage.getItem("pet-chat-custom-templates");
      if (!raw) return [];
      const arr = JSON.parse(raw);
      if (Array.isArray(arr)) {
        return arr
          .filter(
            (v): v is { label: string; text: string } =>
              typeof v === "object" &&
              v !== null &&
              typeof v.label === "string" &&
              typeof v.text === "string",
          )
          .slice(0, CUSTOM_TEMPLATES_CAP);
      }
    } catch {
      // 损坏 → 空数组
    }
    return [];
  });
  const saveCustomTemplate = (label: string, text: string) => {
    setCustomChatTemplates((prev) => {
      // 同 label 替换；新条尾插；cap 10 FIFO
      const filtered = prev.filter((t) => t.label !== label);
      const next = [...filtered, { label, text }].slice(-CUSTOM_TEMPLATES_CAP);
      try {
        window.localStorage.setItem(
          "pet-chat-custom-templates",
          JSON.stringify(next),
        );
      } catch {
        // session 内仍生效
      }
      return next;
    });
  };
  const persistCustomTemplates = (next: Array<{ label: string; text: string }>) => {
    setCustomChatTemplates(next);
    try {
      window.localStorage.setItem(
        "pet-chat-custom-templates",
        JSON.stringify(next),
      );
    } catch {
      // session 内仍生效
    }
  };
  /// 自定义模板管理 modal 开关。
  const [manageTemplatesOpen, setManageTemplatesOpen] = useState(false);
  // compose 草稿持久化：key = `pet-chat-draft-${sessionId}`。3s debounce
  // 防短间隔写盘；input 清空时（发送 / Esc）也清掉该 session 的 draft key
  // 让 storage 不积陈旧条目。sessionId 切换时 effect 自动读新 key 填到
  // textarea（见下一个 useEffect）。
  const draftWriteTimerRef = useRef<number | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [currentResponse, setCurrentResponse] = useState("");
  const [currentToolCalls, setCurrentToolCalls] = useState<ToolCall[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const messagesRef = useRef<any[]>([]);
  // 📎 隐藏 file input：点 📎 按钮 .click() 弹系统选择器，onChange 收文件
  // 走与 paste / drop 同一条 ingestImageBlobs 路径。multiple + accept="image/*"
  // 把限定下推给 OS 对话框。
  const composeFileInputRef = useRef<HTMLInputElement>(null);
  // 主 compose textarea DOM ref：⌘K task 引用选择器需读光标位置 + 插入
  // ref 字符串后把光标恢复到插入末尾。其它路径（paste / drop / submit）
  // 不依赖 DOM，所以之前没拉 ref —— 现在按需补一个。
  const composeTextareaRef = useRef<HTMLTextAreaElement>(null);
  // ⌘K task 引用选择器：popup state。打开时一次性 invoke task_list 缓存
  // 在 picker session 内，输入过滤是纯前端 includes() lowercase。
  type TaskRefView = { title: string; status: string };
  const [taskPickerOpen, setTaskPickerOpen] = useState(false);
  const [taskPickerTasks, setTaskPickerTasks] = useState<TaskRefView[]>([]);
  const [taskPickerQuery, setTaskPickerQuery] = useState("");
  const [taskPickerSelectedIdx, setTaskPickerSelectedIdx] = useState(0);
  const taskPickerInputRef = useRef<HTMLInputElement>(null);
  // `@` inline ref picker：与 ⌘K 互补，IM 风格"@提到"直觉。@ 触发态由 input +
  // 光标位置导出（extractMentionContext），renderable 时显浮窗在 input bar 上
  // 方（同 SlashCommandMenu 锚点）。tasks 列表复用 chatTaskMap（已在挂载时刷新）。
  const [composeCursorPos, setComposeCursorPos] = useState<number>(0);
  const [mentionSelectedIdx, setMentionSelectedIdx] = useState(0);
  /// 当前 task → status + updated_at 的本地缓存。让消息正文里的「title」
  /// 引用 token 渲成 hover-able underline 显当前状态。打开 panel 时拉一
  /// 次；⌘K picker 打开 → 顺路刷新（与最新 task_list 同源）。
  const [chatTaskMap, setChatTaskMap] = useState<
    Record<string, { status: string; updated_at: string }>
  >({});
  /// 消息 "📌 标记" 收藏集：localStorage `pet-chat-marked-messages` →
  /// Map of `${sessionId}::${itemIdx}` → markedAt (epoch ms)。重 hash content
  /// 太重；用 (session, idx) 已覆盖 95% —— 上游消息编辑 / 删除时索引漂移，
  /// dangling 不要紧（用户重新点 📌 即更新）。
  /// 存储兼容：旧版本是 Array<string> 仅含 key（无 ts）；read 时兼容两种
  /// 形态，老 key 的 ts 退到 0（"时间未知"）。新 toggle 写 Record<key, ts>。
  const [markedMessages, setMarkedMessages] = useState<Map<string, number>>(() => {
    try {
      const raw = window.localStorage.getItem("pet-chat-marked-messages");
      if (!raw) return new Map();
      const parsed = JSON.parse(raw);
      const m = new Map<string, number>();
      if (Array.isArray(parsed)) {
        // 旧格式：Array<string>
        for (const v of parsed) {
          if (typeof v === "string") m.set(v, 0);
        }
      } else if (parsed && typeof parsed === "object") {
        // 新格式：Record<key, ts>
        for (const [k, ts] of Object.entries(parsed)) {
          if (typeof k === "string" && typeof ts === "number") {
            m.set(k, ts);
          }
        }
      }
      return m;
    } catch {
      return new Map();
    }
  });
  /// 标记消息查看 modal 状态 + 拉来的条目。Open 时按 sessionId 分组、
  /// batch load_session 后 walk items[idx] 收集 (sessionId/title/idx/role/
  /// content) tuple。entries === null 表示 loading 中；空数组 = "全部
  /// dangling"（用户标过的 session 都已删 / 改名）。
  type MarkedEntry = {
    sessionId: string;
    sessionTitle: string;
    itemIdx: number;
    role: string;
    content: string;
    /// 标记时间 epoch ms。0 表示老格式无 ts。
    markedAt: number;
  };
  const [marksModalOpen, setMarksModalOpen] = useState(false);
  const [marksModalEntries, setMarksModalEntries] = useState<
    MarkedEntry[] | null
  >(null);
  /// marks modal 内的过滤 query：按 session 标题 / content 子串匹配。
  /// 仅在 modal 打开期间用；关闭时清空避免下次打开带 stale query。
  const [marksModalQuery, setMarksModalQuery] = useState("");
  /// "📋 复制"按钮成功后 1.5s "✓ 已复制"反馈态，与 chat 复制按钮同模式。
  const [marksModalCopied, setMarksModalCopied] = useState(false);
  const openMarksModal = useCallback(async () => {
    setMarksModalOpen(true);
    setMarksModalEntries(null);
    setMarksModalQuery("");
    // 按 sessionId 分组：保留每个 idx 的 markedAt ts 给 modal 展示
    const bySession = new Map<string, Array<{ idx: number; markedAt: number }>>();
    for (const [k, ts] of markedMessages) {
      const sepIdx = k.indexOf("::");
      if (sepIdx < 0) continue;
      const sid = k.slice(0, sepIdx);
      const idx = parseInt(k.slice(sepIdx + 2), 10);
      if (!sid || Number.isNaN(idx)) continue;
      const arr = bySession.get(sid) ?? [];
      arr.push({ idx, markedAt: ts });
      bySession.set(sid, arr);
    }
    const entries: MarkedEntry[] = [];
    for (const [sid, idxList] of bySession) {
      try {
        const session = await invoke<Session>("load_session", { id: sid });
        for (const { idx, markedAt } of idxList) {
          const it = session.items?.[idx];
          if (!it) continue;
          // tool / error 行不在 mark 范围（onToggleMark 不传），但 dangling
          // 防御性 skip 一遍
          if (it.type !== "user" && it.type !== "assistant") continue;
          entries.push({
            sessionId: sid,
            sessionTitle: session.title,
            itemIdx: idx,
            role: it.type,
            content: it.content,
            markedAt,
          });
        }
      } catch {
        // 单 session 拉失败容忍 —— 其它 session 仍能显
      }
    }
    // 按标记时间倒序（最新标记在前；老格式 ts=0 自然落底）
    entries.sort((a, b) => b.markedAt - a.markedAt);
    setMarksModalEntries(entries);
  }, [markedMessages]);
  const toggleMessageMark = useCallback((key: string) => {
    setMarkedMessages((prev) => {
      const next = new Map(prev);
      if (next.has(key)) next.delete(key);
      else next.set(key, Date.now());
      try {
        window.localStorage.setItem(
          "pet-chat-marked-messages",
          JSON.stringify(Object.fromEntries(next)),
        );
      } catch {
        // 私密 / quota 满 —— session 内仍生效
      }
      return next;
    });
  }, []);
  /// 「🔍 在当前 session 找类似」：用消息文本作 query 触发顶部 search bar。
  /// 截短到 ≤ 30 字 + 折掉换行，避免 query 过长在 input 里挤爆。scope 强制
  /// "current"（"找本会话内类似" 是这条按钮的核心语义 — 不让 owner 再切
  /// scope 一次）。setSearchMode(true) 唤起 search UI，setSearchInputFocusReq
  /// 让 useEffect rAF 后聚焦 input 让 owner 立刻可改 query / 按 Enter 入历
  /// 史。
  const handleFindSimilarInSession = useCallback((text: string) => {
    const q = text
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, 30);
    if (!q) return;
    setSearchMode(true);
    setSearchScope("current");
    setSearchQuery(q);
  }, []);
  const refreshChatTaskMap = useCallback(async () => {
    try {
      const resp = await invoke<{
        tasks: Array<{ title: string; status: string; updated_at: string }>;
      }>("task_list");
      const m: Record<string, { status: string; updated_at: string }> = {};
      for (const t of resp.tasks) {
        m[t.title] = { status: t.status, updated_at: t.updated_at };
      }
      setChatTaskMap(m);
    } catch {
      // task_list 失败 → 保留旧 map（避免一次 IO 抖动把所有 ref 退化成
      // muted "已归档"提示）；下次刷新有机会再补
    }
  }, []);
  useEffect(() => {
    void refreshChatTaskMap();
  }, [refreshChatTaskMap]);
  const openTaskPicker = useCallback(async () => {
    setTaskPickerOpen(true);
    setTaskPickerQuery("");
    setTaskPickerSelectedIdx(0);
    try {
      const resp = await invoke<{
        tasks: Array<{ title: string; status: string; updated_at: string }>;
      }>("task_list");
      // 只暴露 title + status —— popup 列表用，TaskView 其它字段（priority /
      // due / detail_path 等）暂不在 picker 里露，让 UI 简洁。
      setTaskPickerTasks(
        resp.tasks.map((t) => ({ title: t.title, status: t.status })),
      );
      // 同时刷新 chatTaskMap：picker 打开本来就是用户在做 task 相关动作，
      // 顺手保持消息正文里的 hover tooltip 最新。
      const m: Record<string, { status: string; updated_at: string }> = {};
      for (const t of resp.tasks) {
        m[t.title] = { status: t.status, updated_at: t.updated_at };
      }
      setChatTaskMap(m);
    } catch {
      // task_list 失败（memory 目录权限 / 损坏）—— picker 显空列表 +
      // 用户看到 "（没有任务）" 后用 Esc 关掉就好，不阻塞 chat 主流。
      setTaskPickerTasks([]);
    }
  }, []);
  /// 全局 ⌘K / Ctrl+K 热键：让用户在 Panel 任意位置（消息区 / 侧栏 / 顶部
  /// chip 等）按下也能唤出 task picker，而不必先点到 textarea。textarea 自己
  /// 的 onKeyDown 已有同款 handler —— 当 textarea 处于焦点时不重复触发，让
  /// textarea 路径独占（保留既有 `e.preventDefault()` 抢键时序）。
  /// 离开 Panel（切到 Tasks / Memory tab 等）时此监听仍挂在 window 上，但因
  /// document 实例独立（Tauri 多 webview），不会跨窗口误触。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (
        !(e.metaKey || e.ctrlKey) ||
        e.shiftKey ||
        e.altKey ||
        e.key.toLowerCase() !== "k"
      ) {
        return;
      }
      // textarea 自带 handler → 这里跳过避免双触发（picker open 是幂等的，
      // 但 e.preventDefault 调两次没意义且让事件流不清楚）。
      if (document.activeElement === composeTextareaRef.current) return;
      // input / textarea / contentEditable 等其它输入框也跳过 —— 用户可能在
      // session 标题 inline rename / 搜索框等输入态，⌘K 应该被那些控件优先
      // 接管（如果它们想用）。
      const ae = document.activeElement;
      if (
        ae instanceof HTMLInputElement ||
        ae instanceof HTMLTextAreaElement ||
        (ae instanceof HTMLElement && ae.isContentEditable)
      ) {
        return;
      }
      e.preventDefault();
      void openTaskPicker();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [openTaskPicker]);

  /// 全局 ⌘N / Ctrl+N 热键：新建会话。与 IDE / 浏览器 ⌘N = "新建文件 /
  /// 标签页"直觉一致。让位条件与 ⌘K 同 —— 输入控件聚焦时让那些控件处理
  /// （即便它们当前没有 ⌘N handler，也避免我们抢走可能的未来扩展）。
  /// textarea 自己的 onKeyDown 也有一份（line ~2890），让 power user 从输入
  /// 框敲也能触发；这里仅覆盖"消息区 / 侧栏 / chip 区"等非输入焦点位置。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (
        !(e.metaKey || e.ctrlKey) ||
        e.shiftKey ||
        e.altKey ||
        e.key.toLowerCase() !== "n"
      ) {
        return;
      }
      const ae = document.activeElement;
      if (
        ae instanceof HTMLInputElement ||
        ae instanceof HTMLTextAreaElement ||
        (ae instanceof HTMLElement && ae.isContentEditable)
      ) {
        return;
      }
      e.preventDefault();
      void handleNewSession();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // handleNewSession 每 render 重建但内部只读 stable setters + invoke，故
    // 不放进 deps（避免 N 次 re-subscribe）。ESLint 提示忽略 —— 行为正确。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const insertTaskRef = useCallback(
    (title: string) => {
      // 插入格式 `「title」` —— 全角直角引号，宠物侧 prompt 处理时易抓出
      // ref token（不与 markdown 的反引号 / 直角括号冲突），且对人眼也是
      // 视觉显眼的"特别引用"标记。后接一个空格让用户继续敲不必手 space。
      const ref = `「${title}」 `;
      const ta = composeTextareaRef.current;
      if (!ta) {
        setInput((prev) => prev + ref);
        return;
      }
      const start = ta.selectionStart ?? ta.value.length;
      const end = ta.selectionEnd ?? ta.value.length;
      const cur = ta.value;
      const next = cur.slice(0, start) + ref + cur.slice(end);
      setInput(next);
      const newCursor = start + ref.length;
      // setTimeout 0 等 React 把 next 写入 textarea 再设光标，否则 selectionRange
      // 写在旧 value 上偏位。
      window.setTimeout(() => {
        const t = composeTextareaRef.current;
        if (t) {
          t.focus();
          t.setSelectionRange(newCursor, newCursor);
        }
      }, 0);
    },
    [setInput],
  );
  /// `@` picker 选中后：把 `@<query>` 段（从 mentionContext.start 到当前光标）
  /// 替换为 `「title」 `。`@` 本身也被吞掉 —— 与 ⌘K picker 输出的 ref token
  /// 字面量完全一致，下游 taskRefMap 解析 / 宠物侧 prompt 处理走同一条路径。
  const pickMention = useCallback(
    (title: string, ctx: { start: number; query: string }) => {
      const ref = `「${title}」 `;
      const ta = composeTextareaRef.current;
      const cur = ta?.value ?? input;
      const cursor = ta?.selectionStart ?? composeCursorPos;
      const next = cur.slice(0, ctx.start) + ref + cur.slice(cursor);
      setInput(next);
      const newCursor = ctx.start + ref.length;
      setComposeCursorPos(newCursor);
      setMentionSelectedIdx(0);
      window.setTimeout(() => {
        const t = composeTextareaRef.current;
        if (t) {
          t.focus();
          t.setSelectionRange(newCursor, newCursor);
        }
      }, 0);
    },
    [input, composeCursorPos, setInput],
  );

  // Session state
  const [sessionId, setSessionId] = useState<string>("");
  const [sessionTitle, setSessionTitle] = useState("新会话");
  const [sessionList, setSessionList] = useState<SessionMeta[]>([]);
  const [showSessionList, setShowSessionList] = useState(false);
  /// session 下拉的标题搜索 query。30+ session 用户找"那条关于 Downloads"
  /// 的会话时，title 子串 fuzzy 过滤比逐行扫快得多。与 chip filter（today /
  /// images / tasks）共生 —— 先 chip 过滤，再 title 过滤，AND 关系。
  /// 关闭下拉时自动清空（避免下次开 dropdown 时残留旧 query 让用户疑惑
  /// "为什么只显几条"）。
  const [sessionTitleQuery, setSessionTitleQuery] = useState("");
  useEffect(() => {
    if (!showSessionList) setSessionTitleQuery("");
  }, [showSessionList]);
  /// session 下拉行 hover 1s 浮 "最近 3 条" preview：lazy load_session 后
  /// cache last-3 items 到 previewCache，给 owner 跨 session 选择时不必先
  /// click 即可瞄一眼"这条 session 最后聊了什么"。timer / cache 与下拉
  /// 关闭一起 reset 避免脏 state；非当前 session 才显（当前 session 已在
  /// 主聊天区可见，preview 冗余）。
  const [previewSessionId, setPreviewSessionId] = useState<string | null>(null);
  const previewSessionTimerRef = useRef<number | null>(null);
  const [previewCache, setPreviewCache] = useState<Record<string, ChatItem[]>>(
    {},
  );
  const handleSessionPreviewEnter = useCallback(
    (sid: string) => {
      if (previewSessionTimerRef.current !== null) return;
      if (sid === sessionId) return; // 当前 session 不显
      previewSessionTimerRef.current = window.setTimeout(async () => {
        previewSessionTimerRef.current = null;
        if (!previewCache[sid]) {
          try {
            const session = await invoke<Session>("load_session", { id: sid });
            const last3 = (session.items ?? []).slice(-3);
            setPreviewCache((prev) => ({ ...prev, [sid]: last3 }));
          } catch (e) {
            console.error("session preview load failed:", e);
            return;
          }
        }
        setPreviewSessionId(sid);
      }, 1000);
    },
    [sessionId, previewCache],
  );
  const handleSessionPreviewLeave = useCallback(() => {
    if (previewSessionTimerRef.current !== null) {
      window.clearTimeout(previewSessionTimerRef.current);
      previewSessionTimerRef.current = null;
    }
    setPreviewSessionId(null);
  }, []);
  useEffect(() => {
    // 下拉关闭 → reset preview state 不让脏值跨开关闪现。cache 保留 ——
    // 下次开下拉同一 session hover 仍可命中。
    if (!showSessionList) {
      if (previewSessionTimerRef.current !== null) {
        window.clearTimeout(previewSessionTimerRef.current);
        previewSessionTimerRef.current = null;
      }
      setPreviewSessionId(null);
    }
  }, [showSessionList]);
  /// 非当前 session 的"已读时间戳"：sessionId → ISO timestamp（用户上次
  /// 浏览此 session 时的时间）。session list 渲染时与 s.updated_at 比，
  /// updated_at > lastSeen → 显蓝色小圆点 hint "有未读"。
  /// 首次启动：lastSeen 空 map → 所有 session 默认"已读"（不打扰新用户）。
  /// 仅当用户至少访问过一次某 session 后，该 session 才有可能再回头时显
  /// badge —— 与"用户读到哪了"的真实语义匹配。
  const [sessionLastSeen, setSessionLastSeen] = useState<Record<string, string>>(() => {
    try {
      const raw = window.localStorage.getItem("pet-chat-session-lastseen");
      if (!raw) return {};
      const obj = JSON.parse(raw);
      if (obj && typeof obj === "object" && !Array.isArray(obj)) {
        return obj as Record<string, string>;
      }
    } catch {
      // 解析失败 / localStorage 不可用：退到空 map（全部默认已读）
    }
    return {};
  });
  const markSessionSeen = useCallback((id: string) => {
    if (!id) return;
    const now = new Date().toISOString();
    setSessionLastSeen((prev) => {
      const next = { ...prev, [id]: now };
      try {
        window.localStorage.setItem(
          "pet-chat-session-lastseen",
          JSON.stringify(next),
        );
      } catch {
        // 私密浏览 / 容量满 —— UI state 仍生效，下次 reload 才丢
      }
      return next;
    });
  }, []);
  // session 下拉的内容过滤：互斥 enum，同时只能开一个 filter。null = 全显；
  // "images" / "tasks" 各自走对应后端命令；"today" / "pinned" 本地 derive。
  // filterSessionIds 是后端返回的 id set；loading 时 toggle pill 灰态防重复点。
  type SessionFilter = null | "images" | "tasks" | "today" | "pinned";
  const [sessionFilter, setSessionFilter] = useState<SessionFilter>(null);
  const [filterSessionIds, setFilterSessionIds] = useState<Set<string> | null>(null);
  const [filterLoading, setFilterLoading] = useState(false);
  const toggleSessionFilter = useCallback(
    async (next: Exclude<SessionFilter, null>) => {
      if (sessionFilter === next) {
        // 同一 chip 再点 → 关 filter。
        setSessionFilter(null);
        setFilterSessionIds(null);
        return;
      }
      setSessionFilter(next);
      // "today" 本地 derive，避开 invoke 往返：sessionList 已在内存，直接
      // 用本地日期前缀过滤 updated_at。
      if (next === "today") {
        const todayPrefix = new Date().toLocaleDateString("sv-SE");
        const ids = new Set(
          sessionList
            .filter((s) => s.updated_at.startsWith(todayPrefix))
            .map((s) => s.id),
        );
        setFilterSessionIds(ids);
        return;
      }
      // "pinned" 同 "today" 本地 derive：sessionList 已含 pinned 字段（后端
      // set_session_pinned + list_sessions 返回），无需再发 IPC。
      if (next === "pinned") {
        const ids = new Set(
          sessionList.filter((s) => s.pinned).map((s) => s.id),
        );
        setFilterSessionIds(ids);
        return;
      }
      setFilterSessionIds(null);
      setFilterLoading(true);
      try {
        const cmd =
          next === "images"
            ? "list_sessions_with_images"
            : "list_sessions_with_task_calls";
        const ids = await invoke<string[]>(cmd);
        // race 保护：用户切到另一个 filter 后才返回的 stale 结果不能覆盖当前
        // —— 用 sessionFilter ref 不方便，简单方案：返回时再次读 setSessionFilter
        // 当前值（通过 functional updater 形式）来对比。但 functional updater
        // 不能 await 后访问，所以这里走最弱保护：直接 setFilterSessionIds，
        // 用户切快导致的短暂闪烁可接受（filterSessionIds 在 sessionFilter 改
        // 变时也会被显式清空）。
        setFilterSessionIds(new Set(ids));
      } catch (e) {
        console.error("session filter fetch failed:", e);
      } finally {
        setFilterLoading(false);
      }
    },
    [sessionFilter, sessionList],
  );
  const [loaded, setLoaded] = useState(false);

  // 跨会话搜索状态。searchMode 开启时盖掉 session 下拉；query 实时（无 debounce）
  // 调 search_sessions —— IO 廉价，~50 sessions × ~200 items 全扫 < 100ms。
  // pendingScroll 在切换会话后由 layout effect 消费，把对应 item 滚到中间并
  // 短暂高亮。
  const [searchMode, setSearchMode] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  /// 跨会话搜索 keyword 历史。Enter 时 push；datalist 浮自动完成。与
  /// PanelMemory / PanelTasks 同款 useSearchHistory hook。
  const { history: chatSearchHistory, push: pushChatSearchHistory } =
    useSearchHistory("pet-chat-search-history");
  const [searchResults, setSearchResults] = useState<SearchHit[]>([]);
  // R96: 搜索范围。"all" 跨全部会话；"current" 只搜当前打开会话。退出 search
  // 模式时复位到 "all"，下次进 search 默认全局直觉一致。
  const [searchScope, setSearchScope] = useState<"all" | "current">("all");
  // R101: dropdown 行内 rename 状态。renamingId 非 null = 该 id 处于 edit
  // 模式（input 替代标题）；commit/cancel 后回 null。draft 是用户编辑中的
  // title，commit 时 trim 后落库；空白等价 cancel。
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  // R103: 长会话 scroll position > 200px 时浮 ↑ 按钮回顶。state 由 onScroll
  // 同步驱动；阈值 200 ≈ 2-3 条消息高度，少于此不显（用户接近顶部，没必要跳）。
  const [scrolledFromTop, setScrolledFromTop] = useState(false);
  /// 滚动条距底 > 200px 时显"↓ 跳到最新"浮动按钮 —— 用户翻历史时一键
  /// 回到当前。与既有 "↑ 跳到顶" 按钮垂直堆叠，↑ 在上方 ↓ 在下方贴合
  /// "箭头方向 = 滚动方向"的直觉。两个按钮 visibility 互补但不互斥：
  /// 中段滚动时可能同时浮（让用户两端都能跳）。
  const [scrolledFromBottom, setScrolledFromBottom] = useState(false);
  // R106: 单 session 导出后的短反馈文案。显在 dropdown 顶部 3s 自清空；
  // 单状态串行（同时只显一条），多次点击末次覆盖。
  const [exportToast, setExportToast] = useState("");

  /// 📋 复制最近 N 轮 dropdown：N ∈ {1, 5, 10, 20, 50}。click button → 弹 N
  /// 选项 popover；click 选项 → 取 items 末 N 条 user/assistant 拼带 glyph
  /// 前缀的 markdown 写剪贴板。tool / error / systemNote 行不计 — 与"对话
  /// 复盘"语义对齐。
  const [copyRecentMenuOpen, setCopyRecentMenuOpen] = useState(false);
  useEffect(() => {
    if (!copyRecentMenuOpen) return;
    const close = () => setCopyRecentMenuOpen(false);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setCopyRecentMenuOpen(false);
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [copyRecentMenuOpen]);
  const handleCopyRecentN = useCallback(
    async (n: number) => {
      setCopyRecentMenuOpen(false);
      const slice = items
        .filter(
          (it) =>
            (it.type === "user" || it.type === "assistant") &&
            !it.systemNote &&
            it.content.trim().length > 0,
        )
        .slice(-n);
      if (slice.length === 0) {
        setExportToast("当前 session 还没 user/assistant 消息可复制");
        window.setTimeout(() => setExportToast(""), 3000);
        return;
      }
      const text = slice
        .map((it) => {
          const glyph = it.type === "user" ? "🧑" : "🐾";
          return `${glyph} ${it.content.trim()}`;
        })
        .join("\n\n");
      try {
        await navigator.clipboard.writeText(text);
        setExportToast(`已复制最近 ${slice.length} 条 user/assistant`);
        window.setTimeout(() => setExportToast(""), 3000);
      } catch (e) {
        setExportToast(`复制失败: ${e}`);
        window.setTimeout(() => setExportToast(""), 3000);
      }
    },
    [items],
  );

  /// 全量 sessions 快照导出：base64 写剪贴板，复用 exportToast 通道反馈。
  /// 带安全提示（snapshot 含历史聊天明文，可能有敏感内容）。
  const handleExportSessionsSnapshot = useCallback(async () => {
    try {
      const payload = await invoke<string>("export_sessions_snapshot");
      await navigator.clipboard.writeText(payload);
      setExportToast(
        `已导出 ${payload.length} 字符到剪贴板 · ⚠ 含全部聊天明文，分享前请审`,
      );
      setTimeout(() => setExportToast(""), 6000);
    } catch (e) {
      setExportToast(`导出失败: ${e}`);
      setTimeout(() => setExportToast(""), 4000);
    }
  }, []);

  /// 清碎片 session：item_count ≤ 3 且非 pinned 非 active 的会话一键扫掉。
  /// armed 二次确认与 clearAll 同 5s 自动 disarm 模板。`fragCount` 是按当前
  /// sessionList 实时算的"可清掉"条数，让 owner 在按下前知道会清多少。
  const [purgeFragArmed, setPurgeFragArmed] = useState(false);
  const purgeFragArmTimerRef = useRef<number | null>(null);
  const handlePurgeFragmentSessions = useCallback(async () => {
    const fragCount = sessionList.filter(
      (s) => (s.item_count ?? 0) <= 3 && !s.pinned && s.id !== sessionId,
    ).length;
    if (!purgeFragArmed) {
      if (fragCount === 0) {
        setExportToast(
          "没有碎片 session 可清（碎片 = item_count ≤ 3、未钉住、非当前激活）。",
        );
        setTimeout(() => setExportToast(""), 4000);
        return;
      }
      setPurgeFragArmed(true);
      if (purgeFragArmTimerRef.current !== null) {
        window.clearTimeout(purgeFragArmTimerRef.current);
      }
      purgeFragArmTimerRef.current = window.setTimeout(() => {
        setPurgeFragArmed(false);
        purgeFragArmTimerRef.current = null;
      }, 5000);
      setExportToast(
        `⚠ 再点一次确认清掉 ${fragCount} 个碎片 session（5 秒内）。pinned / 当前会话保留。`,
      );
      setTimeout(() => setExportToast(""), 5000);
      return;
    }
    if (purgeFragArmTimerRef.current !== null) {
      window.clearTimeout(purgeFragArmTimerRef.current);
      purgeFragArmTimerRef.current = null;
    }
    setPurgeFragArmed(false);
    try {
      const deleted = await invoke<number>("purge_fragment_sessions");
      const index = await invoke<SessionIndex>("list_sessions");
      setSessionList(index.sessions);
      setExportToast(
        deleted === 0
          ? "没有碎片 session 被清（可能刚被其它窗口清掉了）"
          : `已清 ${deleted} 个碎片 session`,
      );
      setTimeout(() => setExportToast(""), 4000);
    } catch (e) {
      setExportToast(`清碎片失败: ${e}`);
      setTimeout(() => setExportToast(""), 4000);
    }
  }, [purgeFragArmed, sessionList, sessionId]);

  /// 全量清空 sessions：armed 二次确认。第一次点 → 显"再点确认清空 N 个 session"
  /// + 5s 自动 disarm；二次点 → invoke clear_all_sessions + 刷 sessionList +
  /// 切到新建的空 session。只清聊天历史，不动 memory / SOUL / config。
  const [clearAllArmed, setClearAllArmed] = useState(false);
  const clearAllArmTimerRef = useRef<number | null>(null);
  const handleClearAllSessions = useCallback(async () => {
    if (!clearAllArmed) {
      setClearAllArmed(true);
      if (clearAllArmTimerRef.current !== null) {
        window.clearTimeout(clearAllArmTimerRef.current);
      }
      clearAllArmTimerRef.current = window.setTimeout(() => {
        setClearAllArmed(false);
        clearAllArmTimerRef.current = null;
      }, 5000);
      setExportToast(
        `⚠ 再点一次确认清空全部 ${sessionList.length} 个 session（5 秒内）。仅清聊天历史，不动 memory / SOUL / config。`,
      );
      setTimeout(() => setExportToast(""), 5000);
      return;
    }
    if (clearAllArmTimerRef.current !== null) {
      window.clearTimeout(clearAllArmTimerRef.current);
      clearAllArmTimerRef.current = null;
    }
    setClearAllArmed(false);
    try {
      const deleted = await invoke<number>("clear_all_sessions");
      const index = await invoke<SessionIndex>("list_sessions");
      setSessionList(index.sessions);
      if (index.active_id) {
        await loadSession(index.active_id);
      }
      setExportToast(`已清空 ${deleted} 个 session · 起了一个新空会话`);
      setTimeout(() => setExportToast(""), 4000);
    } catch (e) {
      setExportToast(`清空失败: ${e}`);
      setTimeout(() => setExportToast(""), 4000);
    }
  }, [clearAllArmed, sessionList.length]);

  /// 全量 sessions 快照导入：armed 二次确认（覆盖 index + 重写每条 session 文件）。
  /// pruneSessionsOnImport 勾选时 disk 上不在 snapshot 里的 session.json 一并
  /// 删掉（让 import 是"新机器干净接收"而非两端混杂）。
  const [importSessionsArmed, setImportSessionsArmed] = useState(false);
  const [pruneSessionsOnImport, setPruneSessionsOnImport] = useState(false);
  const importSessionsPayloadRef = useRef<string>("");
  const importSessionsArmTimerRef = useRef<number | null>(null);
  const handleImportSessionsSnapshot = useCallback(async () => {
    if (!importSessionsArmed) {
      let payload = "";
      try {
        payload = await navigator.clipboard.readText();
      } catch (e) {
        setExportToast(`读剪贴板失败: ${e}`);
        setTimeout(() => setExportToast(""), 4000);
        return;
      }
      if (!payload.trim()) {
        setExportToast("剪贴板为空。先把 sessions snapshot 字符串复制过来再点导入。");
        setTimeout(() => setExportToast(""), 4000);
        return;
      }
      importSessionsPayloadRef.current = payload;
      setImportSessionsArmed(true);
      if (importSessionsArmTimerRef.current !== null) {
        window.clearTimeout(importSessionsArmTimerRef.current);
      }
      importSessionsArmTimerRef.current = window.setTimeout(() => {
        setImportSessionsArmed(false);
        importSessionsPayloadRef.current = "";
        importSessionsArmTimerRef.current = null;
      }, 5000);
      setExportToast(
        `⚠ 检测到 ${payload.length} 字符 snapshot，再点一次确认覆盖当前 session 索引（5 秒内）`,
      );
      setTimeout(() => setExportToast(""), 5000);
      return;
    }
    if (importSessionsArmTimerRef.current !== null) {
      window.clearTimeout(importSessionsArmTimerRef.current);
      importSessionsArmTimerRef.current = null;
    }
    setImportSessionsArmed(false);
    const payload = importSessionsPayloadRef.current;
    importSessionsPayloadRef.current = "";
    try {
      const prunedCount = await invoke<number>("import_sessions_snapshot", {
        payload,
        pruneOrphans: pruneSessionsOnImport,
      });
      // 刷新 session 列表；不自动切到某个 session（loadSession 在文件后段定义，
      // 且强切让用户失位 —— 让用户从下拉自行点）。dropdown 会重渲显新列表。
      const index = await invoke<SessionIndex>("list_sessions");
      setSessionList(index.sessions);
      const pruneNote =
        pruneSessionsOnImport && prunedCount > 0
          ? ` · 清理了 ${prunedCount} 个 orphan 文件`
          : "";
      setExportToast(
        `已导入 ${index.sessions.length} 个 session${pruneNote} · 点下拉里的会话切过去`,
      );
      setTimeout(() => setExportToast(""), 6000);
    } catch (e) {
      setExportToast(`导入失败: ${e}`);
      setTimeout(() => setExportToast(""), 4000);
    }
  }, [importSessionsArmed, pruneSessionsOnImport]);
  const [pendingScroll, setPendingScroll] = useState<number | null>(null);
  /// session tab 右键菜单：把 dropdown 里散在两步路径上的操作（pin / 重命名
  /// / 删除 / 复制标题）收敛到 tab 上一次右键即触发。x/y 是 viewport 坐标，
  /// fixed 浮窗。renamingId / pendingDeleteId 的 inline 态仍在 dropdown 里
  /// 显示，所以 rename / delete 触发时顺手 setShowSessionList(true) 让用户
  /// 看到对应控件。
  const [sessionTabCtxMenu, setSessionTabCtxMenu] = useState<
    | { id: string; title: string; pinned: boolean; x: number; y: number }
    | null
  >(null);
  // outside-click + Esc 关闭右键菜单。延迟一帧挂 mousedown 避免触发它的
  // 点击同时 close。
  useEffect(() => {
    if (!sessionTabCtxMenu) return;
    const close = () => setSessionTabCtxMenu(null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setSessionTabCtxMenu(null);
    };
    const id = window.setTimeout(() => {
      window.addEventListener("mousedown", close);
    }, 0);
    window.addEventListener("keydown", onKey);
    return () => {
      window.clearTimeout(id);
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [sessionTabCtxMenu]);

  /// "刚从 mini chat 双击进 panel"过渡视觉：sessionBar 黄底 1.2s 脉冲，
  /// 让用户进 panel 后立刻看到"我桌面聊的是这条 session"。两种触发路径：
  /// 1. Tauri event 'pet-focus-from-mini'（panel 已开时即时）
  /// 2. localStorage ts（panel 刚建首次时事件丢失，挂载 3s 内时戳算 fresh）
  /// 1.2s 自动落回常态。
  const [focusFromMiniPulse, setFocusFromMiniPulse] = useState(false);
  useEffect(() => {
    const trigger = () => {
      setFocusFromMiniPulse(true);
      window.setTimeout(() => setFocusFromMiniPulse(false), 1200);
    };
    // 挂载时读 localStorage 兜底：写入后 3s 内算 fresh，>3s 视为陈旧（
    // 上次开 panel 没读到 / 没切回 chat tab 等场景，避免长 stale 时戳乱触发）。
    try {
      const raw = window.localStorage.getItem("pet-focus-from-mini-ts");
      if (raw) {
        const ts = Number.parseInt(raw, 10);
        if (Number.isFinite(ts) && Date.now() - ts <= 3000) {
          trigger();
        }
        window.localStorage.removeItem("pet-focus-from-mini-ts");
      }
    } catch {
      // ignore: 私密浏览 / 容量满
    }
    let unlisten: (() => void) | null = null;
    listen("pet-focus-from-mini", () => {
      trigger();
    })
      .then((un) => {
        unlisten = un;
      })
      .catch(() => {
        // listener 注册失败不影响主流程，localStorage 路径仍可兜底
      });
    return () => {
      unlisten?.();
    };
  }, []);
  const [highlightedItemIdx, setHighlightedItemIdx] = useState<number | null>(null);
  // 跨会话搜索 hit 跳过去之后，让 matched item 的 bubble 内 keyword 段
  // mark 高亮（与 SearchResultRow 同黄色）。比 highlightedItemIdx 的 1.5s
  // 行级 background 高亮存活时间更长 —— 切到别的会话 / 再点别的 hit 才清，
  // 让用户慢慢读时仍能一眼看到命中位置。
  const [searchHit, setSearchHit] = useState<{ idx: number; keyword: string } | null>(null);
  // 复制按钮：刚被复制的 item idx（短暂展示"已复制"反馈），1.5s 自动清掉。
  // 用 idx 而非 boolean 让多条消息互不干扰（不会 A 复制后 B 也显示"已复制"）。
  const [copiedIdx, setCopiedIdx] = useState<number | null>(null);
  /// assistant 消息的反馈状态：idx → "liked" | "disliked" | "puzzled"。仅
  /// session 内存活（切 session 清空）—— 用户在当前会话内做的标记看得见，
  /// 切走再回来不再持续显示（feedback_history 已落盘留给 LLM 用，UI 不必
  /// 持久化各条状态，否则反复浏览容易误点重复入库）。
  const [reactionsByIdx, setReactionsByIdx] = useState<
    Record<number, "liked" | "disliked" | "puzzled">
  >({});
  const handleReact = useCallback(
    (idx: number, kind: "liked" | "disliked" | "puzzled", content: string) => {
      // 同 kind 重复点 = 切换 off；不同 kind = 覆盖
      setReactionsByIdx((prev) => {
        const next = { ...prev };
        if (next[idx] === kind) delete next[idx];
        else next[idx] = kind;
        return next;
      });
      // 仅在"新选 / 切换"路径下落盘；"切换 off"不写入（避免重复写无意义 entry）。
      // 落盘是 fire-and-forget：失败 console 警告即可，UI 已显选中状态。
      if (reactionsByIdx[idx] === kind) return;
      const excerpt = content.length > 200 ? content.slice(0, 200) : content;
      const cmd =
        kind === "liked"
          ? "record_bubble_liked"
          : kind === "disliked"
            ? "record_message_disliked"
            : "record_bubble_puzzled";
      invoke(cmd, { excerpt }).catch((e) =>
        console.error(`record reaction ${kind} failed:`, e),
      );
    },
    [reactionsByIdx],
  );

  // Slash 命令菜单：当输入处于 slash 模式（首字符 `/` 且未敲到参数空格）时
  // 浮窗可见。selectedSlashIdx 由键盘上下 / Enter 控制；点击命令项也写它。
  const [selectedSlashIdx, setSelectedSlashIdx] = useState(0);

  // shell-readline 风格多条历史召回。messageHistory 是 ring buffer
  // (cap 20, **newest at index 0**)；historyCursor null = 不在浏览模式，
  // 非 null = 当前浏览到第几条历史（0 = 最新）。
  // input 空 + ↑ → cursor=0 进入历史顶；继续 ↑ → cursor+1 往前（更旧）；
  // ↓ → cursor-1，cursor<0 退出 + 清空。
  // slash 命令不入历史（panel 控制流不算 chat content）。
  // 通过 chatHistoryStore 与 pet 窗 ChatPanel 共享 localStorage，跨窗口召回。
  const [messageHistory, setMessageHistory] = useState<string[]>(readSentHistory);
  const [historyCursor, setHistoryCursor] = useState<number | null>(null);

  // 多模态：粘贴板里的图片缓存（base64 data URL），发送时与文本拼成 multipart。
  // 数据全在前端 memory，用户点 ✕ 直接 splice 掉即可；非多模态模型下发送时
  // 由 is_current_model_multimodal 守门并提示。
  const [pendingImages, setPendingImages] = useState<string[]>([]);
  // 拖拽图片到 panel 时的高亮态。只在非零 dragenter 计数时为 true，避免
  // 子元素 enter/leave 抖动让 overlay 闪烁。
  const [dragActive, setDragActive] = useState(false);
  const dragDepthRef = useRef(0);

  // /clear 二次确认。首次 → armed + 5s 自动 disarm；armed 内再敲 /clear → 真清。
  // 误触 /clear 后仍能等 5s 看见自己没确认，会话内容不丢。
  const [clearArmed, setClearArmed] = useState(false);
  const clearArmTimerRef = useRef<number | null>(null);

  // compose 区缩略图条点击查看大图。CopyableMessage 自己管历史气泡的 lightbox；
  // 这里独立状态服务"发前预览"路径，与 ✕ 删图按钮共存（不挤位置）。
  const [composeLightboxSrc, setComposeLightboxSrc] = useState<string | null>(null);

  /// 多个 image blob → 异步读为 data URL → 推到 pendingImages。paste / drop
  /// 共用此路径，保持守门 + 缩略图条 + ✕ 移除 + 发送多模态化的行为完全一致。
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
  const slashPrefix = extractCommandPrefix(input);
  const filteredCommands: SlashCommand[] = useMemo(
    () => (slashPrefix === null ? [] : filterCommandsByPrefix(slashPrefix)),
    [slashPrefix],
  );
  const slashMenuVisible = slashPrefix !== null;
  // `@` inline picker：mentionContext 由 input + cursor 派生。slashMenuVisible
  // 优先级更高（输入开头是 `/` 时不抢菜单，避免两个菜单重叠）。
  const mentionContext = useMemo(
    () =>
      slashMenuVisible ? null : extractMentionContext(input, composeCursorPos),
    [input, composeCursorPos, slashMenuVisible],
  );
  const mentionFilteredTasks = useMemo(() => {
    if (!mentionContext) return [] as Array<{ title: string; status: string }>;
    const q = mentionContext.query.toLowerCase();
    const entries = Object.entries(chatTaskMap);
    if (q.length === 0) {
      return entries
        .slice(0, 30)
        .map(([title, v]) => ({ title, status: v.status }));
    }
    const scored: Array<{ title: string; status: string; score: number }> = [];
    for (const [title, v] of entries) {
      const s = fuzzyMatchTaskTitle(q, title.toLowerCase());
      if (s !== null) scored.push({ title, status: v.status, score: s });
    }
    scored.sort((a, b) => a.score - b.score);
    return scored.slice(0, 30).map(({ title, status }) => ({ title, status }));
  }, [mentionContext, chatTaskMap]);
  const mentionMenuVisible = mentionContext !== null;
  // 候选集变化时 clamp 选中下标（与 slash menu 同模式，避免 stale idx 越界）
  useEffect(() => {
    setMentionSelectedIdx((idx) => {
      if (mentionFilteredTasks.length === 0) return 0;
      return Math.min(idx, mentionFilteredTasks.length - 1);
    });
  }, [mentionFilteredTasks.length]);
  // prefix 变化时把选中项 clamp 回 0（避免上次选第 5 条但现在只剩 2 条）
  useEffect(() => {
    setSelectedSlashIdx((idx) => {
      if (filteredCommands.length === 0) return 0;
      return Math.min(idx, filteredCommands.length - 1);
    });
  }, [filteredCommands.length]);

  /// `/image` 历史 prompt 召回：input 匹配 `/image` / `/image ` / `/image <arg>`
  /// 时进入触发态。arg 部分（trim 后）用来对历史做 substring 模糊过滤；空 arg
  /// 显全部 5 条。0 匹配时菜单自动隐藏（让用户 compose 新 prompt 不被打扰）。
  const imagePromptMatch = input.match(/^\/image(?:\s+(.*))?$/i);
  const imagePromptArg = imagePromptMatch?.[1]?.trim() ?? "";
  const imagePromptTriggerActive = !!imagePromptMatch && !slashMenuVisible;
  const [allImagePrompts, setAllImagePrompts] = useState<ImagePromptEntry[]>([]);
  const [selectedImagePromptIdx, setSelectedImagePromptIdx] = useState(0);
  // 触发态开 / 关时刷新一次历史（避免每次输入都读 localStorage）。
  useEffect(() => {
    if (imagePromptTriggerActive) {
      setAllImagePrompts(readImagePrompts());
      setSelectedImagePromptIdx(0);
    }
  }, [imagePromptTriggerActive]);
  // arg 变化时把 idx clamp 回 0（用户敲新字符 → 候选集变 → 重新从顶选）
  useEffect(() => {
    setSelectedImagePromptIdx(0);
  }, [imagePromptArg]);
  const imagePromptHistory = useMemo(() => {
    if (imagePromptArg.length === 0) return allImagePrompts;
    const q = imagePromptArg.toLowerCase();
    return allImagePrompts.filter((e) => e.prompt.toLowerCase().includes(q));
  }, [allImagePrompts, imagePromptArg]);
  const imagePromptMenuVisible =
    imagePromptTriggerActive && imagePromptHistory.length > 0;

  /// 当前 session 的 token 估算：累加 items[].content 字符数 / 4（OpenAI 通用
  /// 经验比率，英文准、中文偏低估，但作为"会话长度感知"已够）。也把当前
  /// streaming chunk 算进去，让用户看到正在生成的内容也吃 context。超阈值
  /// 时角标变红 + 提示，让用户知道该开新会话或精简历史。
  const TOKEN_WARN = 8000;
  const TOKEN_CRIT = 24000;
  /// 点 token badge 后弹出"压缩历史"小确认。三档剪取（1/2 / 2/3 / 仅近 4），
  /// 选完调本地 trim helper 即时生效（不要求用户切 session）。outside-click
  /// / Esc 关闭。
  const [compactPromptOpen, setCompactPromptOpen] = useState(false);
  useEffect(() => {
    if (!compactPromptOpen) return;
    const close = () => setCompactPromptOpen(false);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setCompactPromptOpen(false);
    };
    const id = window.setTimeout(() => {
      window.addEventListener("mousedown", close);
    }, 0);
    window.addEventListener("keydown", onKey);
    return () => {
      window.clearTimeout(id);
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [compactPromptOpen]);

  const sessionTokensEstimate = useMemo(() => {
    let chars = 0;
    for (const it of items) {
      if (typeof it.content === "string") chars += it.content.length;
    }
    if (currentResponse) chars += currentResponse.length;
    // /4 是 OpenAI 文档常用粗估比；中文 1 char ≈ 1 token 偏高，但作为长度
    // 指标 OK，与 cl100k_base / o200k_base 实测差 ~20% 内
    return Math.round(chars / 4);
  }, [items, currentResponse]);

  /// 顶部 session 横排 tab 栏的展示集：pinned 永远前，剩余按最近活跃；最多
  /// 8 个 + 当前 session 必显。比 dropdown 浏览成本低，"常用 + 当前"一眼可
  /// 见；超出靠右侧 ⋯ 入下拉看全集。
  const MAX_SESSION_TABS = 8;
  const tabSessions = useMemo(() => {
    if (sessionList.length === 0) return [] as SessionMeta[];
    const reversed = [...sessionList].reverse();
    const pinned = reversed.filter((s) => s.pinned);
    const unpinned = reversed.filter((s) => !s.pinned);
    const ordered = [...pinned, ...unpinned];
    const top = ordered.slice(0, MAX_SESSION_TABS);
    // 当前 session 不在 top 8 → 把它挤到首位（其余整体右移一格保 cap）
    if (sessionId && !top.some((s) => s.id === sessionId)) {
      const current = ordered.find((s) => s.id === sessionId);
      if (current) return [current, ...top.slice(0, MAX_SESSION_TABS - 1)];
    }
    return top;
  }, [sessionList, sessionId]);

  /// 持久化 draft helper：纯函数包 localStorage 写读，try/catch 隔离 IO 错。
  const writeDraft = (id: string, text: string) => {
    try {
      if (text.length === 0) {
        window.localStorage.removeItem(`pet-chat-draft-${id}`);
      } else {
        window.localStorage.setItem(`pet-chat-draft-${id}`, text);
      }
    } catch {
      // 私密浏览 / 配额满；session 内仍能用，下次启动丢
    }
  };
  // input ref 让 sessionId 切换 effect 能拿到当前最新 input（state 是异步
  // 的，session 切换 commit 时 input 还是旧值，正好对应"旧 session 的草稿"）。
  const inputRef = useRef(input);
  useEffect(() => {
    inputRef.current = input;
  }, [input]);
  const prevSessionIdRef = useRef<string>("");
  /// ⌘B 切回上一会话的目标：每次 sessionId 切换时记下"被切走的那个 session"，
  /// 让 ⌘B 在两个 session 之间来回。与 prevSessionIdRef 区别：后者每次 switch
  /// 都会被覆盖成新 current，而 swapTargetRef 保留 "切换前的 session id"，
  /// 适合 keyboard swap 模式。empty string 表示"没有上一会话可切"。
  const swapTargetRef = useRef<string>("");
  /// 切 session 时若 prev session 有未发非空草稿，浮 5s toast 提示，让
  /// 用户知道"我刚才写了 N 字没发"。点击 toast 切回去继续写。已经 inputRef
  /// 拿了完整 prev 内容，title 从 sessionList 找；找不到（session 已删 /
  /// 重命名）显空白 fallback。toast id 用 timer ref 防多次切换叠加。
  const [draftReminder, setDraftReminder] = useState<{
    sessionId: string;
    title: string;
    charCount: number;
  } | null>(null);
  const draftReminderTimerRef = useRef<number | null>(null);

  // sessionId 切换：先把当前 input 立即写到 prev session 的 key（防 3s
  // debounce 未触发就丢稿），然后读新 session 的 draft 填到 textarea。
  useEffect(() => {
    if (!sessionId) return;
    const prevId = prevSessionIdRef.current;
    if (prevId && prevId !== sessionId) {
      // 记下被切走的会话 id，供 ⌘B 快速切回
      swapTargetRef.current = prevId;
      const prevDraft = inputRef.current;
      writeDraft(prevId, prevDraft);
      // 非空 trim 才 toast —— 仅有空白字符的"草稿"不打扰
      if (prevDraft.trim().length > 0) {
        const prevSession = sessionList.find((s) => s.id === prevId);
        setDraftReminder({
          sessionId: prevId,
          title: prevSession?.title ?? "（未知会话）",
          charCount: prevDraft.length,
        });
        if (draftReminderTimerRef.current !== null) {
          window.clearTimeout(draftReminderTimerRef.current);
        }
        draftReminderTimerRef.current = window.setTimeout(() => {
          setDraftReminder(null);
          draftReminderTimerRef.current = null;
        }, 5000);
      }
    }
    prevSessionIdRef.current = sessionId;
    try {
      const raw = window.localStorage.getItem(`pet-chat-draft-${sessionId}`);
      setInput(raw && raw.length > 0 ? raw : "");
    } catch {
      setInput("");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  // 在 session 内打字时 3s debounce 写盘。组件 unmount 时清 timer。
  useEffect(() => {
    if (!sessionId) return;
    if (draftWriteTimerRef.current !== null) {
      window.clearTimeout(draftWriteTimerRef.current);
    }
    draftWriteTimerRef.current = window.setTimeout(() => {
      writeDraft(sessionId, input);
      draftWriteTimerRef.current = null;
    }, 3000);
    return () => {
      if (draftWriteTimerRef.current !== null) {
        window.clearTimeout(draftWriteTimerRef.current);
        draftWriteTimerRef.current = null;
      }
    };
  }, [input, sessionId]);

  // Load sessions on mount
  useEffect(() => {
    (async () => {
      try {
        const index = await invoke<SessionIndex>("list_sessions");
        setSessionList(index.sessions);

        if (index.active_id && index.sessions.some((s) => s.id === index.active_id)) {
          await loadSession(index.active_id);
        } else if (index.sessions.length > 0) {
          // Load the most recent session
          const last = index.sessions[index.sessions.length - 1];
          await loadSession(last.id);
        } else {
          // No sessions, create one
          await handleNewSession();
        }
      } catch (e) {
        console.error("Failed to load sessions:", e);
        await handleNewSession();
      }
      setLoaded(true);
    })();
  }, []);

  useEffect(() => {
    // pendingScroll 优先：跨会话跳转时不要把视图甩到底，而是停在命中行。
    // 只要有命中要跳就跳过自动滚到底；scroll-to-match 由下一段 effect 处理。
    if (pendingScroll !== null) return;
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [items, currentResponse, currentToolCalls, pendingScroll]);

  // 跨会话搜索 → 实时 fetch。空查询清空结果（不调命令，避免无意义 IO）。
  useEffect(() => {
    if (!searchMode) return;
    const q = searchQuery.trim();
    if (!q) {
      setSearchResults([]);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        // R96: scope === "current" 时把 sessionId 传给后端，后端会跳过其它
        // 会话；不在前端 post-filter 是因为 limit=50 可能被别的 session
        // 吃满，前端筛会丢当前会话的命中。
        const args: { keyword: string; sessionId?: string } = { keyword: q };
        if (searchScope === "current") args.sessionId = sessionId;
        const hits = await invoke<SearchHit[]>("search_sessions", args);
        if (!cancelled) setSearchResults(hits);
      } catch (e) {
        console.error("search_sessions failed:", e);
        if (!cancelled) setSearchResults([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [searchMode, searchQuery, searchScope, sessionId]);

  // 跨窗口 deeplink chatMatch 消费：反向扫 items 找最近含 excerpt 的
  // user/assistant 行（substring + case-insensitive）→ setPendingScroll(idx)
  // 让既有 scrollIntoView + 高亮路径接管。tool / error 行的 content 可能含
  // 调用日志噪音，不匹配。找不到走 emit message note 让用户知道为啥没跳。
  // items 在 loadSession 后才填，所以 effect 依赖 [pendingChatMatch, items]。
  useEffect(() => {
    if (!pendingChatMatch) return;
    if (items.length === 0) return; // 等 loadSession 落 items 再消费
    const needle = pendingChatMatch.toLowerCase();
    let foundIdx = -1;
    for (let i = items.length - 1; i >= 0; i--) {
      const it = items[i];
      if (it.type !== "user" && it.type !== "assistant") continue;
      if (typeof it.content !== "string") continue;
      if (it.content.toLowerCase().includes(needle)) {
        foundIdx = i;
        break;
      }
    }
    if (foundIdx >= 0) {
      setPendingScroll(foundIdx);
    } else {
      pushLocalAssistantNote(
        `⛶ 没在本会话找到含「${pendingChatMatch.slice(0, 20)}${pendingChatMatch.length > 20 ? "…" : ""}」的消息（可能在别的 session）。`,
      );
    }
    onConsumePendingChatMatch?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingChatMatch, items.length]);

  /// pendingChatPrefill 消费：PanelTasks 🧠 按钮触发后，本 effect 拿到
  /// 预填字符串 → setInput + focus textarea + select-from-end 让 owner
  /// 直接在 prefix 后输入。一次性消费后 setNull 防 stale。
  useEffect(() => {
    if (!pendingChatPrefill) return;
    setInput(pendingChatPrefill);
    // 下一帧 focus textarea + 把 caret 移到末尾（owner 直接在 prefix 后敲问题）
    window.setTimeout(() => {
      const ta = composeTextareaRef.current;
      if (ta) {
        ta.focus();
        const end = ta.value.length;
        try {
          ta.setSelectionRange(end, end);
        } catch {
          // 极个别 webview 不支持 setSelectionRange；忽略，至少 focus 已成
        }
      }
    }, 0);
    onConsumePendingChatPrefill?.();
  }, [pendingChatPrefill, onConsumePendingChatPrefill]);

  // 切到目标会话后把目标 item scrollIntoView + 短暂高亮（1.5s 后清掉）。
  // 通过 data-item-idx 选择器找到 DOM 节点；找不到就当作 noop。
  useEffect(() => {
    if (pendingScroll === null) return;
    const idx = pendingScroll;
    // 跨渲染 frame 等 items 落到 DOM
    const id = window.setTimeout(() => {
      if (!scrollRef.current) {
        setPendingScroll(null);
        return;
      }
      const target = scrollRef.current.querySelector<HTMLElement>(
        `[data-item-idx="${idx}"]`,
      );
      if (target) {
        target.scrollIntoView({ block: "center", behavior: "smooth" });
        setHighlightedItemIdx(idx);
        window.setTimeout(() => setHighlightedItemIdx(null), 1500);
      }
      setPendingScroll(null);
    }, 50);
    return () => window.clearTimeout(id);
  }, [pendingScroll, items]);

  const loadSession = async (id: string) => {
    try {
      const session = await invoke<Session>("load_session", { id });
      setSessionId(session.id);
      setSessionTitle(session.title);
      setItems(session.items || []);
      messagesRef.current = session.messages || [];
      // 切 session 清掉 in-session 三键 reaction 高亮（feedback_history 已
      // 落盘，不需要前端记忆）
      setReactionsByIdx({});
      // 标记此 session 已读到当前时间 —— session list 的 unread badge 基线
      markSessionSeen(session.id);
    } catch (e) {
      console.error("Failed to load session:", e);
    }
  };

  // 当前 session 的 items 变（用户在读时收到新消息 / 自己发了消息）→ 同步
  // 推进 lastSeen，让 list badge 不会误标自己正在看的会话。sessionId 切换
  // 时 loadSession 已经标记过；这里覆盖"持续观看时的增量"。
  useEffect(() => {
    if (sessionId) markSessionSeen(sessionId);
  }, [sessionId, items, markSessionSeen]);

  const saveCurrentSession = useCallback(
    async (newItems: ChatItem[]) => {
      if (!sessionId) return;

      // Auto-generate title from first user message
      let title = sessionTitle;
      if (title === "新会话") {
        const firstUser = newItems.find((i) => i.type === "user");
        if (firstUser) {
          title = firstUser.content.slice(0, 20) + (firstUser.content.length > 20 ? "..." : "");
          setSessionTitle(title);
        }
      }

      const now = new Date().toISOString();
      const session: Session = {
        id: sessionId,
        title,
        created_at: "", // preserved by backend
        updated_at: now,
        messages: messagesRef.current,
        // systemNote 是 slash 命令本地反馈，渲染层 subdued + 导出已过滤，
        // 持久化层也跟齐 —— 不污染回读时的对话历史 / 跨会话搜索。
        items: newItems.filter((it) => !it.systemNote),
      };

      try {
        await invoke("save_session", { session });
        // Refresh session list to reflect updated title
        const index = await invoke<SessionIndex>("list_sessions");
        setSessionList(index.sessions);
      } catch (e) {
        console.error("Failed to save session:", e);
      }
    },
    [sessionId, sessionTitle],
  );

  /// 压缩历史：丢掉前 N 条 items + 同步丢同样数量的非 system messages。
  /// 保留 system 让 SOUL.md / prompt 角色定义不丢。最少留 lastKeep 条让
  /// 上下文有锚（不至于"清完后 LLM 一脸懵"）。session 文件原地覆盖。
  ///
  /// 压缩前自动调 export_sessions_snapshot 把所有 session 备份写到剪贴板，
  /// 用户想反悔 → 点 📥 导入快照 即可恢复。备份失败仍会继续压缩（trim 是
  /// 用户主诉，备份是保险），失败原因在 toast 里提示。
  const compactHistory = useCallback(
    async (lastKeep: number) => {
      const remaining = Math.max(0, Math.min(items.length, lastKeep));
      const trimCount = items.length - remaining;
      if (trimCount <= 0) {
        setCompactPromptOpen(false);
        return;
      }
      // 1) 备份当前所有 sessions 到剪贴板（用 base64 snapshot 与既有"📥
      //    导入快照"路径对偶）。fire-and-await：失败不阻塞 trim，仅在 toast
      //    里提示用户。
      let backupOk = false;
      let backupErr = "";
      try {
        const payload = await invoke<string>("export_sessions_snapshot");
        await navigator.clipboard.writeText(payload);
        backupOk = true;
      } catch (e) {
        backupErr = String(e);
      }

      const nextItems = items.slice(trimCount);
      // messagesRef：保留 system，再保留尾部 lastKeep 条非 system 消息。
      // tool 调用对（assistant tool_calls + tool response）可能在尾部被
      // 截断；接受这点不一致，让用户优先获得 token 削减。
      const sysIdx = messagesRef.current.findIndex(
        (m) => m && typeof m === "object" && m.role === "system",
      );
      let sys: any = null;
      let rest: any[] = messagesRef.current;
      if (sysIdx >= 0) {
        sys = messagesRef.current[sysIdx];
        rest = messagesRef.current.filter((_, i) => i !== sysIdx);
      }
      const restKept = rest.slice(Math.max(0, rest.length - lastKeep));
      messagesRef.current = sys ? [sys, ...restKept] : restKept;
      setItems(nextItems);
      await saveCurrentSession(nextItems);
      setCompactPromptOpen(false);
      const backupNote = backupOk
        ? " · 压缩前快照已复制到剪贴板（想反悔点 📥 导入快照）"
        : ` · ⚠ 备份失败：${backupErr}`;
      setExportToast(
        `已压缩 ${trimCount} 条早期消息，保留近 ${nextItems.length} 条${backupNote}`,
      );
      window.setTimeout(() => setExportToast(""), 6000);
    },
    [items, saveCurrentSession],
  );

  const handleNewSession = async () => {
    try {
      const session = await invoke<Session>("create_session");
      setSessionId(session.id);
      setSessionTitle(session.title);
      setItems([]);
      messagesRef.current = session.messages;
      setShowSessionList(false);

      // Refresh session list
      const index = await invoke<SessionIndex>("list_sessions");
      setSessionList(index.sessions);
    } catch (e) {
      console.error("Failed to create session:", e);
    }
  };

  const switchSession = async (id: string) => {
    await loadSession(id);
    setShowSessionList(false);
  };

  /// fork 当前 session：复制末尾 keepN 条 items + 对应 messages 到新 session。
  /// keepN = Infinity → 整段复制（除了 sessionId / created_at / updated_at 由后端
  /// 自动生成）。session title 加 " · fork" 后缀让用户能区分。
  ///
  /// messages 端：保留 system + 末尾 keepN 条非 system 消息（与压缩历史同算法）。
  /// 让 LLM 在新 session 里能看到末尾对话语境，从这里"分叉"继续讨论。
  const handleForkSession = useCallback(
    async (keepN: number) => {
      if (!sessionId) return;
      try {
        const newSession = await invoke<Session>("create_session");
        const sourceItems = items.slice(
          keepN === Infinity ? 0 : Math.max(0, items.length - keepN),
        );
        const sourceMessages = messagesRef.current;
        const sysIdx = sourceMessages.findIndex(
          (m) => m && typeof m === "object" && m.role === "system",
        );
        let sys: any = null;
        let rest: any[] = sourceMessages;
        if (sysIdx >= 0) {
          sys = sourceMessages[sysIdx];
          rest = sourceMessages.filter((_, i) => i !== sysIdx);
        }
        const messagesTail =
          keepN === Infinity
            ? rest
            : rest.slice(Math.max(0, rest.length - keepN));
        const forkedMessages = sys ? [sys, ...messagesTail] : messagesTail;
        // 后端 newSession.title 默认是 "新会话"；用源 session title + " · fork"
        // 让 dropdown / tab 一眼区分。空 sourceTitle 走 fallback。
        const forkTitle =
          (sessionTitle && sessionTitle !== "新会话"
            ? `${sessionTitle} · fork`
            : `Fork ${new Date().toLocaleTimeString().slice(0, 5)}`).slice(0, 60);
        const forked: Session = {
          ...newSession,
          title: forkTitle,
          // 同 saveCurrentSession 的过滤策略：systemNote 仅是 slash 命令本地
          // 反馈，fork 出新会话时不该把这些"控制流回执"带过去。
          items: sourceItems.filter((it) => !it.systemNote),
          messages: forkedMessages,
        };
        await invoke("save_session", { session: forked });
        // 刷 list + 切到新 fork session
        const index = await invoke<SessionIndex>("list_sessions");
        setSessionList(index.sessions);
        await loadSession(forked.id);
        setExportToast(
          `已 fork ${sourceItems.length} 条到新 session「${forkTitle}」`,
        );
        window.setTimeout(() => setExportToast(""), 4000);
      } catch (e) {
        setExportToast(`fork 失败：${e}`);
        window.setTimeout(() => setExportToast(""), 4000);
      }
    },
    [sessionId, sessionTitle, items],
  );
  /// fork popover open 标志 + outside-click 关。
  const [forkPopoverOpen, setForkPopoverOpen] = useState(false);
  useEffect(() => {
    if (!forkPopoverOpen) return;
    const close = () => setForkPopoverOpen(false);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setForkPopoverOpen(false);
    };
    const id = window.setTimeout(() => {
      window.addEventListener("mousedown", close);
    }, 0);
    window.addEventListener("keydown", onKey);
    return () => {
      window.clearTimeout(id);
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [forkPopoverOpen]);

  /// 把一条本地"系统反馈" message 推到 items（不发给 LLM），用于 `/help` /
  /// `/clear` 后的提示 / 未知命令的错误反馈 / 各种 slash 命令的执行回执。
  /// type 选 assistant 让它出现在 bubble 区域，但置 systemNote=true 让渲染
  /// 走 subdued 样式（小字 / 虚线 / 半透明）与真 LLM 回复区分；markdown 导出
  /// 也会过滤掉这些（不算用户与 AI 的对话内容）。
  const pushLocalAssistantNote = useCallback((text: string) => {
    setItems((prev) => [...prev, { type: "assistant", content: text, systemNote: true }]);
  }, []);

  /// PanelChat 内复用的 `/reset` 逻辑：把 messagesRef 砍到 system-only，保留
  /// 可见 items，立即 save_session 落盘。chip onClick / slash 命令两条路径共
  /// 享同一份。流式中拒绝（与 slash 同保护）；已 system-only 时 noop 反馈。
  const handleResetLlmContext = useCallback(async () => {
    if (isLoading) {
      pushLocalAssistantNote(
        "⚠️ 正在流式回复中；先等完成或 Esc 取消，再 /reset。",
      );
      return;
    }
    const sysOnly = messagesRef.current.filter((m) => m?.role === "system");
    if (sysOnly.length === messagesRef.current.length) {
      pushLocalAssistantNote(
        "🧠 LLM 上下文本就是干净的（只剩 system 人设）。",
      );
      return;
    }
    const droppedCount = messagesRef.current.length - sysOnly.length;
    messagesRef.current = sysOnly;
    try {
      await invoke("save_session", {
        session: {
          id: sessionId,
          title: sessionTitle,
          created_at: "",
          updated_at: new Date().toISOString(),
          messages: sysOnly,
          items,
        },
      });
    } catch (e) {
      console.error("Failed to save reset session:", e);
    }
    pushLocalAssistantNote(
      `🧠 已清掉 ${droppedCount} 条 LLM 上下文（保留可见历史 + system 人设）；下一条消息就是干净的 turn 1。`,
    );
  }, [isLoading, items, sessionId, sessionTitle, pushLocalAssistantNote]);

  /// 当前 session 累积 LLM context token 量。60 秒轮一次，与 ChatMini /
  /// DebugApp 同源信号（get_active_session_context_stats）+ 同阈值（4000）。
  /// > 阈值 → 顶部 chip 浮出 + 一键 /reset 入口。IPC 失败兜 0 = chip 不显。
  const [sessionTokens, setSessionTokens] = useState<number>(0);
  useEffect(() => {
    let alive = true;
    const fetchOnce = async () => {
      try {
        const stats = await invoke<{ tokens: number }>(
          "get_active_session_context_stats",
        );
        if (alive) setSessionTokens(stats.tokens);
      } catch (e) {
        console.error("get_active_session_context_stats failed:", e);
      }
    };
    void fetchOnce();
    const id = window.setInterval(fetchOnce, 60_000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);

  /// `/image` 通用执行：替换 `replaceAtIdx` 处的 item 为 pending 占位（idx<0
  /// 则 append），调后端 image_generate；成功 → 渲染图片的 assistant 行；
  /// 失败 → 错误说明 + `imageRetryPrompt` 让 UI 渲出重试按钮。pendingNote 用
  /// 闭包引用 + content 比对双保险定位。
  const runImageGenerate = useCallback(
    (
      prompt: string,
      replaceAtIdx: number,
      n: number = 1,
      sizeOverride: string | null = null,
    ) => {
      const nLabel = n > 1 ? `（-n ${n}）` : "";
      const sizeLabel = sizeOverride ? `（${sizeOverride}）` : "";
      const pendingNote: ChatItem = {
        type: "assistant",
        content: `🎨 正在生成图片${nLabel}${sizeLabel}：${prompt} …`,
      };
      setItems((prev) => {
        const next = [...prev];
        if (replaceAtIdx >= 0 && replaceAtIdx < next.length) {
          next[replaceAtIdx] = pendingNote;
        } else {
          next.push(pendingNote);
        }
        return next;
      });
      const finishWith = (replacement: ChatItem) => {
        setItems((prev) => {
          const idx = prev.findIndex(
            (it) =>
              it === pendingNote ||
              (it.type === "assistant" && it.content === pendingNote.content),
          );
          if (idx < 0) return prev;
          const next = [...prev];
          next[idx] = replacement;
          void saveCurrentSession(next);
          return next;
        });
      };
      (async () => {
        try {
          const result = await invoke<{ urls: string[]; errors: string[] }>(
            "image_generate",
            { prompt, n, size: sizeOverride ?? undefined },
          );
          // 三档结果：全成 / 部分成 / 全败。全败时走失败行 + 重试按钮，与原
          // 错误路径一致；部分成走带图 + 错误段一起显，让用户看到画了几张
          // 还有几条原因。
          if (result.urls.length === 0) {
            finishWith({
              type: "assistant",
              content: `🎨 图片生成失败${nLabel}${sizeLabel}：${result.errors.join("; ") || "未知"}`,
              imageRetryPrompt: prompt,
              imageRetryN: n > 1 ? n : undefined,
              imageRetrySize: sizeOverride,
            });
          } else {
            const partialNote =
              result.errors.length > 0
                ? `\n\n⚠ ${result.errors.length}/${n} 失败：${result.errors.join("; ")}`
                : "";
            const countLabel =
              n > 1 ? `（${result.urls.length}/${n} 张）` : nLabel;
            finishWith({
              type: "assistant",
              content: `🎨 ${prompt}${countLabel}${sizeLabel}${partialNote}`,
              images: result.urls,
            });
            // 拿首图 canvas 压成 64px 缩略图后回填到 /image 历史菜单条目。
            // fire-and-forget：失败仅 console；菜单下次打开会用 readImagePrompts
            // 重读，所以这里也顺手 refresh in-memory 数据让正在打开的菜单立
            // 即看到新缩略图（用户连续 /image 时切换 prompt 不必关菜单）。
            const firstUrl = result.urls[0];
            if (firstUrl) {
              makeImagePromptThumb(firstUrl)
                .then((thumb) => {
                  attachThumbToImagePrompt(prompt, thumb);
                  setAllImagePrompts(readImagePrompts());
                })
                .catch((e) =>
                  console.error("attach image prompt thumb failed:", e),
                );
            }
          }
        } catch (e) {
          finishWith({
            type: "assistant",
            content: `🎨 图片生成失败${nLabel}${sizeLabel}：${e}`,
            imageRetryPrompt: prompt,
            imageRetryN: n > 1 ? n : undefined,
            imageRetrySize: sizeOverride,
          });
        }
      })();
    },
    [saveCurrentSession],
  );

  /// 执行已 parse 出的 slash action。命令在前端拦截，**不**走 LLM。`/clear`
  /// 与 `/sleep` 涉及后端持久化，其它纯 UI 切换。
  const executeSlash = useCallback(
    async (action: SlashAction) => {
      // 记一次使用频次，让 slash menu 排序按用户偏好刷新。incomplete / unknown
      // 不计 —— 只有"用户成功执行了某个真命令"才算偏好信号。
      // clear 的二次确认首发也算"使用一次"，避免要点两次才能学到偏好；用户
      // 反复用 /clear 时 score 自然累加。
      if (
        action.kind !== "incomplete" &&
        action.kind !== "unknown" &&
        action.kind !== "imageHelp"
      ) {
        // imageHelp 是 /image 的子模式，不在 SLASH_COMMANDS 里 —— 记了也没用
        // 还会污染 history 排序 score map。
        recordSlashCommandUsage(action.kind);
      }
      switch (action.kind) {
        case "clear": {
          // 二次确认：未 armed → 设 armed + 5s 计时 + 提示；armed → 真清。
          if (!clearArmed) {
            setClearArmed(true);
            if (clearArmTimerRef.current !== null) {
              window.clearTimeout(clearArmTimerRef.current);
            }
            clearArmTimerRef.current = window.setTimeout(() => {
              setClearArmed(false);
              clearArmTimerRef.current = null;
            }, 5000);
            pushLocalAssistantNote("⚠️ 再敲一次 /clear 确认清空当前会话（5 秒内）。");
            break;
          }
          // 真执行清空：取消计时、disarm，再走原清空逻辑。
          if (clearArmTimerRef.current !== null) {
            window.clearTimeout(clearArmTimerRef.current);
            clearArmTimerRef.current = null;
          }
          setClearArmed(false);
          // 清空当前 session 的 items / messages（保留 system soul），写盘。
          // 不删 session 文件，session id 不变；用户回看历史时该 session 会显示
          // 空内容。比"新建会话"少一次列表 churn。
          const soul = messagesRef.current.find((m) => m?.role === "system")?.content ?? "";
          const sysMsg = { role: "system", content: soul };
          messagesRef.current = [sysMsg];
          setItems([]);
          setCurrentResponse("");
          setCurrentToolCalls([]);
          await invoke("save_session", {
            session: {
              id: sessionId,
              title: sessionTitle,
              created_at: "",
              updated_at: new Date().toISOString(),
              messages: [sysMsg],
              items: [],
            },
          }).catch((e) => console.error("Failed to save cleared session:", e));
          break;
        }
        case "repeat": {
          // IM 风便利：再发一遍上一条 user 消息（与 ⌥↑ + Enter 等价但少一步）。
          // 用例：宠物回的不满意想再试一次；或网络刚才半截、想重跑同样的
          // 输入。**不丢历史**：直接 append 一条新 user turn 与原 user 同
          // content + images；与 IM 系应用 "long-press → resend" 行为一致。
          // 流式中拒绝（与 /reset 同语义边界）；无 user item 时也明确反馈。
          if (isLoading) {
            pushLocalAssistantNote(
              "⚠️ 正在流式回复中；先等完成或 Esc 取消，再 /repeat。",
            );
            break;
          }
          let lastUser: ChatItem | null = null;
          for (let i = items.length - 1; i >= 0; i--) {
            const it = items[i];
            if (it.type === "user") {
              lastUser = it;
              break;
            }
          }
          if (!lastUser) {
            pushLocalAssistantNote(
              "⚠️ 当前会话还没有 user 消息可以 /repeat。",
            );
            break;
          }
          // sendMessage 会自动 push 新 user item + 触发 LLM stream；不必
          // 手动操作 items / messagesRef。
          void sendMessage(lastUser.content, lastUser.images);
          break;
        }
        case "reset": {
          // 与 TG `/reset` 对偶：清掉 LLM 上下文但保留可见 items。chip onClick
          // 走同一条 path（handleResetLlmContext）—— 流式守门 / system-only
          // noop / save_session 行为完全一致。
          await handleResetLlmContext();
          break;
        }
        case "tasks": {
          if (onRequestTab) {
            onRequestTab("任务");
          } else {
            pushLocalAssistantNote("当前面板未提供切标签能力（onRequestTab 未注入）。");
          }
          break;
        }
        case "today": {
          // 今日叙事视图：到期 / 已完成的任务标题清单。与 /stats（数字汇总）
          // 互补 —— /stats 看结构性指标，/today 直接看"今天该干嘛 / 搞定了啥"
          // 的具体清单。每段 cap 5 条 + 溢出提示。
          try {
            const resp = await invoke<{
              tasks: Array<{
                title: string;
                status: string;
                due: string | null;
                updated_at: string;
              }>;
            }>("task_list");
            const todayPrefix = new Date().toLocaleDateString("sv-SE");
            const todayDisplay = todayPrefix;
            const dueToday: Array<{ title: string; due: string }> = [];
            const doneToday: string[] = [];
            for (const t of resp.tasks) {
              if (t.status === "pending" && t.due && t.due.slice(0, 10) === todayPrefix) {
                dueToday.push({ title: t.title, due: t.due });
              } else if (
                t.status === "done" &&
                t.updated_at.startsWith(todayPrefix)
              ) {
                doneToday.push(t.title);
              }
            }
            const lines: string[] = [`📅 今日（${todayDisplay}）`];
            if (dueToday.length === 0 && doneToday.length === 0) {
              lines.push("");
              lines.push("今日队列清爽 ✨，可 /stats 看整体队列。");
            } else {
              const renderBucket = (header: string, items: string[]) => {
                if (items.length === 0) return;
                lines.push("");
                lines.push(`${header}（${items.length}）：`);
                for (const it of items.slice(0, 5)) lines.push(`· ${it}`);
                if (items.length > 5) lines.push(`…还有 ${items.length - 5} 条`);
              };
              // due 行带 HH:MM 时间后缀；HH:MM 来自 due 末尾（格式 "YYYY-MM-DDTHH:MM"）
              const dueLines = dueToday.map((t) => {
                const hm = t.due.length >= 16 ? t.due.slice(11, 16) : "";
                return hm ? `${t.title} · ${hm}` : t.title;
              });
              renderBucket("今日到期", dueLines);
              renderBucket("今日已完成", doneToday);
            }
            pushLocalAssistantNote(lines.join("\n"));
          } catch (e) {
            pushLocalAssistantNote(`/today 失败：${e}`);
          }
          break;
        }
        case "pin": {
          // toggle 当前 session 的 pinned。从 sessionList 查当前 pinned 状态，
          // 反转后写回；sessionList 由 set_session_pinned 后的 list_sessions 拉回
          // 真值，避免 race。
          try {
            const cur = sessionList.find((s) => s.id === sessionId);
            const wasPinned = !!cur?.pinned;
            await invoke("set_session_pinned", { id: sessionId, pinned: !wasPinned });
            const idx = await invoke<SessionIndex>("list_sessions");
            setSessionList(idx.sessions);
            pushLocalAssistantNote(
              wasPinned ? "📌 已取消钉住本会话" : "📌 已钉住本会话",
            );
          } catch (e) {
            pushLocalAssistantNote(`/pin 失败：${e}`);
          }
          break;
        }
        case "new": {
          // 一键新会话；可选 [initial title]。空 arg 等价点 ＋ 新建按钮
          // （留 "新会话" 默认 → 首条 user 消息后 auto-title 会接管）。
          // 非空 arg → create_session 后立即 save 改 title；防 auto-title 覆盖。
          try {
            const session = await invoke<Session>("create_session");
            const newTitle = action.query.trim();
            setSessionId(session.id);
            setSessionTitle(newTitle || session.title);
            setItems([]);
            messagesRef.current = session.messages;
            setShowSessionList(false);
            if (newTitle) {
              // 第一秒就把 title 改成用户给的，避免后续 auto-title 把它替成
              // first user message 前 20 字（commitSave 那条只在 sessionTitle
              // === "新会话" 时触发；这里立刻覆盖到非默认值即可）。
              const fresh = await invoke<Session>("load_session", { id: session.id });
              fresh.title = newTitle;
              await invoke("save_session", { session: fresh });
            }
            const idx = await invoke<SessionIndex>("list_sessions");
            setSessionList(idx.sessions);
            pushLocalAssistantNote(
              newTitle ? `✨ 已新建会话「${newTitle}」` : "✨ 已新建会话",
            );
          } catch (e) {
            pushLocalAssistantNote(`/new 失败：${e}`);
          }
          break;
        }
        case "title": {
          // 改当前 session title。直接 load → 改 title → save → refresh list →
          // 更新本地 sessionTitle state。与 dropdown rename 三件套同 IO 路径，
          // 不走 renamingId state（slash 命令是 inline 命令，不需要 inline editor）。
          const newTitle = action.query.trim();
          if (!newTitle) {
            pushLocalAssistantNote("⚠️ 用法：/title <新标题>");
            break;
          }
          try {
            const session = await invoke<Session>("load_session", { id: sessionId });
            session.title = newTitle;
            await invoke("save_session", { session });
            const idx = await invoke<SessionIndex>("list_sessions");
            setSessionList(idx.sessions);
            setSessionTitle(newTitle);
            pushLocalAssistantNote(`📝 已改名为「${newTitle}」`);
          } catch (e) {
            pushLocalAssistantNote(`/title 失败：${e}`);
          }
          break;
        }
        case "clearstats": {
          // 清掉 slash 命令使用历史。注意：本 case 顶部已 record 一次本 clearstats
          // 的 usage，所以清完后立刻又会写一条 score=1 的 clearstats entry —— 这
          // 是符合预期的"刚用过 clearstats 也是最近使用"，不抹掉。
          clearSlashScores();
          pushLocalAssistantNote(
            "🧹 已清掉 slash 命令使用历史。/help 与菜单的排序回到声明默认序。",
          );
          break;
        }
        case "version": {
          // 聊天行内的"印一行版本信息"用法 —— 与 Settings chip / PanelDebug
          // 快照同源（app_version + get_db_stats.schema_version + navigator.platform），
          // 但更紧凑，不带 `app:` `schema:` 前缀，3 行内贴完。
          try {
            const [v, s] = await Promise.all([
              invoke<string>("app_version").catch(() => ""),
              invoke<{ schema_version: number }>("get_db_stats")
                .then((d) => d.schema_version)
                .catch(() => 0),
            ]);
            const plat = typeof navigator !== "undefined" ? navigator.platform : "";
            const lines: string[] = [];
            lines.push(v ? `🐾 pet v${v}` : "🐾 pet（版本号缺失）");
            if (s > 0) lines.push(`schema v${s}`);
            if (plat) lines.push(`平台 ${plat}`);
            pushLocalAssistantNote(lines.join("\n"));
          } catch (e) {
            pushLocalAssistantNote(`/version 失败：${e}`);
          }
          break;
        }
        case "whoami": {
          // 宠物自我介绍：把四个 IPC 读源 (companionship / mood / persona_summary
          // / top_tools) 并发 fetch 然后排版。每段失败独立兜底（不让某个源
          // 挂掉导致整段不渲染） —— Promise.allSettled 而非 all。
          try {
            const userNameP = invoke<string>("get_user_name").catch(() => "");
            const daysP = invoke<number>("get_companionship_days").catch(() => null);
            const moodP = invoke<{
              text: string;
              motion: string | null;
              raw: string;
            }>("get_current_mood").catch(() => null);
            const personaP = invoke<{ text: string; updated_at: string }>(
              "get_persona_summary",
            ).catch(() => null);
            const toolsP = invoke<
              Array<{ name: string; count: number; last_used_at: string }>
            >("get_top_tools_used").catch(() => [] as Array<{
              name: string;
              count: number;
              last_used_at: string;
            }>);
            const [userName, days, mood, persona, tools] = await Promise.all([
              userNameP,
              daysP,
              moodP,
              personaP,
              toolsP,
            ]);
            const lines: string[] = ["🪪 **/whoami**"];
            if (userName && userName.trim()) {
              lines.push(`🐾 我叫你「${userName.trim()}」。`);
            }
            if (typeof days === "number") {
              lines.push(
                days === 0
                  ? "📅 今天与你初识。"
                  : `📅 与你相伴已 ${days} 天。`,
              );
            }
            if (mood && mood.raw !== "") {
              const moodText = mood.text.trim();
              if (moodText) {
                lines.push(
                  mood.motion?.trim()
                    ? `💗 现在的心情：${moodText} · 动作组 ${mood.motion.trim()}`
                    : `💗 现在的心情：${moodText}`,
                );
              }
            }
            if (persona && persona.text.trim()) {
              // 自我画像 head：首段（首个空行前），最多 ~90 字截断防过长。
              const first = persona.text.split(/\n\s*\n/, 1)[0]?.trim() ?? "";
              if (first) {
                const head = first.length > 90 ? first.slice(0, 90) + "…" : first;
                lines.push(`🪞 自我画像：${head}`);
              }
            }
            if (tools.length > 0) {
              // top 3 让一行清单不臃肿。count 显示是为了让"频次差"被看见
              // （某工具用了 12 次 vs 用了 1 次的语义不同）。
              const top3 = tools.slice(0, 3);
              const segs = top3.map((t) => `\`${t.name}\`×${t.count}`).join(" · ");
              lines.push(`🛠 近常用工具：${segs}`);
            }
            if (lines.length === 1) {
              // 所有源都空 —— 刚装机 / 全清状态，给一个温和兜底
              lines.push("🐾 还没攒到自我介绍的素材，先一起聊聊吧。");
            }
            pushLocalAssistantNote(lines.join("\n"));
          } catch (e) {
            pushLocalAssistantNote(`/whoami 失败：${e}`);
          }
          break;
        }
        case "mood": {
          // 与 TG `/mood` 同三态：无记录 / 含 motion / 不含 motion。后端
          // `get_current_mood` 返回 CurrentMood{text, motion, raw}；raw === ""
          // 区分"没记过" vs "记了空字符串"。
          try {
            const m = await invoke<{
              text: string;
              motion: string | null;
              raw: string;
            }>("get_current_mood");
            if (m.raw === "") {
              pushLocalAssistantNote(
                "🐾 宠物还没记心情；一会儿主动开口时会写一笔。",
              );
              break;
            }
            const textLine =
              m.text.trim() === ""
                ? "🐾 心情：（无文字）"
                : `🐾 心情：${m.text.trim()}`;
            const lines: string[] = [textLine];
            if (m.motion && m.motion.trim()) {
              lines.push(`  动作组：${m.motion.trim()}`);
            }
            pushLocalAssistantNote(lines.join("\n"));
          } catch (e) {
            pushLocalAssistantNote(`/mood 失败：${e}`);
          }
          break;
        }
        case "stats": {
          // 计数下沉到后端 task_stats（db.rs 单 SoT），桌面前端只负责文案排版。
          // "今日"语义、due 解析都在 Rust 侧统一，未来 widgets / PanelDebug 卡片
          // 复用同一份。TG /stats 走自己的 per-chat 路径，仍保持独立。
          try {
            const s = await invoke<{
              pending: number;
              overdue: number;
              done_today: number;
              error: number;
              cancelled_today: number;
            }>("task_stats");
            const allZero =
              !s.pending && !s.overdue && !s.done_today && !s.error && !s.cancelled_today;
            pushLocalAssistantNote(
              [
                allZero ? "📊 任务状态（今日很安静 ✨）" : "📊 任务状态",
                `○ 待办：${s.pending}`,
                `🔴 逾期：${s.overdue}`,
                `✓ 今日完成：${s.done_today}`,
                `⚠️ 出错：${s.error}`,
                `🗑 今日取消：${s.cancelled_today}`,
              ].join("\n"),
            );
          } catch (e) {
            pushLocalAssistantNote(`/stats 失败：${e}`);
          }
          break;
        }
        case "search": {
          setShowSessionList(false);
          setSearchMode(true);
          setSearchQuery("");
          setSearchResults([]);
          break;
        }
        case "sleep": {
          try {
            const until = await invoke<string>("set_mute_minutes", {
              minutes: action.minutes,
            });
            pushLocalAssistantNote(
              action.minutes === 0
                ? "已解除 mute。"
                : until
                  ? `已 mute 到 ${until.replace("T", " ").slice(0, 19)}（${action.minutes} 分钟）。`
                  : `已 mute ${action.minutes} 分钟。`,
            );
          } catch (e) {
            pushLocalAssistantNote(`mute 失败：${e}`);
          }
          break;
        }
        case "done": {
          // task_list → fuzzy 匹配 → 唯一命中调 task_mark_done。不限定状态
          // （后端 marker 幂等，允许"补打 done"）。匹配/多命中文案逻辑抽到
          // taskSlashHelpers，与 cancel/retry 共享。
          try {
            const resp = await invoke<{ tasks: Array<{ title: string }> }>("task_list");
            const titles = resp.tasks.map((t) => t.title);
            const res = matchTaskByQuery(action.query, titles);
            if (res.kind === "none") {
              pushLocalAssistantNote(
                `⚠️ 没找到匹配 "${action.query.trim()}" 的任务。/tasks 看完整列表。`,
              );
              break;
            }
            if (res.kind === "multi") {
              pushLocalAssistantNote(formatMultiHitMessage(action.query, res.candidates, ""));
              break;
            }
            await invoke<void>("task_mark_done", { title: res.title, result: null });
            pushLocalAssistantNote(`✓ 已标 done：${res.title}`);
          } catch (e) {
            pushLocalAssistantNote(`/done 失败：${e}`);
          }
          break;
        }
        case "cancel": {
          // 与 /done 同模板；差异：调 task_cancel(reason="")（对齐 TG），
          // 反馈图标 🗑。
          try {
            const resp = await invoke<{ tasks: Array<{ title: string }> }>("task_list");
            const titles = resp.tasks.map((t) => t.title);
            const res = matchTaskByQuery(action.query, titles);
            if (res.kind === "none") {
              pushLocalAssistantNote(
                `⚠️ 没找到匹配 "${action.query.trim()}" 的任务。/tasks 看完整列表。`,
              );
              break;
            }
            if (res.kind === "multi") {
              pushLocalAssistantNote(formatMultiHitMessage(action.query, res.candidates, ""));
              break;
            }
            await invoke<void>("task_cancel", { title: res.title, reason: "" });
            pushLocalAssistantNote(`🗑 已取消：${res.title}`);
          } catch (e) {
            pushLocalAssistantNote(`/cancel 失败：${e}`);
          }
          break;
        }
        case "retry": {
          // 差异：fuzzy 前先按 status==error 过滤候选；0 命中文案显式提示
          // "/retry 仅作用于 Error 状态"。
          try {
            const resp = await invoke<{
              tasks: Array<{ title: string; status: string }>;
            }>("task_list");
            const errorTitles = resp.tasks
              .filter((t) => t.status === "error")
              .map((t) => t.title);
            const res = matchTaskByQuery(action.query, errorTitles);
            if (res.kind === "none") {
              pushLocalAssistantNote(
                `⚠️ 没找到匹配 "${action.query.trim()}" 的 Error 任务（/retry 仅作用于 Error 状态；其它状态请去「任务」tab）。`,
              );
              break;
            }
            if (res.kind === "multi") {
              pushLocalAssistantNote(
                formatMultiHitMessage(action.query, res.candidates, "Error "),
              );
              break;
            }
            await invoke<void>("task_retry", { title: res.title });
            pushLocalAssistantNote(`↻ 已重试：${res.title}`);
          } catch (e) {
            pushLocalAssistantNote(`/retry 失败：${e}`);
          }
          break;
        }
        case "snooze": {
          // `/snooze`：与 /done 同模板 fuzzy 命中 title，然后 task_set_snooze
          // 把 description 写入 `[snooze: ...]` marker。spec 由 parser 解析、
          // until 由本地 `new Date()` 计算 —— 与桌面右键 Snooze chip 同源。
          // 候选范围：所有 task（pending / error），与桌面菜单 canMarkDone 同。
          try {
            const resp = await invoke<{
              tasks: Array<{ title: string; status: string }>;
            }>("task_list");
            const candidateTitles = resp.tasks
              .filter((t) => t.status === "pending" || t.status === "error")
              .map((t) => t.title);
            const res = matchTaskByQuery(action.title, candidateTitles);
            if (res.kind === "none") {
              pushLocalAssistantNote(
                `⚠️ 没找到匹配 "${action.title.trim()}" 的待办任务（/snooze 仅作用于 pending / error；已完成 / 取消的请去「任务」tab）。`,
              );
              break;
            }
            if (res.kind === "multi") {
              pushLocalAssistantNote(formatMultiHitMessage(action.title, res.candidates, ""));
              break;
            }
            const until = computeSnoozeUntil(action.spec, new Date());
            await invoke<void>("task_set_snooze", { title: res.title, until });
            pushLocalAssistantNote(`💤 已暂停至 ${until}：${res.title}`);
          } catch (e) {
            pushLocalAssistantNote(`/snooze 失败：${e}`);
          }
          break;
        }
        case "unsnooze": {
          // `/unsnooze`：fuzzy 命中后 task_set_snooze(null) 剥所有 `[snooze:]`
          // marker。候选不限 status —— 已 done 的任务 description 也允许清理
          // 残留 marker，但 0 命中文案聚焦 pending/error 主体场景。
          try {
            const resp = await invoke<{ tasks: Array<{ title: string }> }>("task_list");
            const titles = resp.tasks.map((t) => t.title);
            const res = matchTaskByQuery(action.query, titles);
            if (res.kind === "none") {
              pushLocalAssistantNote(
                `⚠️ 没找到匹配 "${action.query.trim()}" 的任务。/tasks 看完整列表。`,
              );
              break;
            }
            if (res.kind === "multi") {
              pushLocalAssistantNote(formatMultiHitMessage(action.query, res.candidates, ""));
              break;
            }
            await invoke<void>("task_set_snooze", { title: res.title, until: null });
            pushLocalAssistantNote(`☀️ 已解除暂停：${res.title}`);
          } catch (e) {
            pushLocalAssistantNote(`/unsnooze 失败：${e}`);
          }
          break;
        }
        case "pinTask": {
          // `/pin <title>`：带参数时是任务钉住（无参 `/pin` 走既有 case "pin"
          // 切换当前会话钉住）。fuzzy 命中后 task_set_pinned(true) 写 `[pinned]`
          // marker。候选不限 status —— pinned 与状态正交（owner 偏好标注），
          // 与 PanelTasks 右键菜单同语义。strip-before-write 后端保证幂等。
          try {
            const resp = await invoke<{ tasks: Array<{ title: string }> }>("task_list");
            const titles = resp.tasks.map((t) => t.title);
            const res = matchTaskByQuery(action.query, titles);
            if (res.kind === "none") {
              pushLocalAssistantNote(
                `⚠️ 没找到匹配 "${action.query.trim()}" 的任务。/tasks 看完整列表。`,
              );
              break;
            }
            if (res.kind === "multi") {
              pushLocalAssistantNote(formatMultiHitMessage(action.query, res.candidates, ""));
              break;
            }
            await invoke<void>("task_set_pinned", { title: res.title, pinned: true });
            pushLocalAssistantNote(`📌 已钉住：${res.title}`);
          } catch (e) {
            pushLocalAssistantNote(`/pin 失败：${e}`);
          }
          break;
        }
        case "unpin": {
          // 与 /pin 对偶；调 task_set_pinned(false) 剥所有 `[pinned]` marker。
          try {
            const resp = await invoke<{ tasks: Array<{ title: string }> }>("task_list");
            const titles = resp.tasks.map((t) => t.title);
            const res = matchTaskByQuery(action.query, titles);
            if (res.kind === "none") {
              pushLocalAssistantNote(
                `⚠️ 没找到匹配 "${action.query.trim()}" 的任务。/tasks 看完整列表。`,
              );
              break;
            }
            if (res.kind === "multi") {
              pushLocalAssistantNote(formatMultiHitMessage(action.query, res.candidates, ""));
              break;
            }
            await invoke<void>("task_set_pinned", { title: res.title, pinned: false });
            pushLocalAssistantNote(`📌 已取消钉住：${res.title}`);
          } catch (e) {
            pushLocalAssistantNote(`/unpin 失败：${e}`);
          }
          break;
        }
        case "help": {
          pushLocalAssistantNote(formatHelpText());
          break;
        }
        case "image": {
          // 用户回声 + 调起 runImageGenerate（idx=-1 → append pending）。
          // 多张：在回声里把 -n N 显出来，与用户实际输入对齐。
          // -r 标志：把最近一条 assistant 文本拼到 prompt 前作"参考语境"。
          // 找不到 last assistant 时 fallback 到 pushLocalAssistantNote 提示。
          let effectivePrompt = action.prompt;
          if (action.referenceLastAssistant) {
            const lastAssistant = [...items]
              .reverse()
              .find((it) => it.type === "assistant" && it.content.trim());
            if (!lastAssistant) {
              pushLocalAssistantNote(
                "⚠ /image -r：当前会话还没有 assistant 回复可引用。直接走 /image <prompt> 即可。",
              );
              break;
            }
            // 拼接策略：assistant 文本作"上下文"前缀 + "\n\n" 分隔 + 用户的
            // prompt 作主指令。prompt 为空时只用 assistant 文本（已在 parser
            // 允许 -r 空 prompt）。imagen 端模型多数自己能从语境提炼细节。
            const refText = lastAssistant.content.trim();
            effectivePrompt = action.prompt
              ? `${refText}\n\n${action.prompt}`
              : refText;
          }
          recordImagePrompt(effectivePrompt);
          const nFlag = action.n > 1 ? `-n ${action.n} ` : "";
          const rFlag = action.referenceLastAssistant ? "-r " : "";
          const sFlag = action.sizeOverride ? `-s ${action.sizeOverride} ` : "";
          const userEcho: ChatItem = {
            type: "user",
            content: `/image ${nFlag}${rFlag}${sFlag}${action.prompt}`.trimEnd(),
          };
          setItems((prev) => [...prev, userEcho]);
          runImageGenerate(effectivePrompt, -1, action.n, action.sizeOverride);
          break;
        }
        case "imageHelp": {
          pushLocalAssistantNote(formatImageHelpText());
          break;
        }
        case "incomplete":
          break; // 仅 `/` 还没敲完，菜单负责展示
        case "unknown": {
          pushLocalAssistantNote(`未知命令 /${action.name}。输入 /help 查看可用命令。`);
          break;
        }
      }
    },
    [sessionId, sessionTitle, onRequestTab, pushLocalAssistantNote, saveCurrentSession, runImageGenerate, clearArmed],
  );

  /// 复制单条消息到剪贴板。成功 → 闪 1.5s "已复制"反馈；失败 → console.error
  /// 不弹 alert（剪贴板权限错误极少，不值得打断用户）。
  ///
  /// `asMarkdown=false`（普通点击）：把 `「title」` 引用 token 的全角直角
  /// 引号剥掉，输出 `title` —— 用户复制到外部（Slack / 邮件 / IM）粘贴更干
  /// 净，不携带宠物内部约定的 ref 语法标记。
  /// `asMarkdown=true`（⌥/Alt+点击）：原样复制，含全部 `「」` —— 适合粘回
  /// 别的 chat session 让 LLM 继续看到 ref 信号、或归档到 markdown 日志。
  /// `withMeta=true`（⇧/Shift+点击）：在 payload 顶部加 `[<sessionTitle> ·
  /// YYYY-MM-DD HH:MM]` 标题块。两修饰键可叠加：⇧+⌥ → markdown 原文 + meta
  /// 前缀。用于外部归档 / share 上下文。
  const handleCopy = useCallback(
    async (idx: number, text: string, asMarkdown: boolean, withMeta: boolean) => {
      const body = asMarkdown
        ? text
        : text.replace(/「([^「」]+)」/g, "$1");
      const payload = withMeta
        ? `[${sessionTitle} · ${formatLocalStamp(new Date())}]\n${body}`
        : body;
      try {
        await navigator.clipboard.writeText(payload);
        setCopiedIdx(idx);
        window.setTimeout(() => {
          setCopiedIdx((prev) => (prev === idx ? null : prev));
        }, 1500);
      } catch (e) {
        console.error("clipboard write failed:", e);
      }
    },
    [sessionTitle],
  );

  /// 搜索结果点击 → 切到该会话 + 标记 pendingScroll，让下一帧 effect 滚到
  /// 命中 item 并高亮。如果当前已经在该 session，不重新 loadSession（避免
  /// 闪烁），直接触发 scroll。
  const handleSelectSearchHit = useCallback(
    async (hit: SearchHit) => {
      const keyword = searchQuery.trim();
      setSearchMode(false);
      setSearchQuery("");
      setSearchResults([]);
      setSearchScope("all");
      setShowSessionList(false);
      if (hit.session_id !== sessionId) {
        await loadSession(hit.session_id);
      }
      setPendingScroll(hit.item_index);
      setSearchHit(keyword ? { idx: hit.item_index, keyword } : null);
    },
    [sessionId, searchQuery],
  );

  // R106: 单 session 导出 markdown 到剪贴板。load_session 拉完整 items，
  // 跳过 tool / error 行（噪音多对"对话内容"复盘价值低），按 user/assistant
  // 时间序拼成 H2 段落 markdown。reuse copyMsg 通道做短反馈，3000ms 自清空。
  const handleExportSession = async (id: string, fallbackTitle: string) => {
    try {
      const session = await invoke<Session>("load_session", { id });
      const title = session.title || fallbackTitle;
      const md = exportSessionAsMarkdown(title, session.items as ChatItem[]);
      await navigator.clipboard.writeText(md);
      setExportToast(`已导出 "${title}" 到剪贴板`);
      setTimeout(() => setExportToast(""), 3000);
    } catch (e) {
      console.error("Failed to export session:", e);
      setExportToast(`导出失败: ${e}`);
      setTimeout(() => setExportToast(""), 3000);
    }
  };

  // R101: rename 三件套。startRename 进入 edit；commitRename load → 改 title
  // → save → refresh list（顶 bar 同步），失败时 console.error 后退出 edit；
  // cancelRename 直接退出。空标题视为 cancel（保留原 title）。
  const startRename = (s: SessionMeta) => {
    setRenamingId(s.id);
    setRenameDraft(s.title);
  };
  const commitRename = async () => {
    const id = renamingId;
    const newTitle = renameDraft.trim();
    if (!id) return;
    if (!newTitle) {
      setRenamingId(null);
      return;
    }
    try {
      const session = await invoke<Session>("load_session", { id });
      session.title = newTitle;
      await invoke("save_session", { session });
      const idx = await invoke<SessionIndex>("list_sessions");
      setSessionList(idx.sessions);
      if (id === sessionId) setSessionTitle(newTitle);
    } catch (e) {
      console.error("Failed to rename session:", e);
    } finally {
      setRenamingId(null);
    }
  };
  const cancelRename = () => setRenamingId(null);

  /// 切 session 的 pinned 状态。后端写完后重新拉 list_sessions —— 一致性以
  /// 后端为准，不手动 mutate sessionList 防 race（多端 / 并发改写）。
  const handleTogglePinned = async (id: string, pinned: boolean) => {
    try {
      await invoke("set_session_pinned", { id, pinned });
      const index = await invoke<SessionIndex>("list_sessions");
      setSessionList(index.sessions);
    } catch (e) {
      console.error("Failed to toggle session pinned:", e);
    }
  };

  const handleDeleteSession = async (id: string) => {
    try {
      await invoke("delete_session", { id });
      const index = await invoke<SessionIndex>("list_sessions");
      setSessionList(index.sessions);

      // If deleted the current session, switch to another or create new
      if (id === sessionId) {
        if (index.sessions.length > 0) {
          const last = index.sessions[index.sessions.length - 1];
          await loadSession(last.id);
        } else {
          await handleNewSession();
        }
      }
    } catch (e) {
      console.error("Failed to delete session:", e);
    }
  };

  // 删除二次确认：第 1 次点击转 pending（按钮变红填充 + "确定？"），5s 内再
  // 点 → 真删；过期 / 切到别的 session → 自动 revert。同时只允许一个 session
  // 处于 pending（点 B 时 A 自动取消）。
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
  const pendingDeleteTimerRef = useRef<number | null>(null);
  const handleDeleteClick = (id: string) => {
    if (pendingDeleteId === id) {
      if (pendingDeleteTimerRef.current !== null) {
        window.clearTimeout(pendingDeleteTimerRef.current);
        pendingDeleteTimerRef.current = null;
      }
      setPendingDeleteId(null);
      void handleDeleteSession(id);
      return;
    }
    if (pendingDeleteTimerRef.current !== null) {
      window.clearTimeout(pendingDeleteTimerRef.current);
    }
    setPendingDeleteId(id);
    pendingDeleteTimerRef.current = window.setTimeout(() => {
      setPendingDeleteId((prev) => (prev === id ? null : prev));
      pendingDeleteTimerRef.current = null;
    }, 5000);
  };

  const sendMessage = useCallback(
    async (
      content: string,
      images?: string[],
      /// 编辑/重发等场景需要在追加新 user 之前先截断 items —— 但 setItems
      /// 异步、closure 里读到的仍是旧 items。caller 直接传 baseItems 覆盖
      /// 闭包，让 sendMessage 在显式 base 上追加而不必依赖 React 状态调度。
      /// 不传时回到原行为：以闭包里的 `items` 为起点。
      ///
      /// caller 还需先把 `messagesRef.current` 截断到对应位置 ——
      /// sendMessage 内部以 `messagesRef.current` 当时的值为基础 push，
      /// 与 items 双侧对齐由 caller 保证（见 commitMessageEdit）。
      opts?: { baseItems?: ChatItem[] },
    ) => {
      // 多模态消息：OpenAI compatible multipart 内容数组。无图时仍走纯字符串
      // 路径，保持与未升级的后端 / 测试断言兼容。
      const hasImages = !!images && images.length > 0;
      const messageContent: any = hasImages
        ? [
            ...(content ? [{ type: "text", text: content }] : []),
            ...images!.map((url) => ({ type: "image_url", image_url: { url } })),
          ]
        : content;
      const userMsg = { role: "user", content: messageContent };
      messagesRef.current = [...messagesRef.current, userMsg];
      const base = opts?.baseItems ?? items;
      const newItems = [
        ...base,
        {
          type: "user" as const,
          content,
          ...(hasImages ? { images } : {}),
        },
      ];
      setItems(newItems);
      setIsLoading(true);
      setCurrentResponse("");
      setCurrentToolCalls([]);

      const onEvent = new Channel<StreamEvent>();
      let accumulated = "";
      let toolCalls: ToolCall[] = [];
      let finalItems = newItems;

      const flushToolCalls = () => {
        if (toolCalls.length > 0) {
          const snapshot = [...toolCalls];
          finalItems = [...finalItems, { type: "tool" as const, content: "", toolCalls: snapshot }];
          setItems(finalItems);
          toolCalls = [];
          setCurrentToolCalls([]);
        }
      };

      onEvent.onmessage = (event: StreamEvent) => {
        if (event.event === "chunk") {
          flushToolCalls();
          accumulated += event.data.text;
          setCurrentResponse(accumulated);
        } else if (event.event === "toolStart") {
          if (accumulated.trim()) {
            finalItems = [...finalItems, { type: "assistant", content: accumulated }];
            setItems(finalItems);
          }
          accumulated = "";
          setCurrentResponse("");
          const tc: ToolCall = {
            name: event.data.name,
            arguments: event.data.arguments,
            isRunning: true,
          };
          toolCalls = [...toolCalls, tc];
          setCurrentToolCalls([...toolCalls]);
        } else if (event.event === "toolResult") {
          toolCalls = toolCalls.map((tc) =>
            tc.name === event.data.name && tc.isRunning
              ? { ...tc, result: event.data.result, isRunning: false }
              : tc,
          );
          setCurrentToolCalls([...toolCalls]);
        } else if (event.event === "done") {
          flushToolCalls();
          if (accumulated.trim()) {
            finalItems = [...finalItems, { type: "assistant", content: accumulated }];
            setItems(finalItems);
            messagesRef.current = [
              ...messagesRef.current,
              { role: "assistant", content: accumulated },
            ];
          }
          setCurrentResponse("");
          setIsLoading(false);
          // Auto-save after completion
          saveCurrentSession(finalItems);
        } else if (event.event === "error") {
          finalItems = [...finalItems, { type: "error", content: event.data.message }];
          setItems(finalItems);
          setCurrentResponse("");
          setCurrentToolCalls([]);
          setIsLoading(false);
          saveCurrentSession(finalItems);
        }
      };

      try {
        await invoke("chat", { messages: messagesRef.current, onEvent });
      } catch (err) {
        finalItems = [...finalItems, { type: "error" as const, content: `${err}` }];
        setItems(finalItems);
        setIsLoading(false);
        saveCurrentSession(finalItems);
      }
    },
    [items, saveCurrentSession],
  );

  /// 双击 user bubble 进编辑态。流式中拒绝（避免截断 race），有图的消息
  /// 也拒绝（编辑文本同时保留 / 删图边界复杂，留给后续）。
  const enterEditMode = useCallback(
    (idx: number) => {
      if (isLoading) return;
      const it = items[idx];
      if (!it || it.type !== "user") return;
      if (it.images && it.images.length > 0) return;
      setEditingItemIdx(idx);
      setEditingDraft(it.content);
    },
    [isLoading, items],
  );
  const cancelEditMode = useCallback(() => {
    setEditingItemIdx(null);
    setEditingDraft("");
  }, []);
  /// 提交编辑：truncate items[idx:] 同时把 messagesRef.current 截到 user
  /// 消息那条，然后 sendMessage(newContent, baseItems=newItems) —— sendMessage
  /// 内部会再 push 一条新 user，并通过流式响应继续生成。saveCurrentSession
  /// 在 sendMessage 的 done / error 分支里跑，自然写盘。
  const commitMessageEdit = useCallback(() => {
    if (editingItemIdx === null) return;
    const trimmed = editingDraft.trim();
    if (!trimmed) return; // 空文本不允许提交（保持原行为：empty 消息不发）
    if (isLoading) return;
    const msgIdx = findMessageIndexForUserItem(
      items,
      messagesRef.current,
      editingItemIdx,
    );
    if (msgIdx === null) {
      // 不一致（理论不会发生，items 与 messagesRef 一直 lockstep 加 user）。
      // 进入恢复模式：取消编辑、留状态原样，让用户能手动 retry。
      cancelEditMode();
      return;
    }
    const newItems = items.slice(0, editingItemIdx);
    messagesRef.current = messagesRef.current.slice(0, msgIdx);
    setItems(newItems);
    setEditingItemIdx(null);
    setEditingDraft("");
    void sendMessage(trimmed, undefined, { baseItems: newItems });
  }, [editingItemIdx, editingDraft, isLoading, items, sendMessage, cancelEditMode]);

  // R126: submit 主逻辑抽出来让 textarea Enter 路径与 form button click 路径
  // 共用。trim / slash 分支 / sendMessage / messageHistory 维护都在这里。
  const submitInput = useCallback(() => {
    const trimmed = input.trim();
    const hasImages = pendingImages.length > 0;
    // 没图时空白消息直接 noop；有图时允许空文本（"图说一切"）。
    if (!hasImages && !trimmed) return;
    if (isLoading) return;
    if (trimmed.startsWith("/")) {
      const action = parseSlashCommand(trimmed);
      if (action) {
        executeSlash(action);
        setInput("");
        setSelectedSlashIdx(0);
        return;
      }
    }
    if (hasImages) {
      // 非多模态模型 → 拒发，给本地 assistant 提示（不发给 LLM）。检查走
      // 后端 settings 真值；前端不缓存 model 名让用户切换 settings 后立刻生效。
      (async () => {
        try {
          const ok = await invoke<boolean>("is_current_model_multimodal");
          if (!ok) {
            const settings = await invoke<{ model: string }>("get_settings").catch(() => ({ model: "?" }));
            pushLocalAssistantNote(
              `当前模型 \`${settings.model || "?"}\` 不支持图片输入，已忽略 ${pendingImages.length} 张图。可在设置页换成 gpt-4o / claude-3 / gemini / qwen-vl 等多模态模型。`,
            );
            setPendingImages([]);
            return;
          }
          sendMessage(trimmed, pendingImages);
          setPendingImages([]);
          setMessageHistory(pushSentHistory(trimmed));
          setHistoryCursor(null);
          setInput("");
        } catch (e) {
          console.error("multimodal gate failed:", e);
          pushLocalAssistantNote(`检测多模态能力失败：${e}`);
        }
      })();
      return;
    }
    sendMessage(trimmed);
    // 走 chatHistoryStore 共享层 dedup + move-to-front + 写盘 + cap 20。
    // historyCursor 重置 null 让发送后下次 ↑ 从最新（index 0）开始。
    setMessageHistory(pushSentHistory(trimmed));
    setHistoryCursor(null);
    setInput("");
  }, [input, isLoading, executeSlash, sendMessage, pendingImages, pushLocalAssistantNote]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    submitInput();
  };

  /// 命令菜单点击：把命令名回填到输入框。带参数命令保留 `/cmd ` 等空格让用户
  /// 接着填参；不带参数的直接把命令名 + 立即提交（视觉上等价于点了一下回车）。
  const handleSelectSlashCommand = useCallback(
    (cmd: SlashCommand) => {
      if (cmd.parametric) {
        setInput(`/${cmd.name} `);
      } else {
        // 直接执行（无参命令选了就执行，少一次按 Enter）
        executeSlash(parseSlashCommand(`/${cmd.name}`)!);
        setInput("");
        setSelectedSlashIdx(0);
      }
    },
    [executeSlash],
  );

  /// 输入框键盘事件：slash 模式下接管上下 / Enter / Esc / Tab。
  /// Enter 行为：
  /// - 如果当前 input 已经是一个完整命令名（如 `/clear`，能 parse 成功且非
  ///   Unknown / Incomplete）→ 不在 keydown 拦截，让 form onSubmit 执行。
  /// - 否则按 Tab 等价处理：用选中候选自动补全（参数化命令保留 prefix 等用户
  ///   再敲）。这避免了 `/cl` + Enter 触发"未知命令 /cl"。
  const handleInputKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // ⌘K / Ctrl+K → 打开 task 引用选择器。优先级高于 image 历史 / slash 菜单 /
    // Enter 提交 —— 是独立的"召唤面板"快捷键，不应被其它分支抢走。e.preventDefault
    // 吃掉浏览器 / Tauri 默认的 ⌘K（无）行为，安全。
    if (
      (e.metaKey || e.ctrlKey) &&
      !e.shiftKey &&
      !e.altKey &&
      e.key.toLowerCase() === "k"
    ) {
      e.preventDefault();
      void openTaskPicker();
      return;
    }
    // ⌘B / Ctrl+B → 切到上一个 session（与 swapTargetRef 配合）。让 power
    // user 在两个 session 间"乒乓"快速来回。无上一会话 / 上一会话 === 当
    // 前会话时 noop。e.preventDefault 吃浏览器默认（编辑器粗体快捷键，
    // 在 textarea 内浏览器不会渲染粗体，安全劫持）。
    if (
      (e.metaKey || e.ctrlKey) &&
      !e.shiftKey &&
      !e.altKey &&
      e.key.toLowerCase() === "b"
    ) {
      e.preventDefault();
      const target = swapTargetRef.current;
      if (target && target !== sessionId) {
        void loadSession(target);
      }
      return;
    }
    // ⌘N / Ctrl+N → 新建会话。与 IDE / 浏览器 ⌘N = "新建"直觉一致。也有
    // 全局监听同效果（line ~602 useEffect），这里 textarea 内独立分支让事
    // 件流单一明确（与 ⌘K / ⌘B 同模式 —— 在输入框内键盘党更快）。
    if (
      (e.metaKey || e.ctrlKey) &&
      !e.shiftKey &&
      !e.altKey &&
      e.key.toLowerCase() === "n"
    ) {
      e.preventDefault();
      void handleNewSession();
      return;
    }
    // `@` 提及菜单可见时：↑↓ 选 / Enter / Tab 填入 / Esc 关。优先级高于
    // image / slash / Enter 提交 — `@<query>` 形态下用户的意图就是 mention，
    // 让其它分支抢键会破坏体验。Esc 仅退出 picker（清掉 `@<query>` 段）。
    if (mentionMenuVisible && mentionContext) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setMentionSelectedIdx((i) =>
          Math.min(i + 1, mentionFilteredTasks.length - 1),
        );
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setMentionSelectedIdx((i) => Math.max(0, i - 1));
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        const picked = mentionFilteredTasks[mentionSelectedIdx];
        if (picked) {
          pickMention(picked.title, mentionContext);
        }
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        // 退出方式：把 `@<query>` 段从 input 中删掉，光标落回 `@` 之前。
        // 与 ⌘K modal 的 Esc"关 modal 不动 input"语义不同 —— inline picker
        // 里 `@` 还残留在 input 上会让用户下次 keystroke 又触发菜单。
        const cur = input;
        const next =
          cur.slice(0, mentionContext.start) +
          cur.slice(mentionContext.start + 1 + mentionContext.query.length);
        setInput(next);
        const newCursor = mentionContext.start;
        setComposeCursorPos(newCursor);
        window.setTimeout(() => {
          const t = composeTextareaRef.current;
          if (t) {
            t.focus();
            t.setSelectionRange(newCursor, newCursor);
          }
        }, 0);
        return;
      }
      // 其它键透传 —— 用户继续敲字 query 实时变化，菜单跟着 filter。
    }
    // `/image` 历史菜单可见时：↑↓ 选 / Enter / Tab 填入 / Esc 关。这条分支
    // 必须在 slashMenuVisible 检查之前 —— imagePromptMenuVisible 只在
    // input === `/image` 或 `/image ` 时成立，此时 slashPrefix 已经走过空格
    // → null，所以两个 menu 不会同时可见，但显式分支更清晰。
    if (imagePromptMenuVisible) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedImagePromptIdx((i) =>
          Math.min(i + 1, imagePromptHistory.length - 1),
        );
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedImagePromptIdx((i) => Math.max(i - 1, 0));
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        const picked = imagePromptHistory[selectedImagePromptIdx];
        if (picked) {
          // Enter 直接 fill + submit 一气呵成；Tab 仅 fill 让用户继续编辑
          // （vim ci 风格，给"先 pick 历史再加几个字"的细调留口子）。
          if (e.key === "Enter") {
            setInput("");
            executeSlash({
              kind: "image",
              prompt: picked.prompt,
              n: 1,
              referenceLastAssistant: false,
              sizeOverride: null,
            });
          } else {
            setInput(`/image ${picked.prompt}`);
          }
        }
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        // 关菜单：把 input 推进到 `/image cancel` 这种"非触发态" — 但用户没敲东西
        // 我们不能凭空添加字。最自然的关法是清空 input。
        setInput("");
        return;
      }
      // 其它键透传给 input 让用户继续敲 prompt（敲一个字符就关菜单 —— 由
      // imagePromptTriggerActive 的 regex 自动失效）
    }
    if (!slashMenuVisible) {
      // R126: textarea Enter 默认换行；在非-slash 模式 + Enter（无 shift）
      // → 显式提交（preventDefault 防换行）；Shift+Enter → 让默认行为换行。
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        submitInput();
        return;
      }
      // R127: 非 slash 模式 + 非空 input + Esc → 清空（"我不发了"语义）。
      // 空 input 时不拦截，避免吞掉浏览器 / IME 默认 Esc 行为。
      // R129: 历史模式中按 Esc 也退出（设 cursor=null + 清空 input）。
      if (e.key === "Escape" && input.length > 0) {
        e.preventDefault();
        setInput("");
        setHistoryCursor(null);
        return;
      }
      // ⌥↑（Alt+ArrowUp）IM 风召回：跳过 send-history 循环、直接进入最近
      // 一条 user 消息的 inline 编辑模式。要求 input 为空 + 不在历史模式
      // 中 + 找得到 user item + 不在 isLoading 中（enterEditMode 内还会再
      // 守一次），所有条件不满足则让出键位走原逻辑。
      // 用 ⌥↑ 而非纯 ↑ 是为了不抢 send-history ↑（已有肌肉记忆）；与
      // README §7「聊天输入历史栈」并存。
      if (
        e.key === "ArrowUp" &&
        e.altKey &&
        !e.metaKey &&
        !e.ctrlKey &&
        input.length === 0 &&
        historyCursor === null &&
        !isLoading
      ) {
        // 倒序找最近一条 type === "user" 的 item。null 时 noop（fresh
        // session 没消息或全是 assistant 系统反馈，不该消化 ⌥↑）。
        let lastUserIdx = -1;
        for (let i = items.length - 1; i >= 0; i--) {
          if (items[i]?.type === "user") {
            lastUserIdx = i;
            break;
          }
        }
        if (lastUserIdx >= 0) {
          e.preventDefault();
          enterEditMode(lastUserIdx);
          return;
        }
      }
      // shell-readline 风 ↑ / ↓ 多条历史穿越。newest-at-front 约定：
      // ↑：历史模式中 cursor+1 往前翻（更旧）；空 input + 历史非空 → cursor=0 顶
      // ↓：历史模式中 cursor-1；< 0 → 退出 + 清空
      // 非空 input 且不在历史模式时不拦截 ↑（textarea 多行光标向上行为）。
      if (e.key === "ArrowUp") {
        if (historyCursor !== null) {
          e.preventDefault();
          const next = Math.min(historyCursor + 1, messageHistory.length - 1);
          setHistoryCursor(next);
          setInput(messageHistory[next]);
          return;
        }
        if (input.length === 0 && messageHistory.length > 0) {
          e.preventDefault();
          setHistoryCursor(0);
          setInput(messageHistory[0]);
          return;
        }
        return;
      }
      if (e.key === "ArrowDown") {
        if (historyCursor !== null) {
          e.preventDefault();
          const next = historyCursor - 1;
          if (next < 0) {
            setHistoryCursor(null);
            setInput("");
          } else {
            setHistoryCursor(next);
            setInput(messageHistory[next]);
          }
        }
        return;
      }
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedSlashIdx((i) =>
        filteredCommands.length === 0 ? 0 : Math.min(i + 1, filteredCommands.length - 1),
      );
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedSlashIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Tab") {
      e.preventDefault();
      const cmd = filteredCommands[selectedSlashIdx];
      if (cmd) handleSelectSlashCommand(cmd);
    } else if (e.key === "Escape") {
      e.preventDefault();
      setInput("");
      setSelectedSlashIdx(0);
    } else if (e.key === "Enter") {
      // R126: textarea Enter 总不应换行（命令是单行）。complete known →
      // 显式 submitInput（不再依赖 form onSubmit fallthrough）；prefix →
      // autocomplete 选中候选。
      e.preventDefault();
      const action = parseSlashCommand(input);
      const isCompleteKnown =
        action !== null &&
        action.kind !== "unknown" &&
        action.kind !== "incomplete";
      if (isCompleteKnown) {
        submitInput();
      } else {
        const cmd = filteredCommands[selectedSlashIdx];
        if (cmd) handleSelectSlashCommand(cmd);
      }
    }
  };

  if (!loaded) {
    return (
      <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: "var(--pet-color-muted)" }}>
        加载中...
      </div>
    );
  }

  return (
    <div
      className="pet-panelchat-root"
      style={{ display: "flex", flexDirection: "column", height: "100%", position: "relative" }}
      onDragEnter={(e) => {
        // 仅当 dataTransfer 含 file 类型时才进入 drag 态 —— 普通文本 / DOM
        // 拖拽不触发 overlay 干扰。dragenter 在子元素冒泡里也会触发，用计数防抖。
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
        const imageBlobs: Blob[] = [];
        const textFiles: File[] = [];
        for (let i = 0; i < files.length; i++) {
          const f = files[i];
          if (f.type.startsWith("image/")) {
            imageBlobs.push(f);
            continue;
          }
          // 文本文件：text/* MIME 或 .md / .txt / .json / .csv / .log / .yaml 后缀。
          // 部分 OS 给 .md 报 application/octet-stream，所以两条判定都要走。
          const lower = f.name.toLowerCase();
          if (
            f.type.startsWith("text/") ||
            f.type === "application/json" ||
            /\.(md|markdown|txt|json|jsonl|csv|tsv|log|ya?ml|toml|ini|conf|env|sh|rs|py|ts|tsx|js|jsx|html|css)$/i.test(
              lower,
            )
          ) {
            textFiles.push(f);
          }
        }
        if (imageBlobs.length > 0) ingestImageBlobs(imageBlobs);
        if (textFiles.length > 0) {
          // 文本文件：FileReader.readAsText 读取后拼成 ```<filename>\n<content>\n```
          // 块附加到 input。100KB 软上限 —— 大文件多半应该让用户主动 paste
          // 抢救 token，不让 drop 不慎吞超长内容。多个文件按顺序拼接，每个
          // 独立 code fence 段。
          const MAX_TEXT_BYTES = 100_000;
          const reads: Promise<string>[] = textFiles.map(
            (f) =>
              new Promise((resolve) => {
                const reader = new FileReader();
                reader.onload = () => {
                  let text = typeof reader.result === "string" ? reader.result : "";
                  let truncated = false;
                  if (text.length > MAX_TEXT_BYTES) {
                    text = text.slice(0, MAX_TEXT_BYTES);
                    truncated = true;
                  }
                  // 取文件扩展名作 fence 语言提示（让 LLM 知道这是什么类型）
                  const ext = (f.name.match(/\.([a-zA-Z0-9]+)$/) || [])[1] ?? "";
                  const lang = ext === "md" || ext === "markdown" ? "" : ext;
                  const fence = "```" + lang;
                  const note = truncated
                    ? `\n（已截断到前 ${MAX_TEXT_BYTES} 字节）`
                    : "";
                  resolve(
                    `\n\n📎 ${f.name}${note}\n${fence}\n${text}\n\`\`\``,
                  );
                };
                reader.onerror = () => resolve(`\n\n⚠ 读取失败：${f.name}`);
                reader.readAsText(f);
              }),
          );
          Promise.all(reads).then((chunks) => {
            setInput((prev) => prev + chunks.join(""));
          });
        }
      }}
    >
      {/* 拖拽 image 文件时的 overlay：覆盖整个 panel 给视觉反馈，pointerEvents
          none 让 onDragOver / onDrop 仍走 root（不被 overlay 接掉）。zIndex 高
          于 search 下拉等浮层。 */}
      {dragActive && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            zIndex: 100,
            background: "color-mix(in srgb, var(--pet-color-accent) 18%, transparent)",
            border: "2px dashed var(--pet-color-accent)",
            borderRadius: 12,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            pointerEvents: "none",
            color: "var(--pet-color-accent)",
            fontSize: 14,
            fontWeight: 600,
          }}
        >
          📎 松开把图片加到输入区
        </div>
      )}
      {/* Iter R47: focus ring audit — input had `outline: none` with no
          replacement (same accessibility issue R46 fixed in ChatPanel
          and R47 fixed in PanelSettings). Scoped descendant selector
          covers all input/textarea inside this panel. */}
      <style>{`
        .pet-panelchat-root input:focus,
        .pet-panelchat-root textarea:focus {
          border-color: var(--pet-color-accent);
          box-shadow: 0 0 0 2px color-mix(in srgb, var(--pet-color-accent) 22%, transparent);
          transition: border-color 150ms ease-out, box-shadow 150ms ease-out;
        }
        /* 聊天消息复制按钮：默认隐藏，hover 整行时弱可见，hover 按钮自身时强化。
           已复制状态由内联 style 强制 opacity=1 与绿色，覆盖默认。 */
        .pet-chat-row .pet-copy-btn {
          opacity: 0;
          transition: opacity 120ms ease-out, color 120ms ease-out, border-color 120ms ease-out;
        }
        .pet-chat-row:hover .pet-copy-btn {
          opacity: 0.85;
        }
        .pet-chat-row .pet-copy-btn:hover {
          opacity: 1;
          color: var(--pet-color-accent);
          border-color: color-mix(in srgb, var(--pet-color-accent) 50%, transparent);
        }
        /* 迭代 5：气泡 hover 阴影增强 —— 仅触发更强的 shadow，不动 transform
           （气泡之间间距小，translate 易抖；shadow 渐变更稳）。user / assistant
           各用对应色相的 hover-tier shadow，与 inline base shadow 同层级。 */
        .pet-chat-row:hover .pet-chat-bubble[data-role="user"] {
          box-shadow: 0 4px 14px color-mix(in srgb, var(--pet-color-accent) 40%, transparent);
        }
        .pet-chat-row:hover .pet-chat-bubble[data-role="assistant"] {
          box-shadow: var(--pet-shadow-md);
        }
        /* 图片缩略图 hover-复制 CSS 由 ImageThumb 组件自己 inject 一次 head <style>，
           PanelChat 这里不再重复维护。 */
        /* R131: 会话列表行 hover 高亮，与 R122/R123/R130 同模式。!important
           反压 inline selected 蓝色（hover 时短暂换浅灰，移开恢复 — 用户
           操作流是"hover 看清 → 点击"，期间 selected 蓝退让可接受）。
           迭代 7：换成 accent 8% alpha 暖底（与 tab bar hover 同节奏 / dark
           友好），原 rgba(0,0,0,0.04) 在 dark 主题下不可见。加 border-left
           accent 边条做"轨道感"，扫长 session 列表时定位更明确。 */
        .pet-session-row {
          transition: background-color 0.14s ease, box-shadow 0.18s ease,
            border-color 0.18s ease;
          border-left: 3px solid transparent;
        }
        .pet-session-row:hover {
          background: color-mix(in srgb, var(--pet-color-accent) 8%, transparent) !important;
          border-left-color: color-mix(in srgb, var(--pet-color-accent) 55%, transparent);
        }
        /* R142: 发送按钮 hover 强化。filter brightness 跨色域生效（accent
           在 light/dark 不同色都自动加深）；scale 0.98 给 click 微触感；
           :not(:disabled) 保护 loading 灰态不被 hover 干扰。 */
        .pet-chat-send {
          transition: filter 0.12s ease, transform 0.08s ease;
        }
        .pet-chat-send:not(:disabled):hover {
          filter: brightness(0.92);
          transform: scale(0.98);
        }
        /* "刚从 mini chat 进面板"黄底脉冲：仅在 focusFromMiniPulse 状态时
           class 挂上，1.2s linear 单跑一次后被 React 卸掉。box-shadow 拉
           出柔光圈让脉冲不抖动布局（不改 layout 维度）。 */
        @keyframes pet-session-bar-focus-pulse {
          0%   { background: var(--pet-tint-yellow-bg); box-shadow: 0 0 0 0 rgba(250, 204, 21, 0.45); }
          60%  { background: var(--pet-tint-yellow-bg); box-shadow: 0 0 0 6px rgba(250, 204, 21, 0); }
          100% { background: transparent; box-shadow: 0 0 0 0 rgba(250, 204, 21, 0); }
        }
        .pet-session-bar-pulse {
          animation: pet-session-bar-focus-pulse 1.2s ease-out 1;
        }
        @media (prefers-reduced-motion: reduce) {
          .pet-session-bar-pulse { animation: none; background: var(--pet-tint-yellow-bg); }
        }
      `}</style>
      {/* Session header bar */}
      <div
        style={sessionBarStyle}
        className={focusFromMiniPulse ? "pet-session-bar-pulse" : undefined}
      >
        <div
          style={{ display: "flex", alignItems: "center", gap: "8px", flex: 1, cursor: "pointer", minWidth: 0 }}
          onClick={() => {
            // 进 session 下拉时关掉 search mode（互斥 UI）
            setSearchMode(false);
            setShowSessionList(!showSessionList);
          }}
        >
          <span style={{ fontSize: "13px", fontWeight: 600, color: "var(--pet-color-fg)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {sessionTitle}
          </span>
          {/* token 估算角标：粗估 chars/4。> TOKEN_WARN 黄底；> TOKEN_CRIT
              红底警示可能溢出 context window，提示用户开新会话或精简。
              < 100 token 不显（新会话刚起 / 无意义）。 */}
          {sessionTokensEstimate >= 100 && (() => {
            const interactive = sessionTokensEstimate >= TOKEN_WARN;
            return (
              <span
                onMouseDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.stopPropagation();
                  if (interactive) setCompactPromptOpen((v) => !v);
                }}
                style={{
                  position: "relative",
                  fontSize: 10,
                  fontWeight: 600,
                  padding: "1px 6px",
                  borderRadius: 8,
                  whiteSpace: "nowrap",
                  fontVariantNumeric: "tabular-nums",
                  background:
                    sessionTokensEstimate >= TOKEN_CRIT
                      ? "var(--pet-tint-red-bg)"
                      : sessionTokensEstimate >= TOKEN_WARN
                        ? "var(--pet-tint-yellow-bg)"
                        : "var(--pet-color-bg)",
                  color:
                    sessionTokensEstimate >= TOKEN_CRIT
                      ? "var(--pet-tint-red-fg)"
                      : sessionTokensEstimate >= TOKEN_WARN
                        ? "var(--pet-tint-yellow-fg)"
                        : "var(--pet-color-muted)",
                  border:
                    sessionTokensEstimate >= TOKEN_CRIT
                      ? "1px solid var(--pet-tint-red-fg)"
                      : "1px solid var(--pet-color-border)",
                  cursor: interactive ? "pointer" : "default",
                }}
                title={
                  sessionTokensEstimate >= TOKEN_CRIT
                    ? `估算约 ${sessionTokensEstimate} token —— 已接近常见模型 context 上限。点击压缩前面历史让 context 立刻轻量。`
                    : sessionTokensEstimate >= TOKEN_WARN
                      ? `估算约 ${sessionTokensEstimate} token —— 历史已较长，点击可压缩前面消息。`
                      : `估算约 ${sessionTokensEstimate} token（粗估：累计字符数 / 4）`
                }
              >
                ~{sessionTokensEstimate >= 1000 ? `${(sessionTokensEstimate / 1000).toFixed(1)}k` : sessionTokensEstimate} tok
                {/* 压缩历史浮窗：仅 interactive 时 click 切换打开。three-level
                    trim 选项让用户挑保留多少条；选完即时 trim + save + close。 */}
                {compactPromptOpen && interactive && (
                  <div
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={(e) => e.stopPropagation()}
                    style={{
                      position: "absolute",
                      top: "calc(100% + 6px)",
                      left: 0,
                      minWidth: 240,
                      background: "var(--pet-color-card)",
                      border: "1px solid var(--pet-color-border)",
                      borderRadius: 8,
                      boxShadow: "var(--pet-shadow-md)",
                      padding: 8,
                      zIndex: 30,
                      whiteSpace: "normal",
                      fontWeight: 400,
                      fontSize: 12,
                      color: "var(--pet-color-fg)",
                      fontVariantNumeric: "normal",
                    }}
                  >
                    <div
                      style={{
                        fontSize: 11,
                        color: "var(--pet-color-muted)",
                        marginBottom: 6,
                      }}
                    >
                      压缩前面历史（保留 system / SOUL 提示）：
                    </div>
                    {(() => {
                      const total = items.length;
                      const opts = [
                        { keep: Math.max(4, Math.ceil(total / 2)), label: "保留近 1/2" },
                        { keep: Math.max(4, Math.ceil(total / 3)), label: "保留近 1/3" },
                        { keep: Math.min(total, 4), label: "仅保留最近 4 条" },
                      ];
                      return opts.map((o) => {
                        const drop = total - o.keep;
                        return (
                          <button
                            key={o.label}
                            type="button"
                            disabled={drop <= 0}
                            onClick={(e) => {
                              e.stopPropagation();
                              void compactHistory(o.keep);
                            }}
                            style={{
                              display: "block",
                              width: "100%",
                              textAlign: "left",
                              padding: "5px 8px",
                              marginBottom: 3,
                              fontSize: 12,
                              border: "1px solid var(--pet-color-border)",
                              borderRadius: 4,
                              background: drop <= 0 ? "var(--pet-color-bg)" : "var(--pet-color-card)",
                              color: drop <= 0 ? "var(--pet-color-muted)" : "var(--pet-color-fg)",
                              cursor: drop <= 0 ? "default" : "pointer",
                              fontFamily: "inherit",
                            }}
                          >
                            {o.label}
                            <span
                              style={{
                                fontSize: 10,
                                marginLeft: 6,
                                color: "var(--pet-color-muted)",
                              }}
                            >
                              （丢 {drop} 条 / 保留 {o.keep} 条）
                            </span>
                          </button>
                        );
                      });
                    })()}
                    <div
                      style={{
                        fontSize: 10,
                        color: "var(--pet-color-muted)",
                        marginTop: 4,
                        lineHeight: 1.4,
                      }}
                    >
                      💾 压缩前会自动把所有 session 备份到剪贴板，反悔时点 📥 导入快照即可恢复
                    </div>
                  </div>
                )}
              </span>
            );
          })()}
          <span style={{ fontSize: "10px", color: "var(--pet-color-muted)" }}>{showSessionList ? "▲" : "▼"}</span>
        </div>
        <button
          onClick={() => {
            setShowSessionList(false);
            setSearchMode((prev) => !prev);
            setSearchQuery("");
            setSearchResults([]);
            // R96: 关闭 search 时复位 scope，下次进默认全局
            setSearchScope("all");
          }}
          style={{ ...newSessionBtnStyle, color: searchMode ? "var(--pet-tint-blue-fg)" : "var(--pet-color-fg)", background: searchMode ? "var(--pet-tint-blue-bg)" : "var(--pet-color-bg)" }}
          title="搜索消息（默认全部会话；进面板后可切换『本会话』）"
          aria-label="cross-session search"
        >
          🔍
        </button>
        {/* 📋 复制最近 N 轮 dropdown：click 弹 popover 选 N（1/5/10/20/50），
            把当前 session 的 user/assistant 消息（过滤 tool/error/systemNote）
            末 N 条带 glyph 前缀拼成 markdown 写剪贴板。导出对话便于贴 issue /
            分享。仅 items 非空时显避免空状态无 op。 */}
        {items.length > 0 && (
          <span
            style={{ position: "relative", display: "inline-block" }}
            onMouseDown={(e) => e.stopPropagation()}
          >
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                setCopyRecentMenuOpen((v) => !v);
              }}
              style={{
                ...newSessionBtnStyle,
                color: copyRecentMenuOpen
                  ? "var(--pet-tint-blue-fg)"
                  : "var(--pet-color-fg)",
                background: copyRecentMenuOpen
                  ? "var(--pet-tint-blue-bg)"
                  : "var(--pet-color-bg)",
              }}
              title="复制最近 N 轮 user/assistant 消息（去 tool / error / systemNote 行）到剪贴板。N=1/5/10/20/50 dropdown 选。"
              aria-label="copy recent N rounds"
            >
              📋
            </button>
            {copyRecentMenuOpen && (
              <div
                onMouseDown={(e) => e.stopPropagation()}
                onClick={(e) => e.stopPropagation()}
                style={{
                  position: "absolute",
                  top: "calc(100% + 4px)",
                  right: 0,
                  minWidth: 140,
                  padding: 4,
                  background: "var(--pet-color-card)",
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 6,
                  boxShadow: "0 4px 12px rgba(0,0,0,0.18)",
                  zIndex: 40,
                  display: "flex",
                  flexDirection: "column",
                  gap: 2,
                }}
              >
                <div
                  style={{
                    padding: "4px 9px",
                    fontSize: 10,
                    color: "var(--pet-color-muted)",
                    borderBottom: "1px solid var(--pet-color-border)",
                    marginBottom: 2,
                  }}
                >
                  📋 复制最近 N 条
                </div>
                {[1, 5, 10, 20, 50].map((n) => (
                  <button
                    key={n}
                    type="button"
                    onClick={() => void handleCopyRecentN(n)}
                    onMouseOver={(e) => {
                      (e.currentTarget as HTMLButtonElement).style.background =
                        "var(--pet-color-bg)";
                    }}
                    onMouseOut={(e) => {
                      (e.currentTarget as HTMLButtonElement).style.background =
                        "transparent";
                    }}
                    style={{
                      display: "block",
                      width: "100%",
                      textAlign: "left",
                      padding: "5px 9px",
                      fontSize: 11,
                      border: "none",
                      background: "transparent",
                      color: "var(--pet-color-fg)",
                      cursor: "pointer",
                      fontFamily: "inherit",
                      borderRadius: 4,
                    }}
                  >
                    {`最近 ${n} 条`}
                  </button>
                ))}
              </div>
            )}
          </span>
        )}
        {/* "今日 N 个会话 · M 条" chip：sessionList 里 updated_at 在今日
            的会话数 + 它们 item_count 之和。是会话级近似（per-message
            timestamp 不存在），点击不交互；hover 给 caveat。仅 N>0 时显，
            避免新启动空状态噪音。 */}
        {(() => {
          if (sessionList.length === 0) return null;
          const today = (() => {
            const d = new Date();
            const y = d.getFullYear();
            const m = String(d.getMonth() + 1).padStart(2, "0");
            const da = String(d.getDate()).padStart(2, "0");
            return `${y}-${m}-${da}`;
          })();
          const todaySessions = sessionList.filter((s) =>
            s.updated_at.startsWith(today),
          );
          if (todaySessions.length === 0) return null;
          const itemSum = todaySessions.reduce(
            (acc, s) => acc + (s.item_count ?? 0),
            0,
          );
          const todayActive = sessionFilter === "today";
          return (
            <button
              type="button"
              onClick={() => {
                void toggleSessionFilter("today");
                setShowSessionList(true);
              }}
              style={{
                ...newSessionBtnStyle,
                color: todayActive
                  ? "var(--pet-tint-blue-fg)"
                  : "var(--pet-color-muted)",
                background: todayActive
                  ? "var(--pet-tint-blue-bg)"
                  : "var(--pet-color-bg)",
                borderColor: todayActive
                  ? "var(--pet-color-accent)"
                  : undefined,
                cursor: "pointer",
                fontVariantNumeric: "tabular-nums",
                fontWeight: todayActive ? 600 : undefined,
              }}
              title={
                todayActive
                  ? `已开"今日"过滤，session 下拉只显今日 ${todaySessions.length} 个；再点关闭`
                  : `今日活跃过 ${todaySessions.length} 个会话（updated_at 在今天）；它们累计 ${itemSum} 条消息（含 user / assistant / tool / error 行，不含 system）。点击在 session 下拉里只显今日。`
              }
              aria-label="today activity filter"
            >
              📅 {todaySessions.length} · {itemSum}
            </button>
          );
        })()}
        {/* "📌 查看全部标记消息"按钮：仅 markedMessages 非空时显，count
            badge 显当前标记数量。点击 open marks modal 异步加载所有 mark
            过的 session 内容并展示。 */}
        {markedMessages.size > 0 && (
          <button
            onClick={() => {
              setShowSessionList(false);
              void openMarksModal();
            }}
            style={{
              ...newSessionBtnStyle,
              color: "var(--pet-tint-yellow-fg)",
              background: "var(--pet-tint-yellow-bg)",
            }}
            title={`查看全部 📌 标记的消息（${markedMessages.size} 条）`}
            aria-label="view all marked messages"
          >
            📌 {markedMessages.size}
          </button>
        )}
        {/* ⑂ Fork 当前 session：弹三档选项（整段 / 近 20 / 近 10）。仅当
            前 session 非空时显（空 session fork = 等同新建空 session）。 */}
        {items.length > 0 && (
          <div
            style={{ position: "relative" }}
            onMouseDown={(e) => e.stopPropagation()}
          >
            <button
              onClick={(e) => {
                e.stopPropagation();
                setForkPopoverOpen((v) => !v);
              }}
              style={newSessionBtnStyle}
              title={`从当前 ${items.length} 条消息分叉一个新 session 接着讨论另一话题`}
              aria-label="fork session"
            >
              ⑂ Fork
            </button>
            {forkPopoverOpen && (
              <div
                onClick={(e) => e.stopPropagation()}
                style={{
                  position: "absolute",
                  top: "calc(100% + 6px)",
                  right: 0,
                  minWidth: 200,
                  background: "var(--pet-color-card)",
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 6,
                  boxShadow: "var(--pet-shadow-md)",
                  padding: 6,
                  zIndex: 30,
                  fontSize: 12,
                }}
              >
                <div
                  style={{
                    fontSize: 11,
                    color: "var(--pet-color-muted)",
                    padding: "2px 8px 6px",
                    borderBottom: "1px solid var(--pet-color-border)",
                    marginBottom: 4,
                  }}
                >
                  从「{sessionTitle}」分叉
                </div>
                {(
                  [
                    { keep: Infinity, label: "整段复制", note: `${items.length} 条` },
                    {
                      keep: Math.min(20, items.length),
                      label: "近 20 条",
                      note: items.length < 20 ? "（不够，全部）" : "末段 20",
                    },
                    {
                      keep: Math.min(10, items.length),
                      label: "近 10 条",
                      note: items.length < 10 ? "（不够，全部）" : "末段 10",
                    },
                  ]
                ).map((opt) => (
                  <button
                    key={opt.label}
                    type="button"
                    onClick={() => {
                      setForkPopoverOpen(false);
                      void handleForkSession(opt.keep);
                    }}
                    style={{
                      display: "block",
                      width: "100%",
                      textAlign: "left",
                      padding: "5px 8px",
                      marginBottom: 2,
                      fontSize: 12,
                      border: "1px solid var(--pet-color-border)",
                      borderRadius: 4,
                      background: "var(--pet-color-card)",
                      color: "var(--pet-color-fg)",
                      cursor: "pointer",
                      fontFamily: "inherit",
                    }}
                  >
                    {opt.label}
                    <span
                      style={{
                        fontSize: 10,
                        marginLeft: 6,
                        color: "var(--pet-color-muted)",
                      }}
                    >
                      {opt.note}
                    </span>
                  </button>
                ))}
                <div
                  style={{
                    fontSize: 10,
                    color: "var(--pet-color-muted)",
                    marginTop: 4,
                    padding: "0 4px",
                    lineHeight: 1.4,
                  }}
                >
                  fork 后自动切到新 session；原 session 不动
                </div>
              </div>
            )}
          </div>
        )}
        <button
          onClick={handleNewSession}
          style={newSessionBtnStyle}
          title="新建会话（也可按 ⌘N / Ctrl+N）"
        >
          + 新会话
        </button>
      </div>

      {/* Session tab 横排栏：pinned + 最近活跃 + 当前 session 必显 (cap 8)。
          tab-like 视觉：active 突出 + 加底边 accent，非 active 浅灰底。溢出
          时整行 overflowX auto 横滚（用户拖滚条 / 触控板横滚）。dropdown 入
          口仍在 session header（▼），覆盖 8 个之外的旧会话。仅在 search
          mode 关闭 + sessionList ≥ 2 条时显（单条 session 显 tab 无意义）。 */}
      {!searchMode && tabSessions.length >= 2 && (
        <div
          style={{
            display: "flex",
            alignItems: "stretch",
            gap: 2,
            padding: "4px 8px",
            borderBottom: "1px solid var(--pet-color-border)",
            background: "var(--pet-color-bg)",
            overflowX: "auto",
            overflowY: "hidden",
            flexShrink: 0,
            scrollbarWidth: "thin",
          }}
          title="最常用 / 最近会话快速切换 · ▼ 看全部"
        >
          {tabSessions.map((s) => {
            const active = s.id === sessionId;
            const shownTitle =
              s.title.length > 12 ? s.title.slice(0, 12) + "…" : s.title;
            return (
              <button
                key={s.id}
                type="button"
                onClick={() => {
                  if (s.id === sessionId) return;
                  setShowSessionList(false);
                  void loadSession(s.id);
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setSessionTabCtxMenu({
                    id: s.id,
                    title: s.title,
                    pinned: !!s.pinned,
                    x: e.clientX,
                    y: e.clientY,
                  });
                }}
                title={`${s.pinned ? "📌 " : ""}${s.title}${
                  s.item_count != null ? ` · ${s.item_count} 条` : ""
                } · 右键改名 / pin / 删除`}
                style={{
                  padding: "4px 10px",
                  fontSize: 11,
                  border: "1px solid",
                  borderColor: active
                    ? "var(--pet-color-accent)"
                    : "var(--pet-color-border)",
                  borderRadius: "6px 6px 2px 2px",
                  background: active
                    ? "var(--pet-color-card)"
                    : "var(--pet-color-card)",
                  color: active
                    ? "var(--pet-color-accent)"
                    : "var(--pet-color-muted)",
                  fontWeight: active ? 600 : 400,
                  cursor: active ? "default" : "pointer",
                  whiteSpace: "nowrap",
                  flexShrink: 0,
                  fontFamily: "inherit",
                  borderBottomWidth: active ? 0 : 1,
                  borderBottom: active
                    ? "2px solid var(--pet-color-accent)"
                    : "1px solid var(--pet-color-border)",
                  marginBottom: -1,
                }}
              >
                {s.pinned && <span style={{ marginRight: 3 }}>📌</span>}
                {shownTitle}
              </button>
            );
          })}
          {sessionList.length > tabSessions.length && (
            <button
              type="button"
              onClick={() => setShowSessionList(true)}
              title={`还有 ${sessionList.length - tabSessions.length} 个会话；点 ⋯ 展开全部下拉`}
              style={{
                padding: "4px 10px",
                fontSize: 11,
                border: "1px dashed var(--pet-color-border)",
                borderRadius: 6,
                background: "transparent",
                color: "var(--pet-color-muted)",
                cursor: "pointer",
                whiteSpace: "nowrap",
                flexShrink: 0,
                fontFamily: "inherit",
              }}
            >
              ⋯ +{sessionList.length - tabSessions.length}
            </button>
          )}
        </div>
      )}

      {/* Search panel (取代 session dropdown 当 search mode 开启) */}
      {searchMode && (
        <div style={sessionDropdownStyle}>
          <div style={{ padding: "8px 12px", borderBottom: "1px solid var(--pet-color-border)", display: "flex", gap: 6 }}>
            <input
              type="text"
              autoFocus
              placeholder={searchScope === "current" ? "搜本会话…" : "按关键字搜索全部会话…"}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              list="pet-chat-search-history"
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  setSearchMode(false);
                  setSearchQuery("");
                  setSearchResults([]);
                  setSearchScope("all");
                  return;
                }
                // Enter：把当前 query 入 history datalist。live filter 已在
                // onChange 即时生效，Enter 是"用得满意 / 记一下"的显式信号。
                if (e.key === "Enter" && searchQuery.trim()) {
                  e.preventDefault();
                  pushChatSearchHistory(searchQuery);
                }
              }}
              style={{ flex: 1, padding: "6px 10px", border: "1px solid var(--pet-color-border)", borderRadius: 4, fontSize: 13, color: "var(--pet-color-fg)", background: "var(--pet-color-card)" }}
            />
            {chatSearchHistory.length > 0 && (
              <datalist id="pet-chat-search-history">
                {chatSearchHistory.map((kw) => (
                  <option key={kw} value={kw} />
                ))}
              </datalist>
            )}
            {/* R96: scope 双键 pill toggle —— 全部 vs 本会话。active 态走 card
                fg，inactive 走 bg muted（与系统 toggle pill 配色一致）。 */}
            <div
              style={{
                display: "flex",
                gap: 0,
                alignItems: "center",
                padding: 1,
                background: "var(--pet-color-bg)",
                borderRadius: 4,
              }}
            >
              {(["all", "current"] as const).map((scope) => {
                const active = searchScope === scope;
                return (
                  <button
                    key={scope}
                    type="button"
                    onClick={() => setSearchScope(scope)}
                    style={{
                      padding: "2px 8px",
                      fontSize: 11,
                      border: "none",
                      borderRadius: 3,
                      background: active ? "var(--pet-color-card)" : "transparent",
                      color: active ? "var(--pet-color-fg)" : "var(--pet-color-muted)",
                      cursor: active ? "default" : "pointer",
                      fontWeight: active ? 600 : 400,
                    }}
                    title={
                      scope === "all"
                        ? "搜全部历史会话"
                        : "只搜当前打开会话（更精准，结果不被其它 session 抢限额）"
                    }
                  >
                    {scope === "all" ? "全部" : "本会话"}
                  </button>
                );
              })}
            </div>
            {searchQuery && (
              <button
                type="button"
                onClick={() => setSearchQuery("")}
                style={{ padding: "0 10px", border: "1px solid var(--pet-color-border)", borderRadius: 4, background: "var(--pet-color-card)", color: "var(--pet-color-muted)", cursor: "pointer", fontSize: 12 }}
                title="清空"
              >
                ✕
              </button>
            )}
          </div>
          {!searchQuery.trim() ? (
            <div style={{ padding: "12px", textAlign: "center", color: "var(--pet-color-muted)", fontSize: "12px" }}>
              {searchScope === "current"
                ? "输入关键字搜本会话内的消息"
                : "输入关键字查找跨会话消息（user / assistant）"}
            </div>
          ) : searchResults.length === 0 ? (
            <div style={{ padding: "12px", textAlign: "center", color: "var(--pet-color-muted)", fontSize: "12px" }}>
              {searchScope === "current" ? "本会话没有匹配的消息" : "没有匹配的消息"}
            </div>
          ) : (
            (() => {
              // 跨会话搜索结果按月份分组：仅 searchScope === "all" 且命中
              // > 20 条时启用 —— 单会话搜索 (current 模式) 所有 hit 共享同
              // 一 session.updated_at 月份，分组无意义。"本月 / 上月 / YYYY-MM /
              // 更早" 与 session 下拉同 4 档 label（共用 module-level helper）。
              const enableGrouping =
                searchScope === "all" && searchResults.length > 20;
              const groupingNow = new Date();
              const headerByIdx = new Map<
                number,
                { key: string; label: string; count: number }
              >();
              if (enableGrouping) {
                let curKey: string | null = null;
                let curStart = 0;
                const flush = (endExclusive: number) => {
                  if (curKey === null) return;
                  headerByIdx.set(curStart, {
                    key: curKey,
                    label: monthLabelOf(curKey),
                    count: endExclusive - curStart,
                  });
                };
                for (let i = 0; i < searchResults.length; i++) {
                  const key = monthKeyFromIso(
                    searchResults[i].session_updated_at,
                    groupingNow,
                  );
                  if (key !== curKey) {
                    flush(i);
                    curKey = key;
                    curStart = i;
                  }
                }
                flush(searchResults.length);
              }
              return searchResults.map((hit, idx) => (
                <Fragment key={`${hit.session_id}-${hit.item_index}`}>
                  {headerByIdx.get(idx) && (() => {
                    const h = headerByIdx.get(idx)!;
                    return (
                      <div
                        style={{
                          padding: "6px 12px 4px",
                          fontSize: 11,
                          fontWeight: 600,
                          color: "var(--pet-color-muted)",
                          background: "var(--pet-color-bg)",
                          borderBottom: "1px solid var(--pet-color-border)",
                          letterSpacing: 0.3,
                          userSelect: "none",
                          position: "sticky",
                          top: 0,
                          zIndex: 1,
                        }}
                      >
                        {h.label}（{h.count}）
                      </div>
                    );
                  })()}
                  <SearchResultRow
                    hit={hit}
                    onSelect={handleSelectSearchHit}
                  />
                </Fragment>
              ));
            })()
          )}
        </div>
      )}

      {/* Session list dropdown */}
      {showSessionList && !searchMode && (
        <div style={sessionDropdownStyle}>
          {/* 全量 snapshot 工具栏：导出 / 导入。armed-import 与 config 快照同模式
              （5s 二次确认）。export 不需 confirm，但 toast 提示含敏感聊天明文。 */}
          <div
            style={{
              padding: "6px 12px",
              borderBottom: "1px solid var(--pet-color-border)",
              display: "flex",
              gap: 6,
              alignItems: "center",
              flexWrap: "wrap",
            }}
          >
            <button
              type="button"
              onClick={() => void handleExportSessionsSnapshot()}
              style={{
                fontSize: 11,
                padding: "2px 8px",
                borderRadius: 4,
                border: "1px solid var(--pet-color-border)",
                background: "var(--pet-color-card)",
                color: "var(--pet-color-muted)",
                cursor: "pointer",
              }}
              title="把全部 session 打包成 base64 字符串复制到剪贴板（搬家用，不上云）。⚠ 含全部聊天明文。"
            >
              📦 导出全部 sessions
            </button>
            <button
              type="button"
              onClick={() => void handleImportSessionsSnapshot()}
              style={{
                fontSize: 11,
                padding: "2px 8px",
                borderRadius: 4,
                border: `1px solid ${importSessionsArmed ? "var(--pet-tint-red-fg)" : "var(--pet-color-border)"}`,
                background: importSessionsArmed ? "var(--pet-tint-red-fg)" : "var(--pet-color-card)",
                color: importSessionsArmed ? "#fff" : "var(--pet-color-muted)",
                cursor: "pointer",
                fontWeight: importSessionsArmed ? 700 : 400,
              }}
              title="读剪贴板里的 sessions snapshot 覆盖当前 index 和所有 session 文件。第一次点弹确认；5 秒内再点真覆盖。"
            >
              {importSessionsArmed ? "⚠ 确认导入？" : "📥 导入快照"}
            </button>
            {/* 清理 orphan checkbox：勾后导入时一并 rm disk 上不在 snapshot
                里的 session 文件。新机干净接收典型用法；老机想保留本地历史
                则取消勾。 */}
            <label
              style={{
                fontSize: 11,
                color: "var(--pet-color-muted)",
                display: "flex",
                alignItems: "center",
                gap: 3,
                cursor: "pointer",
                userSelect: "none",
              }}
              title="导入时同时删 disk 上不在 snapshot 里的 session.json（清理 orphan）。不勾则保留本地老 session 文件（但 index 不显，下拉看不见）。"
            >
              <input
                type="checkbox"
                checked={pruneSessionsOnImport}
                onChange={(e) => setPruneSessionsOnImport(e.target.checked)}
                style={{ margin: 0 }}
              />
              清 orphan
            </label>
            {/* 全清按钮：armed 红填充 + 二次确认。让用户彻底重置聊天历史
                （不动 memory / SOUL / config）。marginLeft auto 推到行末与
                上面 export/import 区分语义（清除性 vs 迁移性）。 */}
            {/* 🧹 清碎片：item_count ≤ 3 + 非 pinned + 非 active 的会话一键
                扫掉。owner 多次 /reset 后会积累一堆 1-2 条对话就放弃的碎片。
                armed 二次确认同 🗑 全清模板，5s 自动 disarm。 */}
            {(() => {
              const fragCount = sessionList.filter(
                (s) =>
                  (s.item_count ?? 0) <= 3 &&
                  !s.pinned &&
                  s.id !== sessionId,
              ).length;
              return (
                <button
                  type="button"
                  onClick={() => void handlePurgeFragmentSessions()}
                  disabled={fragCount === 0 && !purgeFragArmed}
                  style={{
                    marginLeft: "auto",
                    fontSize: 11,
                    padding: "2px 8px",
                    borderRadius: 4,
                    border: `1px solid ${purgeFragArmed ? "var(--pet-tint-red-fg)" : "var(--pet-color-border)"}`,
                    background: purgeFragArmed
                      ? "var(--pet-tint-red-fg)"
                      : "var(--pet-color-card)",
                    color: purgeFragArmed ? "#fff" : "var(--pet-color-muted)",
                    cursor:
                      fragCount === 0 && !purgeFragArmed
                        ? "default"
                        : "pointer",
                    opacity: fragCount === 0 && !purgeFragArmed ? 0.5 : 1,
                    fontWeight: purgeFragArmed ? 700 : 400,
                  }}
                  title={
                    fragCount === 0
                      ? "没有碎片 session 可清（碎片 = item_count ≤ 3 + 未钉住 + 非当前激活）"
                      : `清掉 ${fragCount} 个碎片 session（item_count ≤ 3 + 未钉住 + 非当前激活）。第一次点弹确认；5 秒内再点真清。`
                  }
                >
                  {purgeFragArmed
                    ? `⚠ 确认清 ${fragCount}？`
                    : `🧹 清碎片${fragCount > 0 ? ` (${fragCount})` : ""}`}
                </button>
              );
            })()}
            <button
              type="button"
              onClick={() => void handleClearAllSessions()}
              style={{
                fontSize: 11,
                padding: "2px 8px",
                borderRadius: 4,
                border: `1px solid ${clearAllArmed ? "var(--pet-tint-red-fg)" : "var(--pet-color-border)"}`,
                background: clearAllArmed ? "var(--pet-tint-red-fg)" : "var(--pet-color-card)",
                color: clearAllArmed ? "#fff" : "var(--pet-color-muted)",
                cursor: "pointer",
                fontWeight: clearAllArmed ? 700 : 400,
              }}
              title="清空全部 session 历史（仅聊天，不动 memory / SOUL / config）。第一次点弹确认；5 秒内再点真清。"
            >
              {clearAllArmed ? "⚠ 确认全清？" : "🗑 全清"}
            </button>
          </div>
          {/* 内容过滤 toggle 行：两个 chip 互斥，同时只能开一个；同一 chip
              再点 → 关。imageSessionIds 模式由 sessionFilter+filterSessionIds 协调。 */}
          <div
            style={{
              padding: "6px 12px",
              borderBottom: "1px solid var(--pet-color-border)",
              display: "flex",
              gap: 6,
              alignItems: "center",
              flexWrap: "wrap",
            }}
          >
            {(() => {
              // 📌 钉住 chip 仅当 sessionList 含 pinned 会话时才出现 ——
              // 用户没钉过任何会话时这个 chip 是噪音。pinned 会话本来就浮顶
              // （后端 list_sessions 排序）；chip 提供"只看钉住"过滤路径。
              const hasPinnedSession = sessionList.some((s) => s.pinned);
              const baseChips = [
                { kind: "today" as const, label: "📅 今日", desc: "只显今日活跃过的会话（updated_at 在今天）" },
                { kind: "images" as const, label: "📷 含图片", desc: "只显含图片消息的 session（粘贴 / 生图过的）" },
                { kind: "tasks" as const, label: "📋 含派单", desc: "只显含 propose_task / task_create 工具调用的 session（工作场景）" },
              ];
              const chips = hasPinnedSession
                ? [
                    ...baseChips,
                    {
                      kind: "pinned" as const,
                      label: "📌 钉住",
                      desc: "只显已钉住的会话（与 /pin 切换 + 列表浮顶同源）",
                    },
                  ]
                : baseChips;
              return chips;
            })().map(({ kind, label, desc }) => {
              const active = sessionFilter === kind;
              const isLoading = active && filterLoading;
              const count = active && filterSessionIds ? filterSessionIds.size : null;
              return (
                <button
                  key={kind}
                  type="button"
                  onClick={() => void toggleSessionFilter(kind)}
                  disabled={isLoading}
                  style={{
                    fontSize: 11,
                    padding: "2px 8px",
                    borderRadius: 10,
                    border: `1px solid ${active ? "var(--pet-color-accent)" : "var(--pet-color-border)"}`,
                    background: active ? "var(--pet-tint-blue-bg)" : "var(--pet-color-card)",
                    color: active ? "var(--pet-tint-blue-fg)" : "var(--pet-color-muted)",
                    cursor: isLoading ? "default" : "pointer",
                    fontWeight: active ? 600 : 400,
                  }}
                  title={
                    active
                      ? `再点关闭过滤${count !== null ? `（命中 ${count} 个 session）` : ""}`
                      : desc
                  }
                >
                  {isLoading
                    ? `${label.split(" ")[0]} 加载中…`
                    : active && count !== null
                      ? `✓ ${label} (${count})`
                      : label}
                </button>
              );
            })}
          </div>
          {/* R106: 导出 toast。3s 自清空；非空时贴顶横幅，accent 色提示。 */}
          {exportToast && (
            <div
              style={{
                padding: "6px 12px",
                fontSize: 11,
                color: "var(--pet-color-accent)",
                background: "var(--pet-color-bg)",
                borderBottom: "1px solid var(--pet-color-border)",
              }}
            >
              {exportToast}
            </div>
          )}
          {/* 标题搜索框：sessionList.length > 5 时浮出，与既有 chip filter
              组合（chip 过滤后再按 title 子串过滤）。autoFocus 让用户开下
              拉就能敲；clear 由 effect 在 setShowSessionList(false) 时自动
              做。`renamingId` 在键时段无任何冲突 —— 那是另一个 input 的 focus。 */}
          {sessionList.length > 5 && (
            <div
              style={{
                padding: "6px 12px",
                borderBottom: "1px solid var(--pet-color-border)",
                display: "flex",
                gap: 6,
                alignItems: "center",
              }}
            >
              <input
                type="text"
                value={sessionTitleQuery}
                onChange={(e) => setSessionTitleQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") {
                    e.preventDefault();
                    if (sessionTitleQuery) {
                      setSessionTitleQuery("");
                    } else {
                      setShowSessionList(false);
                    }
                  }
                }}
                placeholder="按标题筛选…（Esc 清空 / 关下拉）"
                style={{
                  flex: 1,
                  padding: "4px 8px",
                  fontSize: 11,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 4,
                  background: "var(--pet-color-card)",
                  color: "var(--pet-color-fg)",
                  outline: "none",
                  fontFamily: "inherit",
                }}
              />
              {sessionTitleQuery && (
                <button
                  type="button"
                  onClick={() => setSessionTitleQuery("")}
                  title="清空标题筛选"
                  style={{
                    fontSize: 11,
                    padding: "2px 8px",
                    borderRadius: 4,
                    border: "1px solid var(--pet-color-border)",
                    background: "var(--pet-color-card)",
                    color: "var(--pet-color-muted)",
                    cursor: "pointer",
                  }}
                >
                  ✕
                </button>
              )}
            </div>
          )}
          {(() => {
            // pinned 永远排前；同组内按 backend index 倒序（最近创建在前）。
            // 不动原 sessionList，复制再 stable-sort —— `[].reverse()` 后用
            // pin 优先级再分组：pinned 整段在 unpinned 之前。
            // 含图过滤命中 0 → 显单独的 empty message 提醒用户"过滤生效但无匹配"。
            if (sessionList.length === 0) {
              return (
                <EmptyState icon="📂" title="暂无历史会话" hint="开始聊天就会自动新建会话" compact />
              );
            }
            const reversed = [...sessionList].reverse();
            const chipFiltered =
              sessionFilter !== null && filterSessionIds !== null
                ? reversed.filter((s) => filterSessionIds.has(s.id))
                : reversed;
            // 标题过滤：trim + case-insensitive 子串。空 query 直通。与 chip
            // 过滤 AND 组合（两层都要满足）—— 让"📋 含派单 + 标题含 Downloads"
            // 一类组合查询自然命中。
            const titleQuery = sessionTitleQuery.trim().toLowerCase();
            const filtered = titleQuery
              ? chipFiltered.filter((s) =>
                  s.title.toLowerCase().includes(titleQuery),
                )
              : chipFiltered;
            if (filtered.length === 0 && (sessionFilter !== null || titleQuery)) {
              // 区分两种"无匹配"反馈：单独 chip / 单独 title / 两者都开。
              const chipLabel = sessionFilter === "images" ? "📷" : "📋";
              const reason =
                sessionFilter !== null && titleQuery
                  ? `chip 「${chipLabel}」与标题「${titleQuery}」组合无命中`
                  : sessionFilter !== null
                    ? "chip 过滤无命中"
                    : `没有标题含「${titleQuery}」的会话`;
              const hint =
                sessionFilter !== null && titleQuery
                  ? "改 chip 或清标题再试"
                  : sessionFilter !== null
                    ? `点 ${chipLabel} 关闭过滤`
                    : "Esc 清空筛选";
              return (
                <EmptyState
                  icon="🔍"
                  title={reason}
                  hint={hint}
                  compact
                />
              );
            }
            if (filtered.length === 0) {
              // 既无 chip / 无 title query 但 list 空 —— 上面 sessionList.length===0
              // 早 return 了，这里其实不可达；防御保留。
              return null;
            }
            const pinned = filtered.filter((s) => s.pinned);
            const unpinned = filtered.filter((s) => !s.pinned);
            const ordered = [...pinned, ...unpinned];
            // 月份分组开关：sessionList > 20 时启用 —— 用户已积累足够历史，分
            // 段比平铺更易扫。filter 收窄到 5 条仍按 sessionList 总量 gate，避
            // 免"chip 临时过滤后突然无 header"的认知抖动。
            const enableGrouping = sessionList.length > 20;
            // 月份 key / label 算法走 module-level helper（monthKeyFromIso /
            // monthLabelOf），与跨会话搜索结果分组共用 —— `now` 一次性 new Date()
            // 让本 IIFE 内多条 session 复用同一 wall clock 快照。
            const groupingNow = new Date();
            // 预扫一遍：哪些 idx 应该在前面插 group header + 该 group 的成员
            // 总数。同组连续 → 仅在第一个 idx 上挂 header。
            const headerByIdx = new Map<
              number,
              { key: string; label: string; count: number }
            >();
            if (enableGrouping) {
              let curKey: string | null = null;
              let curStart = 0;
              const flush = (endExclusive: number) => {
                if (curKey === null) return;
                headerByIdx.set(curStart, {
                  key: curKey,
                  label: monthLabelOf(curKey),
                  count: endExclusive - curStart,
                });
              };
              for (let i = 0; i < ordered.length; i++) {
                const key = ordered[i].pinned
                  ? "_pinned"
                  : monthKeyFromIso(ordered[i].updated_at, groupingNow);
                if (key !== curKey) {
                  flush(i);
                  curKey = key;
                  curStart = i;
                }
              }
              flush(ordered.length);
            }
            return ordered.map((s, idx) => (
              <Fragment key={s.id}>
                {headerByIdx.get(idx) && (() => {
                  const h = headerByIdx.get(idx)!;
                  return (
                    <div
                      style={{
                        padding: "6px 12px 4px",
                        fontSize: 11,
                        fontWeight: 600,
                        color: "var(--pet-color-muted)",
                        background: "var(--pet-color-bg)",
                        borderBottom: "1px solid var(--pet-color-border)",
                        letterSpacing: 0.3,
                        userSelect: "none",
                        position: "sticky",
                        top: 0,
                        zIndex: 1,
                      }}
                    >
                      {h.label}（{h.count}）
                    </div>
                  );
                })()}
              <div
                className="pet-session-row"
                onMouseEnter={() => handleSessionPreviewEnter(s.id)}
                onMouseLeave={handleSessionPreviewLeave}
                onContextMenu={(e) => {
                  // 与顶 tab bar session 同 ctx menu（既有 sessionTabCtxMenu），
                  // 让 dropdown 行也能右键 pin / rename / 复制标题 / 复制 ID /
                  // 重写标题。owner 在长 session 列表里就近右键比"先 click
                  // 进 tab → 右键 tab" 两步顺。
                  e.preventDefault();
                  e.stopPropagation();
                  setSessionTabCtxMenu({
                    id: s.id,
                    title: s.title,
                    pinned: !!s.pinned,
                    x: e.clientX,
                    y: e.clientY,
                  });
                }}
                style={{
                  display: "flex",
                  flexDirection: "column",
                  alignItems: "stretch",
                  gap: "4px",
                  padding: "8px 12px",
                  cursor: "pointer",
                  background: s.id === sessionId ? "var(--pet-tint-blue-bg)" : "transparent",
                  borderBottom: "1px solid var(--pet-color-border)",
                }}
              >
              <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                <div
                  style={{ flex: 1, minWidth: 0 }}
                  onClick={() => {
                    if (renamingId !== s.id) switchSession(s.id);
                  }}
                >
                  {renamingId === s.id ? (
                    /* R101: inline rename input。Enter / blur 提交；Esc 取消；
                       click stopPropagation 防 switchSession 触发。 */
                    <input
                      autoFocus
                      value={renameDraft}
                      onChange={(e) => setRenameDraft(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          void commitRename();
                        } else if (e.key === "Escape") {
                          e.preventDefault();
                          cancelRename();
                        }
                      }}
                      onBlur={() => void commitRename()}
                      onClick={(e) => e.stopPropagation()}
                      style={{
                        width: "100%",
                        padding: "2px 6px",
                        fontSize: 13,
                        border: "1px solid var(--pet-color-accent)",
                        borderRadius: 3,
                        background: "var(--pet-color-card)",
                        color: "var(--pet-color-fg)",
                        outline: "none",
                        boxSizing: "border-box",
                      }}
                    />
                  ) : (
                    <>
                      <div style={{ fontSize: "13px", color: "var(--pet-color-fg)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontWeight: s.id === sessionId ? 600 : 400 }}>
                        {/* 未读 badge：仅在非当前 session 且用户至少访问过一次
                            (lastSeen 有记录) + s.updated_at 比 lastSeen 新时显。
                            首次启动用户没访问过任何 session → 全部默认已读，
                            不打扰新用户。 */}
                        {(() => {
                          if (s.id === sessionId) return null;
                          const seen = sessionLastSeen[s.id];
                          if (!seen) return null;
                          if (s.updated_at <= seen) return null;
                          return (
                            <span
                              style={{
                                display: "inline-block",
                                width: 8,
                                height: 8,
                                borderRadius: "50%",
                                background: "var(--pet-color-accent)",
                                marginRight: 6,
                                verticalAlign: "middle",
                              }}
                              title={`自上次访问（${seen.slice(0, 16).replace("T", " ")}）后此会话有更新`}
                            />
                          );
                        })()}
                        {s.title}
                        {typeof s.item_count === "number" && (
                          <span
                            style={{ fontWeight: 400, color: "var(--pet-color-muted)", marginLeft: 6 }}
                            title="会话内可见对话条数（不含 system message）"
                          >
                            ({s.item_count} 条)
                          </span>
                        )}
                      </div>
                      <div
                        style={{ fontSize: "11px", color: "var(--pet-color-muted)" }}
                        title={s.updated_at.replace("T", " ").slice(0, 16)}
                      >
                        {(() => {
                          // 今天 / 昨天 / 否则原日期。当地时区前缀（sv-SE 给 ISO
                          // YYYY-MM-DD 但走 local），与 updated_at 的本地 ISO 兼容。
                          // hover title 显完整 "YYYY-MM-DD HH:MM" 供精确查看。
                          const date = s.updated_at.slice(0, 10);
                          const now = new Date();
                          const today = now.toLocaleDateString("sv-SE");
                          const yest = new Date(now.getTime() - 86_400_000)
                            .toLocaleDateString("sv-SE");
                          if (date === today) return "今天";
                          if (date === yest) return "昨天";
                          return date;
                        })()}
                      </div>
                    </>
                  )}
                </div>
                {renamingId !== s.id && (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      void handleTogglePinned(s.id, !s.pinned);
                    }}
                    style={{
                      padding: "2px 6px",
                      borderRadius: "4px",
                      border: "none",
                      background: s.pinned ? "var(--pet-tint-yellow-bg)" : "transparent",
                      color: s.pinned ? "var(--pet-tint-yellow-fg)" : "var(--pet-color-muted)",
                      fontSize: "12px",
                      cursor: "pointer",
                      opacity: s.pinned ? 1 : 0.7,
                    }}
                    title={s.pinned ? "取消钉住（恢复按时间排序）" : "钉住到列表顶部"}
                    aria-label={s.pinned ? "unpin session" : "pin session"}
                  >
                    📌
                  </button>
                )}
                {renamingId !== s.id && (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      void handleExportSession(s.id, s.title);
                    }}
                    style={{
                      padding: "2px 6px",
                      borderRadius: "4px",
                      border: "none",
                      background: "transparent",
                      color: "var(--pet-color-muted)",
                      fontSize: "12px",
                      cursor: "pointer",
                    }}
                    title="把会话全部 user / assistant 消息复制为 markdown 到剪贴板"
                    aria-label="export session"
                  >
                    📋
                  </button>
                )}
                {renamingId !== s.id && (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      startRename(s);
                    }}
                    style={{
                      padding: "2px 6px",
                      borderRadius: "4px",
                      border: "none",
                      background: "transparent",
                      color: "var(--pet-color-muted)",
                      fontSize: "12px",
                      cursor: "pointer",
                    }}
                    title="重命名会话"
                    aria-label="rename session"
                  >
                    ✏️
                  </button>
                )}
                {renamingId !== s.id && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDeleteClick(s.id);
                    }}
                    style={{
                      padding: "2px 6px",
                      borderRadius: "4px",
                      border: "none",
                      background: pendingDeleteId === s.id ? "var(--pet-tint-red-fg)" : "var(--pet-tint-red-bg)",
                      color: pendingDeleteId === s.id ? "#fff" : "var(--pet-tint-red-fg)",
                      fontSize: "11px",
                      cursor: "pointer",
                      fontWeight: pendingDeleteId === s.id ? 700 : 400,
                    }}
                    title={pendingDeleteId === s.id ? "再点一次确认删除（5 秒后自动撤回）" : "删除会话"}
                  >
                    {pendingDeleteId === s.id ? "确定？" : "删除"}
                  </button>
                )}
              </div>
              {/* hover 1s 浮的"最近 3 条" preview 段：role glyph + 文字段
                  fixed 3 行高度，避免 layout 抖动；为空时显灰字"(空会话)"
                  让 owner 看出确实是空而非加载中。preview cache 命中即时
                  显，未命中显加载中态。仅非当前 session 显（当前 session
                  主聊天区已可见，preview 冗余）。 */}
              {previewSessionId === s.id && s.id !== sessionId && (
                <div
                  onClick={(e) => {
                    // preview 段 click → 直接切到该 session（与外层标题区
                    // 同 switchSession 路径）。让 owner 看完 preview 决策切
                    // 不必再移到上方标题区。renamingId === s.id 时不切（与
                    // 标题区同 gate）。stopPropagation 防止本 click 被外层
                    // row 二次接收（虽然外层 column flex 容器无 onClick，
                    // 但守一道防回归）。
                    e.stopPropagation();
                    if (renamingId !== s.id) void switchSession(s.id);
                  }}
                  title="点击切到此 session（与点上方标题区同语义）"
                  style={{
                    marginTop: 4,
                    padding: "4px 6px",
                    background: "var(--pet-color-bg)",
                    border: "1px dashed var(--pet-color-border)",
                    borderRadius: 4,
                    fontSize: 10,
                    color: "var(--pet-color-muted)",
                    lineHeight: 1.45,
                    cursor: "pointer",
                  }}
                >
                  {(() => {
                    const cached = previewCache[s.id];
                    if (!cached) return "加载预览中...";
                    if (cached.length === 0) return "（空会话）";
                    return cached.map((it, i) => {
                      const glyph =
                        it.type === "user"
                          ? "🧑"
                          : it.type === "assistant"
                            ? "🐾"
                            : it.type === "tool"
                              ? "🛠"
                              : "⚠";
                      const txt = (it.content ?? "")
                        .replace(/\s+/g, " ")
                        .trim();
                      const snip =
                        txt.length > 80 ? txt.slice(0, 80) + "…" : txt;
                      return (
                        <div
                          key={i}
                          style={{
                            display: "flex",
                            gap: 6,
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                          }}
                        >
                          <span style={{ flexShrink: 0 }}>{glyph}</span>
                          <span
                            style={{
                              overflow: "hidden",
                              textOverflow: "ellipsis",
                              whiteSpace: "nowrap",
                            }}
                          >
                            {snip || <em>（无文字内容）</em>}
                          </span>
                        </div>
                      );
                    });
                  })()}
                </div>
              )}
              </div>
              </Fragment>
            ));
          })()}
        </div>
      )}

      {/* Message list — R103: 外层 relative 让 ↑ 浮动按钮锚定到视口右下，
          不被滚动卷走；内层 height:100% + overflowY:auto 保留原 scroll
          行为。 */}
      <div style={{ flex: 1, position: "relative", overflow: "hidden" }}>
      <div
        ref={scrollRef}
        onScroll={() => {
          const el = scrollRef.current;
          if (!el) return;
          setScrolledFromTop(el.scrollTop > 200);
          // 距底 = scrollHeight - scrollTop - clientHeight。> 200 时浮 ↓
          // 按钮；按 (200, ∞) 阈值匹配 ↑ 的对称语义，让两条逻辑同形。
          const distFromBottom =
            el.scrollHeight - el.scrollTop - el.clientHeight;
          setScrolledFromBottom(distFromBottom > 200);
        }}
        style={{ height: "100%", overflowY: "auto", padding: "16px" }}
      >
        {items.length === 0 && !currentResponse && (
          <EmptyState
            icon="💬"
            title="新会话，开始聊天吧"
            hint="敲 / 看快捷命令 · @ 引用任务 · Shift+Enter 换行"
          />
        )}

        {items.map((item, i) => {
          // 给每条消息挂 data-item-idx，跨会话搜索点击结果后能定位 + 高亮。
          const isHighlighted = highlightedItemIdx === i;
          // 命中行单独传 keyword 给 CopyableMessage，让 bubble 内文本 mark 高亮。
          const hitKeyword =
            searchHit && searchHit.idx === i ? searchHit.keyword : undefined;
          const wrapperBase = (justify: "flex-end" | "flex-start"): React.CSSProperties => ({
            marginBottom: "12px",
            display: "flex",
            justifyContent: justify,
            transition: "background-color 200ms ease-out",
            background: isHighlighted ? "rgba(254, 243, 199, 0.7)" : "transparent",
            borderRadius: 8,
            padding: isHighlighted ? "4px 0" : 0,
          });
          if (item.type === "user") {
            const markKey = `${sessionId}::${i}`;
            // 编辑态：inline textarea 替换 bubble。Enter 提交、Shift+Enter 换
            // 行、Esc 取消，与桌面输入框肌肉记忆一致。图片消息走原 bubble
            // 路径（enterEditMode 已拒绝 images.length > 0 的进入）。
            if (editingItemIdx === i) {
              return (
                <div
                  key={i}
                  data-item-idx={i}
                  style={wrapperBase("flex-end")}
                >
                  <div
                    style={{
                      display: "flex",
                      flexDirection: "column",
                      gap: 6,
                      maxWidth: "85%",
                      width: "100%",
                    }}
                  >
                    <textarea
                      autoFocus
                      value={editingDraft}
                      onChange={(e) => setEditingDraft(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Escape") {
                          e.preventDefault();
                          cancelEditMode();
                        } else if (e.key === "Enter" && !e.shiftKey) {
                          // IME composing 时 Enter 不该触发提交 ——
                          // nativeEvent.isComposing 是 DOM-level 真值
                          // （React SyntheticEvent 没暴露 isComposing）。
                          if ((e.nativeEvent as KeyboardEvent).isComposing) return;
                          e.preventDefault();
                          commitMessageEdit();
                        }
                      }}
                      style={{
                        width: "100%",
                        minHeight: 60,
                        padding: "8px 10px",
                        borderRadius: 8,
                        border:
                          "1px solid color-mix(in srgb, var(--pet-color-accent) 50%, var(--pet-color-border))",
                        background: "var(--pet-color-card)",
                        color: "var(--pet-color-fg)",
                        fontSize: 13,
                        lineHeight: 1.5,
                        resize: "vertical",
                        fontFamily: "inherit",
                        boxSizing: "border-box",
                        boxShadow:
                          "0 0 0 3px color-mix(in srgb, var(--pet-color-accent) 18%, transparent)",
                      }}
                    />
                    <div
                      style={{
                        display: "flex",
                        gap: 6,
                        justifyContent: "flex-end",
                        alignItems: "center",
                        fontSize: 11,
                        color: "var(--pet-color-muted)",
                      }}
                    >
                      <span style={{ marginRight: "auto" }}>
                        Enter 重发 · Shift+Enter 换行 · Esc 取消
                      </span>
                      <button
                        type="button"
                        onClick={cancelEditMode}
                        style={{
                          padding: "4px 12px",
                          fontSize: 12,
                          border: "1px solid var(--pet-color-border)",
                          borderRadius: 6,
                          background: "var(--pet-color-card)",
                          color: "var(--pet-color-fg)",
                          cursor: "pointer",
                        }}
                      >
                        取消
                      </button>
                      <button
                        type="button"
                        onClick={commitMessageEdit}
                        disabled={!editingDraft.trim()}
                        style={{
                          padding: "4px 14px",
                          fontSize: 12,
                          border: "none",
                          borderRadius: 6,
                          background: editingDraft.trim()
                            ? "var(--pet-color-accent)"
                            : "var(--pet-color-border)",
                          color: editingDraft.trim()
                            ? "#fff"
                            : "var(--pet-color-muted)",
                          fontWeight: 600,
                          cursor: editingDraft.trim() ? "pointer" : "not-allowed",
                        }}
                      >
                        保存并重发
                      </button>
                    </div>
                  </div>
                </div>
              );
            }
            const canEdit = !isLoading && !item.images?.length;
            return (
              <div
                key={i}
                onDoubleClick={() => {
                  if (!canEdit) return;
                  // bubble 内的 task-ref token 已在自身 onDoubleClick 里
                  // stopPropagation —— 双击 ref 跳任务的语义优先，本 handler
                  // 不会被触发。其它双击（bubble 空白 / 文字）才进入编辑。
                  enterEditMode(i);
                }}
                title={canEdit ? "双击编辑这条消息并重新生成回复" : undefined}
              >
                <CopyableMessage
                  role="user"
                  content={item.content}
                  itemIdx={i}
                  copied={copiedIdx === i}
                  onCopy={handleCopy}
                  wrapperStyle={wrapperBase("flex-end")}
                  images={item.images}
                  highlightKeyword={hitKeyword}
                  taskRefMap={chatTaskMap}
                  onRefDoubleClick={onRequestFocusTask}
                  marked={markedMessages.has(markKey)}
                  onToggleMark={() => toggleMessageMark(markKey)}
                  onFindSimilar={handleFindSimilarInSession}
                />
              </div>
            );
          }
          if (item.type === "assistant") {
            const hasImg = !!item.images && item.images.length > 0;
            if (!item.content.trim() && !hasImg) return null;
            // /image 失败行：在 bubble 旁挂🔄重试按钮，调 runImageGenerate
            // 替换本行（idx=i）为新 pending → 走完整生图流程。点击后立刻禁
            // 用按钮（替换为 pending 后 imageRetryPrompt 自然消失，按钮不再渲染）。
            if (item.imageRetryPrompt) {
              const retryPrompt = item.imageRetryPrompt;
              const retryN = item.imageRetryN ?? 1;
              const retrySize = item.imageRetrySize ?? null;
              return (
                <div
                  key={i}
                  data-item-idx={i}
                  style={wrapperBase("flex-start")}
                >
                  <div style={{ display: "flex", alignItems: "flex-end", gap: 6 }}>
                    <div style={{ ...bubbleStyle("assistant"), background: "var(--pet-tint-orange-bg)", color: "var(--pet-tint-orange-fg)" }}>
                      {item.content}
                    </div>
                    <button
                      type="button"
                      onClick={() => runImageGenerate(retryPrompt, i, retryN, retrySize)}
                      title={`用同一 prompt 重试：${retryPrompt}${retryN > 1 ? ` (-n ${retryN})` : ""}${retrySize ? ` (-s ${retrySize})` : ""}`}
                      aria-label="retry image generation"
                      style={{
                        alignSelf: "flex-end",
                        padding: "4px 10px",
                        fontSize: 12,
                        lineHeight: 1.2,
                        border: "1px solid var(--pet-color-border)",
                        borderRadius: 6,
                        background: "var(--pet-color-card)",
                        color: "var(--pet-color-accent)",
                        cursor: "pointer",
                        whiteSpace: "nowrap",
                        flexShrink: 0,
                      }}
                    >
                      🔄 重试
                    </button>
                  </div>
                </div>
              );
            }
            const markKey = `${sessionId}::${i}`;
            return (
              <CopyableMessage
                key={i}
                role="assistant"
                content={item.content}
                itemIdx={i}
                copied={copiedIdx === i}
                onCopy={handleCopy}
                wrapperStyle={wrapperBase("flex-start")}
                images={item.images}
                highlightKeyword={hitKeyword}
                reaction={reactionsByIdx[i] ?? null}
                onReact={handleReact}
                taskRefMap={chatTaskMap}
                onRefDoubleClick={onRequestFocusTask}
                marked={markedMessages.has(markKey)}
                onToggleMark={() => toggleMessageMark(markKey)}
                subdued={item.systemNote}
                onFindSimilar={handleFindSimilarInSession}
              />
            );
          }
          if (item.type === "tool") {
            return (
              <div key={i} data-item-idx={i} style={{ marginBottom: "12px", maxWidth: "85%" }}>
                {item.toolCalls?.map((tc, j) => {
                  // 自然语言派单：propose_task 的结果走确认卡，而非普通工具块。
                  // 解析失败 / 工具返回 error → 回退普通展示，保留排查能见性。
                  if (tc.name === "propose_task" && tc.result) {
                    const proposal = parseTaskProposal(tc.result);
                    if (proposal) {
                      return <TaskProposalCard key={j} proposal={proposal} />;
                    }
                  }
                  return <ToolCallBlock key={j} name={tc.name} arguments={tc.arguments} result={tc.result} />;
                })}
              </div>
            );
          }
          if (item.type === "error") {
            return (
              <div key={i} data-item-idx={i} style={{ marginBottom: "12px", display: "flex", justifyContent: "flex-start" }}>
                <div style={{ ...bubbleStyle("assistant"), background: "var(--pet-tint-orange-bg)", color: "var(--pet-tint-orange-fg)" }}>
                  {item.content}
                </div>
              </div>
            );
          }
          return null;
        })}

        {/* Live tool calls */}
        {currentToolCalls.length > 0 && (
          <div style={{ marginBottom: "12px", maxWidth: "85%" }}>
            {currentToolCalls.map((tc, j) => {
              if (tc.name === "propose_task" && tc.result) {
                const proposal = parseTaskProposal(tc.result);
                if (proposal) {
                  return <TaskProposalCard key={j} proposal={proposal} />;
                }
              }
              return (
                <ToolCallBlock
                  key={j}
                  name={tc.name}
                  arguments={tc.arguments}
                  result={tc.result}
                  isRunning={tc.isRunning}
                />
              );
            })}
          </div>
        )}

        {/* Streaming response */}
        {currentResponse && (
          <div style={{ marginBottom: "12px", display: "flex", justifyContent: "flex-start" }}>
            <div style={bubbleStyle("assistant")}>
              {currentResponse}
              <span style={{ animation: "blink 1s infinite" }}>▌</span>
            </div>
          </div>
        )}
      </div>
      {/* R103: 浮动 ↑ 跳到顶按钮。仅在 scrollTop > 200 时显，accent 圆形按钮
          锚定外层 relative 容器右下，不被卷入滚动。
          ↓ 跳到最新 加入后，两按钮垂直堆叠：↑ 在上 (bottom: 60)，↓ 在下
          (bottom: 16)，"箭头方向 = 滚动方向" 让用户不必读字也能找对。 */}
      {scrolledFromTop && (
        <button
          type="button"
          onClick={() =>
            scrollRef.current?.scrollTo({ top: 0, behavior: "smooth" })
          }
          title="回到会话开头"
          aria-label="scroll to top"
          style={{
            position: "absolute",
            right: 16,
            bottom: 60,
            width: 36,
            height: 36,
            borderRadius: "50%",
            border: "none",
            background: "var(--pet-color-accent)",
            color: "#fff",
            fontSize: 18,
            cursor: "pointer",
            boxShadow: "var(--pet-shadow-md)",
            opacity: 0.92,
          }}
        >
          ↑
        </button>
      )}
      {scrolledFromBottom && (
        <button
          type="button"
          onClick={() =>
            scrollRef.current?.scrollTo({
              top: scrollRef.current.scrollHeight,
              behavior: "smooth",
            })
          }
          title="跳到最新消息"
          aria-label="scroll to bottom"
          style={{
            position: "absolute",
            right: 16,
            bottom: 16,
            width: 36,
            height: 36,
            borderRadius: "50%",
            border: "none",
            background: "var(--pet-color-accent)",
            color: "#fff",
            fontSize: 18,
            cursor: "pointer",
            boxShadow: "var(--pet-shadow-md)",
            opacity: 0.92,
          }}
        >
          ↓
        </button>
      )}
      {/* 切 session 后浮 5s toast 提示前一 session 有未发草稿。点击切回。
          backdrop click / 5s 超时都自动关。固定 top-center 给会话切换路径
          稳定的"最近动作反馈"位置。 */}
      {draftReminder && (
        <div
          onClick={async () => {
            // 切回 toast 中记录的 session，loadSession 路径会自动把 draft
            // 填入 textarea
            const target = draftReminder.sessionId;
            setDraftReminder(null);
            if (draftReminderTimerRef.current !== null) {
              window.clearTimeout(draftReminderTimerRef.current);
              draftReminderTimerRef.current = null;
            }
            await loadSession(target);
          }}
          style={{
            position: "absolute",
            top: 12,
            left: "50%",
            transform: "translateX(-50%)",
            background: "var(--pet-color-card)",
            border: "1px solid var(--pet-color-accent)",
            color: "var(--pet-color-fg)",
            fontSize: 12,
            padding: "8px 14px",
            borderRadius: 8,
            boxShadow: "0 4px 12px rgba(0, 0, 0, 0.2)",
            zIndex: 50,
            cursor: "pointer",
            display: "flex",
            alignItems: "center",
            gap: 8,
            maxWidth: "85%",
          }}
          title="点击切回原会话继续写"
        >
          <span>📝</span>
          <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            <span style={{ color: "var(--pet-color-muted)" }}>「{draftReminder.title}」</span>{" "}
            <span style={{ color: "var(--pet-color-accent)", fontWeight: 600 }}>
              有 {draftReminder.charCount} 字未发草稿
            </span>
            <span style={{ color: "var(--pet-color-muted)" }}> · 点此切回</span>
          </span>
        </div>
      )}
      </div>

      {/* "全部标记消息"modal：fixed overlay 居中，列表内每条点击跳源
          session + 滚到该 itemIdx。backdrop click / Esc 关闭。 */}
      {/* 🛠 自定义模板管理 modal：列全部 customChatTemplates，每条
          可 rename（window.prompt）/ delete。clip 10 内不需要分页。 */}
      {manageTemplatesOpen && (
        <div
          onClick={() => setManageTemplatesOpen(false)}
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(15, 23, 42, 0.55)",
            zIndex: 200,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: 24,
          }}
        >
          <div
            onClick={(e) => e.stopPropagation()}
            style={{
              width: "100%",
              maxWidth: 520,
              maxHeight: "70vh",
              background: "var(--pet-color-card)",
              borderRadius: 10,
              boxShadow: "0 20px 60px rgba(0, 0, 0, 0.35)",
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
            }}
          >
            <div
              style={{
                padding: "10px 14px",
                borderBottom: "1px solid var(--pet-color-border)",
                display: "flex",
                alignItems: "center",
                gap: 8,
              }}
            >
              <span style={{ fontSize: 13, fontWeight: 600, color: "var(--pet-color-fg)" }}>
                🛠 管理自定义模板（{customChatTemplates.length}）
              </span>
              <span style={{ flex: 1 }} />
              <button
                onClick={() => setManageTemplatesOpen(false)}
                style={{
                  border: "none",
                  background: "transparent",
                  color: "var(--pet-color-muted)",
                  fontSize: 16,
                  cursor: "pointer",
                }}
                aria-label="close manage templates"
              >
                ✕
              </button>
            </div>
            <div style={{ overflowY: "auto", flex: 1 }}>
              {customChatTemplates.length === 0 ? (
                <div
                  style={{
                    padding: "24px 14px",
                    textAlign: "center",
                    color: "var(--pet-color-muted)",
                    fontSize: 12,
                  }}
                >
                  （还没保存自定义模板 — 在 input 写一段后点 💾 保存）
                </div>
              ) : (
                customChatTemplates.map((tpl, i) => (
                  <div
                    key={`${tpl.label}-${i}`}
                    style={{
                      padding: "10px 14px",
                      borderBottom: "1px solid var(--pet-color-border)",
                      display: "flex",
                      flexDirection: "column",
                      gap: 4,
                    }}
                  >
                    <div
                      style={{
                        display: "flex",
                        gap: 6,
                        alignItems: "center",
                      }}
                    >
                      <span
                        style={{
                          fontSize: 12,
                          fontWeight: 600,
                          color: "var(--pet-color-fg)",
                          flex: 1,
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                      >
                        {tpl.label}
                      </span>
                      <button
                        type="button"
                        onClick={() => {
                          const nextLabel = window.prompt("新 label：", tpl.label);
                          if (nextLabel === null) return;
                          const t = nextLabel.trim();
                          if (!t || t === tpl.label) return;
                          // 重命名：替换该条；同名碰撞按 saveCustomTemplate 同
                          // 模式去重（保留新条 + 删旧条）
                          const filtered = customChatTemplates.filter(
                            (x, j) => j !== i && x.label !== t,
                          );
                          const next = [...filtered, { label: t, text: tpl.text }];
                          persistCustomTemplates(next);
                        }}
                        title="重命名 label"
                        style={{
                          padding: "2px 8px",
                          fontSize: 11,
                          border: "1px solid var(--pet-color-border)",
                          borderRadius: 4,
                          background: "var(--pet-color-card)",
                          color: "var(--pet-color-muted)",
                          cursor: "pointer",
                          flexShrink: 0,
                        }}
                      >
                        ✏️
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          persistCustomTemplates(
                            customChatTemplates.filter((_, j) => j !== i),
                          );
                        }}
                        title="删除此模板（不可恢复）"
                        style={{
                          padding: "2px 8px",
                          fontSize: 11,
                          border: "1px solid var(--pet-color-border)",
                          borderRadius: 4,
                          background: "var(--pet-color-card)",
                          color: "var(--pet-color-muted)",
                          cursor: "pointer",
                          flexShrink: 0,
                        }}
                      >
                        🗑
                      </button>
                    </div>
                    <div
                      style={{
                        fontSize: 11,
                        color: "var(--pet-color-muted)",
                        whiteSpace: "pre-wrap",
                        wordBreak: "break-word",
                        display: "-webkit-box",
                        WebkitLineClamp: 3,
                        WebkitBoxOrient: "vertical",
                        overflow: "hidden",
                      }}
                    >
                      {tpl.text}
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}
      {marksModalOpen && (
        <div
          onClick={() => setMarksModalOpen(false)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              setMarksModalOpen(false);
            }
          }}
          style={{
            position: "fixed",
            inset: 0,
            background: "color-mix(in srgb, var(--pet-color-fg) 50%, transparent)",
            zIndex: 200,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: 24,
            animation: "pet-modal-fade-in 140ms ease-out",
          }}
        >
          <div
            onClick={(e) => e.stopPropagation()}
            style={{
              width: "100%",
              maxWidth: 640,
              maxHeight: "75vh",
              background: "var(--pet-color-card)",
              borderRadius: 12,
              boxShadow: "var(--pet-shadow-lg)",
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
              animation: "pet-modal-pop 180ms ease-out",
            }}
          >
            <div
              style={{
                padding: "10px 14px",
                borderBottom: "1px solid var(--pet-color-border)",
                display: "flex",
                alignItems: "center",
                gap: 8,
              }}
            >
              <span style={{ fontSize: 13, fontWeight: 600, color: "var(--pet-color-fg)", flexShrink: 0 }}>
                📌 全部标记消息 ({markedMessages.size})
              </span>
              <input
                type="text"
                value={marksModalQuery}
                onChange={(e) => setMarksModalQuery(e.target.value)}
                placeholder="按 session 标题 / 内容子串过滤…"
                style={{
                  flex: 1,
                  padding: "4px 8px",
                  fontSize: 12,
                  border: "1px solid var(--pet-color-border)",
                  borderRadius: 4,
                  background: "var(--pet-color-bg)",
                  color: "var(--pet-color-fg)",
                  outline: "none",
                }}
              />
              {/* 📋 复制（当前过滤的）全部标记为 markdown：把列表中的 entries
                  拼成 H1 + 各条 H2 段，复制到剪贴板。与 search 输入框同行
                  让"先搜后复制"流程顺。无 entries / loading 时 disabled。 */}
              {marksModalEntries !== null && marksModalEntries.length > 0 && (
                <button
                  onClick={async () => {
                    const q = marksModalQuery.trim().toLowerCase();
                    const list =
                      q.length === 0
                        ? marksModalEntries
                        : marksModalEntries.filter(
                            (e) =>
                              e.sessionTitle.toLowerCase().includes(q) ||
                              e.content.toLowerCase().includes(q),
                          );
                    if (list.length === 0) return;
                    const ts = new Date().toLocaleString();
                    const lines: string[] = [
                      `# 📌 标记消息导出（${ts}）`,
                      "",
                      `共 ${list.length} 条${q.length > 0 ? `（过滤："${marksModalQuery.trim()}"）` : ""}`,
                      "",
                    ];
                    for (const e of list) {
                      const roleIcon = e.role === "user" ? "🧑" : "🐾";
                      const markedStr =
                        e.markedAt > 0
                          ? ` · 标记于 ${new Date(e.markedAt).toLocaleString()}`
                          : "";
                      lines.push(
                        `## ${roleIcon} ${e.sessionTitle} · #${e.itemIdx + 1}${markedStr}`,
                        "",
                        e.content || "（空）",
                        "",
                      );
                    }
                    try {
                      await navigator.clipboard.writeText(lines.join("\n"));
                      setMarksModalCopied(true);
                      window.setTimeout(() => setMarksModalCopied(false), 1500);
                    } catch (err) {
                      console.error("clipboard write failed:", err);
                    }
                  }}
                  title={
                    marksModalQuery.trim().length > 0
                      ? "把当前过滤后的标记列表拼成 markdown 复制到剪贴板"
                      : "把全部标记列表拼成 markdown 复制到剪贴板"
                  }
                  style={{
                    padding: "4px 10px",
                    fontSize: 11,
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 4,
                    background: "var(--pet-color-card)",
                    color: marksModalCopied ? "#16a34a" : "var(--pet-color-fg)",
                    cursor: "pointer",
                    flexShrink: 0,
                    fontWeight: marksModalCopied ? 600 : 400,
                  }}
                >
                  {marksModalCopied ? "✓ 已复制" : "📋 复制"}
                </button>
              )}
              <button
                onClick={() => setMarksModalOpen(false)}
                style={{
                  border: "none",
                  background: "transparent",
                  color: "var(--pet-color-muted)",
                  fontSize: 16,
                  cursor: "pointer",
                }}
                aria-label="close marks modal"
              >
                ✕
              </button>
            </div>
            <div style={{ overflowY: "auto", flex: 1 }}>
              {marksModalEntries === null ? (
                <div
                  style={{
                    padding: "24px 14px",
                    textAlign: "center",
                    color: "var(--pet-color-muted)",
                    fontSize: 12,
                  }}
                >
                  加载中…
                </div>
              ) : marksModalEntries.length === 0 ? (
                <div
                  style={{
                    padding: "24px 14px",
                    textAlign: "center",
                    color: "var(--pet-color-muted)",
                    fontSize: 12,
                  }}
                >
                  （没有可显示的标记 — session 可能已删 / 消息可能已被改）
                </div>
              ) : (() => {
                const q = marksModalQuery.trim().toLowerCase();
                const filtered =
                  q.length === 0
                    ? marksModalEntries
                    : marksModalEntries.filter(
                        (e) =>
                          e.sessionTitle.toLowerCase().includes(q) ||
                          e.content.toLowerCase().includes(q),
                      );
                if (filtered.length === 0) {
                  return (
                    <div
                      style={{
                        padding: "24px 14px",
                        textAlign: "center",
                        color: "var(--pet-color-muted)",
                        fontSize: 12,
                      }}
                    >
                      没有匹配 "{marksModalQuery.trim()}" 的标记
                    </div>
                  );
                }
                return filtered.map((e) => (
                  <div
                    key={`${e.sessionId}::${e.itemIdx}`}
                    onClick={async () => {
                      setMarksModalOpen(false);
                      if (e.sessionId !== sessionId) {
                        await loadSession(e.sessionId);
                      }
                      // 顺手设 pendingScroll → useEffect 把 item 滚中 +
                      // 1.5s 黄色高亮（与跨会话搜索同款 jump path）。
                      setPendingScroll(e.itemIdx);
                    }}
                    style={{
                      padding: "10px 14px",
                      borderBottom: "1px solid var(--pet-color-border)",
                      cursor: "pointer",
                      display: "flex",
                      flexDirection: "column",
                      gap: 4,
                    }}
                    title={`跳到「${e.sessionTitle}」#${e.itemIdx + 1}\n\n${e.content || "（空）"}`}
                  >
                    <div
                      style={{
                        fontSize: 10,
                        color: "var(--pet-color-muted)",
                        display: "flex",
                        gap: 6,
                      }}
                    >
                      <span>{e.role === "user" ? "🧑 user" : "🐾 assistant"}</span>
                      <span>·</span>
                      <span
                        style={{
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                          maxWidth: 280,
                        }}
                      >
                        {e.sessionTitle}
                      </span>
                      <span>·</span>
                      <span>#{e.itemIdx + 1}</span>
                      {/* 标记时间：ts > 0 时显相对时间（< 60s "刚刚" / < 1h
                          "X 分钟前" / < 1d "X 小时前" / else "X 天前"）。
                          ts === 0 是老格式 dangling 跳过。 */}
                      {e.markedAt > 0 && (() => {
                        const age = Date.now() - e.markedAt;
                        const rel =
                          age < 60_000 ? "刚刚" : formatRelativeAgeBuckets(age);
                        return (
                          <>
                            <span>·</span>
                            <span
                              title={`标记于 ${new Date(e.markedAt).toLocaleString()}`}
                            >
                              📌 {rel}
                            </span>
                          </>
                        );
                      })()}
                      <span style={{ flex: 1 }} />
                      {/* 🗑 取消标记：直接从 markedMessages map 移除 + 从
                          marksModalEntries 立即过滤掉，省去"先跳源再去找
                          📌 取消"的两步。stopPropagation 阻止行级 jump
                          handler。 */}
                      <button
                        type="button"
                        onClick={(ev) => {
                          ev.stopPropagation();
                          const k = `${e.sessionId}::${e.itemIdx}`;
                          toggleMessageMark(k);
                          setMarksModalEntries((prev) =>
                            prev
                              ? prev.filter(
                                  (x) =>
                                    !(
                                      x.sessionId === e.sessionId &&
                                      x.itemIdx === e.itemIdx
                                    ),
                                )
                              : prev,
                          );
                        }}
                        title="取消此消息的 📌 标记（从 localStorage 收藏集移除）"
                        aria-label="unmark message"
                        style={{
                          padding: "1px 6px",
                          fontSize: 10,
                          border: "1px solid var(--pet-color-border)",
                          borderRadius: 3,
                          background: "var(--pet-color-card)",
                          color: "var(--pet-color-muted)",
                          cursor: "pointer",
                          flexShrink: 0,
                          fontFamily: "inherit",
                        }}
                      >
                        🗑
                      </button>
                    </div>
                    <div
                      style={{
                        fontSize: 12,
                        color: "var(--pet-color-fg)",
                        whiteSpace: "pre-wrap",
                        wordBreak: "break-word",
                        // 长内容截断到 200 字，hover 看完整 via title
                        display: "-webkit-box",
                        WebkitLineClamp: 4,
                        WebkitBoxOrient: "vertical",
                        overflow: "hidden",
                      }}
                    >
                      {e.content || "（空）"}
                    </div>
                  </div>
                ));
              })()}
            </div>
          </div>
        </div>
      )}

      {/* 上下文 token 警示 banner：与 ChatMini 同源信号（get_active_session_
          context_stats 60s 轮）+ 同 4000 阈值。> 阈值时贴顶 input bar 浮出，让
          owner 在敲下一条之前感知 prompt 在膨胀。点 /reset 按钮调
          handleResetLlmContext（与 slash 命令同 path）—— PanelChat /reset 保
          留可见 items 仅清 LLM 上下文，比 ChatMini reset 软（无需 armed 二次
          确认）。流式中按钮 disabled 防 race。 */}
      {sessionTokens > 4000 && (
        <div
          style={{
            padding: "5px 16px",
            fontSize: 11,
            color: "var(--pet-tint-yellow-fg)",
            background: "var(--pet-tint-yellow-bg)",
            borderTop: "1px solid var(--pet-color-border)",
            display: "flex",
            alignItems: "center",
            gap: 8,
          }}
        >
          <span style={{ flex: 1 }}>
            💭 上下文 ~{sessionTokens} tok（已超 4000，建议
            <strong> /reset</strong> 让宠物注意力回到当前话题）
          </span>
          <button
            type="button"
            onClick={() => void handleResetLlmContext()}
            disabled={isLoading}
            style={{
              fontSize: 10,
              fontWeight: 600,
              padding: "2px 8px",
              borderRadius: 6,
              border:
                "1px solid color-mix(in srgb, var(--pet-tint-yellow-fg) 50%, transparent)",
              background: "var(--pet-color-card)",
              color: "var(--pet-tint-yellow-fg)",
              cursor: isLoading ? "default" : "pointer",
              whiteSpace: "nowrap",
              opacity: isLoading ? 0.5 : 1,
            }}
            title={
              isLoading
                ? "流式回复中，先等完成或 Esc 取消再 /reset"
                : "清掉本 session 的 LLM 上下文（保留可见 mini chat 历史 + system 人设），等价于敲 /reset"
            }
          >
            /reset
          </button>
        </div>
      )}

      {/* Input bar */}
      <form
        onSubmit={handleSubmit}
        style={{
          display: "flex",
          gap: "8px",
          padding: "12px 16px",
          borderTop: "1px solid var(--pet-color-border)",
          background: "var(--pet-color-card)",
          position: "relative", // SlashCommandMenu 用 absolute / bottom: 100% 锚到这里
        }}
      >
        {slashMenuVisible && (
          <SlashCommandMenu
            commands={filteredCommands}
            selectedIdx={selectedSlashIdx}
            onSelect={handleSelectSlashCommand}
          />
        )}
        {/* 输入历史浏览态 idx / total hint：仅 historyCursor 非空（owner
            按 ↑ 进入 history browse mode）时显。让 owner 看到 "我在历史
            第几条 / 共 N 条" 不迷路。位置同 SlashCommandMenu 锚位（form
            relative + bottom: 100% 偏上），但更小更 muted。 */}
        {historyCursor !== null && messageHistory.length > 0 && (
          <div
            style={{
              position: "absolute",
              bottom: "calc(100% + 4px)",
              right: 16,
              fontSize: 10,
              fontFamily: "'SF Mono', 'Menlo', monospace",
              color: "var(--pet-color-muted)",
              background: "var(--pet-color-card)",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 4,
              padding: "2px 8px",
              boxShadow: "var(--pet-shadow-sm)",
              opacity: 0.85,
              pointerEvents: "none",
              userSelect: "none",
              zIndex: 5,
            }}
            aria-hidden
          >
            ↕ 历史 {historyCursor + 1} / {messageHistory.length} · Esc 退出
          </div>
        )}
        {/* `@` 提及浮窗：与 SlashCommandMenu 同锚位（form 的 relative 容器 +
            bottom:100%）。无 task 命中 → 显空态提示 + 用户敲字 / Esc 自然退。 */}
        {mentionMenuVisible && mentionContext && (
          <div style={mentionMenuContainerStyle}>
            {mentionFilteredTasks.length === 0 ? (
              <div
                style={{
                  padding: "10px 12px",
                  fontSize: "12px",
                  color: "var(--pet-color-muted)",
                }}
              >
                {Object.keys(chatTaskMap).length === 0
                  ? "（没有任务可引用）"
                  : `没有匹配「${mentionContext.query}」的任务；Esc 退出 / 继续敲改 query`}
              </div>
            ) : (
              <>
                {mentionFilteredTasks.map((t, i) => {
                  const selected = i === mentionSelectedIdx;
                  return (
                    <div
                      key={t.title}
                      onMouseDown={(e) => {
                        e.preventDefault();
                        pickMention(t.title, mentionContext);
                      }}
                      style={{
                        padding: "6px 12px",
                        cursor: "pointer",
                        background: selected
                          ? "var(--pet-tint-blue-bg)"
                          : "transparent",
                        borderLeft: selected
                          ? "2px solid var(--pet-color-accent)"
                          : "2px solid transparent",
                        display: "flex",
                        alignItems: "baseline",
                        gap: "10px",
                        fontSize: "13px",
                      }}
                    >
                      <span
                        style={{
                          color: selected
                            ? "var(--pet-tint-blue-fg)"
                            : "var(--pet-color-fg)",
                          fontWeight: 500,
                          flex: 1,
                          whiteSpace: "nowrap",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                        }}
                      >
                        {t.title}
                      </span>
                      <span
                        style={{
                          fontSize: "10px",
                          color: "var(--pet-color-muted)",
                          fontFamily: "'SF Mono', 'Menlo', monospace",
                        }}
                      >
                        {t.status}
                      </span>
                    </div>
                  );
                })}
                {/* 键盘提示 footer：与 SlashCommandMenu 同 hint 节奏。让 ↑↓
                    / Enter / Tab / Esc 的隐式 keymap 可见。 */}
                <div
                  style={{
                    padding: "5px 12px",
                    borderTop: "1px solid var(--pet-color-border)",
                    background: "var(--pet-color-bg)",
                    fontSize: 10,
                    color: "var(--pet-color-muted)",
                    display: "flex",
                    gap: 10,
                    fontFamily: "'SF Mono', Menlo, monospace",
                    letterSpacing: 0.2,
                  }}
                >
                  <span>↑↓ 选</span>
                  <span>Enter / Tab 引用</span>
                  <span>Esc 取消</span>
                </div>
              </>
            )}
          </div>
        )}
        {imagePromptMenuVisible && (
          <ImagePromptHistoryMenu
            prompts={imagePromptHistory}
            selectedIdx={selectedImagePromptIdx}
            onSelect={(prompt) => setInput(`/image ${prompt}`)}
          />
        )}
        {/* R134: 输入框字数 counter。非空时浮 input bar 顶左（与 R132 历史
            hint 错开 — 顶右）。pointerEvents none 让它不挡按钮 / textarea。 */}
        {input.length > 0 && (
          <div
            style={{
              position: "absolute",
              top: -22,
              left: 16,
              fontSize: 10,
              color: "var(--pet-color-muted)",
              pointerEvents: "none",
              fontFamily: "'SF Mono', 'Menlo', monospace",
              whiteSpace: "nowrap",
            }}
            title="当前消息字符数（Unicode code units 计；含换行 / 空白）"
          >
            {input.length} 字
          </div>
        )}
        {/* Token 估算 chip：input 非空时浮在 input bar 顶部左侧（与右侧
            history 提示错开互不挡）。粗略估算（CJK ~1tok/字、其它 ~4字/tok）；
            tooltip 解释口径。input.length === 0 → 不渲染避免空 chip。
            estimateInputTokens 是 pure / O(n)，input 通常 < 几千字，每次
            re-render 重算无虞，不必 useMemo。 */}
        {input.length > 0 && (() => {
          const tokens = estimateInputTokens(input);
          return (
            <div
              style={{
                position: "absolute",
                top: -22,
                left: 16,
                fontSize: 10,
                background: "var(--pet-color-card)",
                border: "1px solid var(--pet-color-border)",
                borderRadius: 4,
                padding: "2px 8px",
                color: "var(--pet-color-muted)",
                pointerEvents: "auto",
                whiteSpace: "nowrap",
                fontFamily: "'SF Mono', 'Menlo', monospace",
              }}
              title={`粗略估算输入的 token 数：CJK 1 token/字 + 其它 ~4 字/token。准确值因模型而异（GPT-4o BPE / Claude / Qwen 都不同），本 chip 仅供"我打了多长" 感知。\n\n当前 ${input.length} 字 → ~${tokens} tok`}
            >
              ~{tokens} tok
            </div>
          );
        })()}
        {/* R132: 历史模式视觉提示。historyCursor 非 null 时浮在 input bar
            顶部右侧；onChange 改写 / Esc / 发送 / ↓ 越过末尾 自然 cursor=null
            → hint 消失。pointerEvents none 让它不挡按钮。 */}
        {historyCursor !== null && (
          <div
            style={{
              position: "absolute",
              top: -22,
              right: 16,
              fontSize: 10,
              background: "var(--pet-color-card)",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 4,
              padding: "2px 8px",
              color: "var(--pet-color-muted)",
              pointerEvents: "none",
              whiteSpace: "nowrap",
              fontFamily: "'SF Mono', 'Menlo', monospace",
            }}
            title={`当前浏览历史第 ${historyCursor + 1} / ${messageHistory.length} 条；↑ 往更早，↓ 往更新，Esc 退出`}
          >
            ↑ 历史 {historyCursor + 1} / {messageHistory.length}
          </div>
        )}
        {/* 多模态：pendingImages 折叠成单 chip "📎 N 附件待发"；hover / focus
            展开浮窗显完整缩略图条 + ✕ 移除按钮。原 always-expanded 视觉占空
            间过多；折叠后 input bar 视觉干净，长 prompt 区域不被挤压。 */}
        {pendingImages.length > 0 && (
          <PendingAttachmentsChip
            images={pendingImages}
            onOpen={(src) => setComposeLightboxSrc(src)}
            onRemove={(idx) =>
              setPendingImages((prev) => prev.filter((_, j) => j !== idx))
            }
            onClearAll={() => setPendingImages([])}
          />
        )}
        {/* 📎 文件选择：点按钮 → 触发 hidden input.click()，OS 弹系统对话框；
            选中的 image/* 走 ingestImageBlobs 与 paste / drop 同一管道。multiple
            支持一次选多图；accept 限定让对话框默认只显图，但 OS 仍会让用户
            切到"全部文件"——所以 onChange 里再过滤一次 type.startsWith。
            清空 value 让选完同一份文件后能再选一次（input file 默认只触发新
            值的 change，不重置就再点 📎 选同名文件不会 fire）。 */}
        <input
          ref={composeFileInputRef}
          type="file"
          accept="image/*"
          multiple
          style={{ display: "none" }}
          onChange={(e) => {
            const files = e.target.files;
            if (!files || files.length === 0) return;
            const blobs: Blob[] = [];
            for (let i = 0; i < files.length; i++) {
              const f = files[i];
              if (f.type.startsWith("image/")) blobs.push(f);
            }
            // reset 让同一张图能再次选；onChange 之后再走 ingest
            e.target.value = "";
            if (blobs.length === 0) {
              pushLocalAssistantNote(
                "⚠ 没选到图片（仅支持 image/* 格式）。",
              );
              return;
            }
            ingestImageBlobs(blobs);
          }}
        />
        {/* 📋 prompt 模板下拉：选中 prefill input 字符串 + focus 让用户立
            刻编辑占位。value="" sentinel + reset 让下次能再选同条。仅
            input 当前为空时浮（已敲字时显得多余 + 易误触清掉用户输入）。 */}
        {input.length === 0 && (
          <select
            value=""
            onChange={(e) => {
              // 用 "B:i" / "C:i" 区分内置 vs 自定义 entry
              const v = e.target.value;
              if (!v) return;
              const [src, idxStr] = v.split(":");
              const idx = parseInt(idxStr, 10);
              if (Number.isNaN(idx)) return;
              const tpl =
                src === "B"
                  ? CHAT_PROMPT_TEMPLATES[idx]
                  : customChatTemplates[idx];
              if (!tpl) return;
              setInput(tpl.text);
              e.currentTarget.value = "";
              window.setTimeout(() => {
                composeTextareaRef.current?.focus();
              }, 0);
            }}
            disabled={isLoading}
            title="选一个常见 prompt 模板预填到输入框（含内置 + 用户自定义）"
            style={{
              padding: "10px 6px",
              borderRadius: "10px",
              border: "1px solid var(--pet-color-border)",
              background: "var(--pet-color-card)",
              color: "var(--pet-color-muted)",
              cursor: isLoading ? "default" : "pointer",
              fontSize: 12,
              flexShrink: 0,
              fontFamily: "inherit",
            }}
          >
            <option value="">📋 模板…</option>
            <optgroup label="内置">
              {CHAT_PROMPT_TEMPLATES.map((tpl, i) => (
                <option key={tpl.label} value={`B:${i}`}>
                  {tpl.label}
                </option>
              ))}
            </optgroup>
            {customChatTemplates.length > 0 && (
              <optgroup label="自定义">
                {customChatTemplates.map((tpl, i) => (
                  <option key={`custom-${tpl.label}-${i}`} value={`C:${i}`}>
                    {tpl.label}
                  </option>
                ))}
              </optgroup>
            )}
          </select>
        )}
        {/* "🛠 管理自定义模板" 按钮：仅在 input 空 且有 custom 模板时浮。
            打开 modal 让用户重命名 / 删除条目。不与 📋 dropdown 合并 —
            select option 不能内嵌交互按钮。 */}
        {input.length === 0 && customChatTemplates.length > 0 && (
          <button
            type="button"
            onClick={() => setManageTemplatesOpen(true)}
            disabled={isLoading}
            title={`管理自定义模板（${customChatTemplates.length} 条）：重命名 / 删除`}
            aria-label="manage custom templates"
            style={{
              padding: "10px 8px",
              borderRadius: "10px",
              border: "1px solid var(--pet-color-border)",
              background: "var(--pet-color-card)",
              color: "var(--pet-color-muted)",
              cursor: isLoading ? "default" : "pointer",
              fontSize: 12,
              flexShrink: 0,
            }}
          >
            🛠
          </button>
        )}
        {/* 💾 保存当前 input 为自定义模板：仅 input 非空时浮。点击 prompt
            用户输 label（default 首 12 字 trim 作为 hint），保存到
            localStorage 列表。cap 10 FIFO；同 label 替换。 */}
        {input.trim().length > 0 && (
          <button
            type="button"
            onClick={() => {
              const hint = input.trim().slice(0, 12);
              const label = window.prompt(
                "给当前输入起个 label（保存到自定义模板，可在 📋 下拉里复用）：",
                hint,
              );
              if (label === null) return;
              const trimmed = label.trim();
              if (trimmed.length === 0) return;
              saveCustomTemplate(trimmed, input);
            }}
            disabled={isLoading}
            title="把当前 input 内容保存为自定义模板（localStorage，跨重启），出现在 📋 下拉的'自定义'分组里。cap 10 条 FIFO；同 label 替换。"
            aria-label="save as template"
            style={{
              padding: "10px 12px",
              borderRadius: "10px",
              border: "1px solid var(--pet-color-border)",
              background: "var(--pet-color-card)",
              color: "var(--pet-color-muted)",
              cursor: isLoading ? "default" : "pointer",
              fontSize: 14,
              lineHeight: 1,
              flexShrink: 0,
            }}
          >
            💾
          </button>
        )}
        {/* ↩️ 快速 follow-up：仅在 input 空 且 chat 已有交流（items 非空）
            时浮。短回应模板，覆盖"对话中接 assistant"路径。与 prompt
            模板共占 input 左侧 toolbar，select 与 file button 同列。 */}
        {input.length === 0 && items.length > 0 && (
          <select
            value=""
            onChange={(e) => {
              const idx = parseInt(e.target.value, 10);
              if (Number.isNaN(idx)) return;
              const tpl = CHAT_FOLLOWUP_TEMPLATES[idx];
              if (!tpl) return;
              setInput(tpl.text);
              e.currentTarget.value = "";
              window.setTimeout(() => {
                composeTextareaRef.current?.focus();
              }, 0);
            }}
            disabled={isLoading}
            title="选一个常见短回应预填到输入框"
            style={{
              padding: "10px 6px",
              borderRadius: "10px",
              border: "1px solid var(--pet-color-border)",
              background: "var(--pet-color-card)",
              color: "var(--pet-color-muted)",
              cursor: isLoading ? "default" : "pointer",
              fontSize: 12,
              flexShrink: 0,
              fontFamily: "inherit",
            }}
          >
            <option value="">↩️ 回应…</option>
            {CHAT_FOLLOWUP_TEMPLATES.map((tpl, i) => (
              <option key={tpl.label} value={i}>
                {tpl.label}
              </option>
            ))}
          </select>
        )}
        <button
          type="button"
          onClick={() => composeFileInputRef.current?.click()}
          title="选择本地图片附带发送（与粘贴 / 拖入同管道，需多模态模型）"
          aria-label="upload image"
          disabled={isLoading}
          style={{
            padding: "10px 12px",
            borderRadius: "10px",
            border: "1px solid var(--pet-color-border)",
            background: "var(--pet-color-card)",
            color: "var(--pet-color-muted)",
            cursor: isLoading ? "default" : "pointer",
            fontSize: "16px",
            lineHeight: 1,
            flexShrink: 0,
          }}
        >
          📎
        </button>
        {/* R126: 单行 input → auto-grow textarea。rows 用 \n 计数 + 1（cap 5），
            soft-wrap 不影响（用户主动换行才 grow）。Enter 提交 / Shift+Enter
            换行的语义在 handleInputKeyDown 里。 */}
        <textarea
          ref={composeTextareaRef}
          value={input}
          onChange={(e) => {
            const v = e.target.value;
            setInput(v);
            // 追踪 `@` 触发态需要光标位置 —— onChange 是最及时的 hook
            // （比 onKeyUp 早；IME 多字符插入也覆盖到）。
            setComposeCursorPos(e.target.selectionStart ?? v.length);
            // R129: 用户改写 history 召回内容 → 自动退出历史模式让 free editing。
            // 再 ↑ 时从最新一条（历史末）重新进入；不会跳到 cursor 之前残留的位置。
            if (
              historyCursor !== null &&
              v !== messageHistory[historyCursor]
            ) {
              setHistoryCursor(null);
            }
          }}
          onSelect={(e) => {
            // 鼠标点击 / 键盘 ArrowLeft/Right 移动光标 → 重新评估 `@` 触发态。
            // onSelect 在选区或 caret 位置变化时 fire，覆盖纯 caret-move。
            setComposeCursorPos(
              (e.target as HTMLTextAreaElement).selectionStart ?? 0,
            );
          }}
          onPaste={(e) => {
            // 多模态：扫 clipboardData.items，把 image/* 的 blob 转 base64 data
            // URL 推到 pendingImages。preventDefault 阻止图片"路径文本"误粘到
            // textarea；同帧多图（截图 + 应用复制）按时序异步推入。
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
          onKeyDown={handleInputKeyDown}
          placeholder='输入消息（Enter 发送 / Shift+Enter 换行；可粘贴图片；"/" 触发命令面板；"@" / ⌘K 召唤 task ref；⌘B 切上一会话；⌘N 新建会话）'
          rows={Math.max(
            1,
            Math.min(5, (input.match(/\n/g)?.length ?? 0) + 1),
          )}
          style={{
            flex: 1,
            padding: "10px 14px",
            borderRadius: "10px",
            border: "1px solid var(--pet-color-border)",
            fontSize: "14px",
            outline: "none",
            color: "var(--pet-color-fg)",
            background: "var(--pet-color-card)",
            resize: "none",
            fontFamily: "inherit",
            lineHeight: 1.5,
            overflowY: "auto",
          }}
        />
        <button
          type="submit"
          className="pet-chat-send"
          disabled={isLoading}
          style={{
            padding: "10px 22px",
            borderRadius: "10px",
            border: "none",
            background: isLoading ? "#cbd5e1" : "var(--pet-color-accent)",
            color: "#fff",
            fontSize: "14px",
            fontWeight: 600,
            letterSpacing: 0.4,
            cursor: isLoading ? "default" : "pointer",
            // accent 色 30% alpha 拖一道柔光圈让发送按钮"立"在 input bar 上 ——
            // 高频交互按钮应该有最显眼的视觉优先级。
            boxShadow: isLoading
              ? "none"
              : "0 4px 14px color-mix(in srgb, var(--pet-color-accent) 35%, transparent)",
            flexShrink: 0,
          }}
        >
          {isLoading ? "..." : "发送"}
        </button>
      </form>
      <ImageLightbox
        src={composeLightboxSrc}
        onClose={() => setComposeLightboxSrc(null)}
      />
      {/* ⌘K task 引用选择器 modal：fixed overlay 居中，输入过滤 +
          键盘 nav。选中 → 把「title」插入 textarea 光标位 + 关 popup。
          backdrop click / Esc 关；空 query 列全部 task。 */}
      {taskPickerOpen && (() => {
        const q = taskPickerQuery.trim().toLowerCase();
        // char-order 子序列 fuzzy 匹配：query 每个字符按顺序在 title 里
        // 找下一处出现位置，找全 = 命中。"intDown" 能命中 "整理 Downloads"
        // 因为 i/n/t/D/o/w/n 都按序出现在小写 title 里。score 用"匹配段长度
        // + 首次匹配位置"组合，越紧凑越靠前 = 越优。空 query 直接列全集。
        const fuzzyMatch = (query: string, target: string): number | null => {
          if (query.length === 0) return 0;
          let qi = 0;
          let firstMatch = -1;
          let lastMatch = -1;
          for (let ti = 0; ti < target.length && qi < query.length; ti++) {
            if (target[ti] === query[qi]) {
              if (firstMatch < 0) firstMatch = ti;
              lastMatch = ti;
              qi += 1;
            }
          }
          if (qi !== query.length) return null;
          // 紧凑度（span 小）权重高于位置（first 小）；用 *100 让两个量级不冲突
          return (lastMatch - firstMatch + 1) * 100 + firstMatch;
        };
        const filtered =
          q.length === 0
            ? taskPickerTasks
            : (() => {
                const scored: Array<{ t: TaskRefView; score: number }> = [];
                for (const t of taskPickerTasks) {
                  const s = fuzzyMatch(q, t.title.toLowerCase());
                  if (s !== null) scored.push({ t, score: s });
                }
                scored.sort((a, b) => a.score - b.score);
                return scored.map((x) => x.t);
              })();
        const safeIdx = Math.max(
          0,
          Math.min(taskPickerSelectedIdx, filtered.length - 1),
        );
        const close = () => {
          setTaskPickerOpen(false);
          setTaskPickerQuery("");
          setTaskPickerSelectedIdx(0);
          // 关闭后把焦点还给 chat textarea，让用户继续敲消息
          window.setTimeout(() => {
            composeTextareaRef.current?.focus();
          }, 0);
        };
        const pick = (title: string) => {
          insertTaskRef(title);
          close();
        };
        return (
          <div
            onClick={close}
            style={{
              position: "fixed",
              inset: 0,
              background: "rgba(15, 23, 42, 0.55)",
              zIndex: 200,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              padding: 24,
            }}
          >
            <div
              onClick={(e) => e.stopPropagation()}
              style={{
                width: "100%",
                maxWidth: 480,
                maxHeight: "70vh",
                background: "var(--pet-color-card)",
                borderRadius: 10,
                boxShadow: "0 20px 60px rgba(0, 0, 0, 0.35)",
                display: "flex",
                flexDirection: "column",
                overflow: "hidden",
              }}
            >
              <div
                style={{
                  padding: "10px 14px",
                  borderBottom: "1px solid var(--pet-color-border)",
                  display: "flex",
                  gap: 8,
                  alignItems: "center",
                }}
              >
                <span style={{ fontSize: 13, fontWeight: 600, color: "var(--pet-color-fg)" }}>
                  📎 引用任务
                </span>
                <input
                  ref={taskPickerInputRef}
                  autoFocus
                  value={taskPickerQuery}
                  placeholder="按标题模糊搜任务…（按字符顺序匹配；↑↓ 选 / Enter 插入 / Esc 关闭）"
                  onChange={(e) => {
                    setTaskPickerQuery(e.target.value);
                    setTaskPickerSelectedIdx(0);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "ArrowDown") {
                      e.preventDefault();
                      setTaskPickerSelectedIdx((i) =>
                        Math.min(i + 1, filtered.length - 1),
                      );
                    } else if (e.key === "ArrowUp") {
                      e.preventDefault();
                      setTaskPickerSelectedIdx((i) => Math.max(0, i - 1));
                    } else if (e.key === "Enter") {
                      e.preventDefault();
                      const picked = filtered[safeIdx];
                      if (picked) pick(picked.title);
                    } else if (e.key === "Escape") {
                      e.preventDefault();
                      close();
                    }
                  }}
                  style={{
                    flex: 1,
                    padding: "6px 10px",
                    fontSize: 13,
                    border: "1px solid var(--pet-color-border)",
                    borderRadius: 4,
                    background: "var(--pet-color-bg)",
                    color: "var(--pet-color-fg)",
                    outline: "none",
                  }}
                />
              </div>
              <div style={{ overflowY: "auto", flex: 1 }}>
                {filtered.length === 0 ? (
                  <div
                    style={{
                      padding: "20px 14px",
                      textAlign: "center",
                      color: "var(--pet-color-muted)",
                      fontSize: 12,
                    }}
                  >
                    {taskPickerTasks.length === 0
                      ? "（没有任务可引用）"
                      : "没有匹配的任务"}
                  </div>
                ) : (
                  filtered.map((t, i) => {
                    const active = i === safeIdx;
                    return (
                      <div
                        key={t.title}
                        onClick={() => pick(t.title)}
                        onMouseEnter={() => setTaskPickerSelectedIdx(i)}
                        style={{
                          padding: "8px 14px",
                          cursor: "pointer",
                          background: active ? "var(--pet-color-bg)" : "transparent",
                          borderLeft: active
                            ? "3px solid var(--pet-color-accent)"
                            : "3px solid transparent",
                          display: "flex",
                          alignItems: "center",
                          gap: 8,
                          fontSize: 12,
                          color: "var(--pet-color-fg)",
                        }}
                      >
                        <span
                          style={{
                            fontSize: 10,
                            padding: "1px 6px",
                            borderRadius: 3,
                            background: "var(--pet-color-bg)",
                            color: "var(--pet-color-muted)",
                            flexShrink: 0,
                            fontFamily: "'SF Mono', 'Menlo', monospace",
                          }}
                        >
                          {t.status}
                        </span>
                        <span
                          style={{
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                          }}
                        >
                          {t.title}
                        </span>
                      </div>
                    );
                  })
                )}
              </div>
              <div
                style={{
                  padding: "6px 14px",
                  borderTop: "1px solid var(--pet-color-border)",
                  fontSize: 10,
                  color: "var(--pet-color-muted)",
                  fontFamily: "'SF Mono', 'Menlo', monospace",
                }}
              >
                {filtered.length} / {taskPickerTasks.length} · 插入格式：「任务标题」
              </div>
            </div>
          </div>
        );
      })()}
      {sessionTabCtxMenu && (() => {
        const m = sessionTabCtxMenu;
        const W = 180;
        const H = 200;
        const left = Math.max(8, Math.min(m.x, window.innerWidth - W - 8));
        const top = Math.max(8, Math.min(m.y, window.innerHeight - H - 8));
        const itemBtn: React.CSSProperties = {
          display: "block",
          width: "100%",
          textAlign: "left",
          padding: "6px 10px",
          fontSize: 12,
          lineHeight: 1.3,
          border: "none",
          background: "transparent",
          color: "var(--pet-color-fg)",
          cursor: "pointer",
          fontFamily: "inherit",
          borderRadius: 4,
        };
        const hoverIn = (e: React.MouseEvent<HTMLButtonElement>) => {
          (e.currentTarget as HTMLButtonElement).style.background =
            "var(--pet-color-bg)";
        };
        const hoverOut = (e: React.MouseEvent<HTMLButtonElement>) => {
          (e.currentTarget as HTMLButtonElement).style.background = "transparent";
        };
        return (
          <div
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => e.stopPropagation()}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
            }}
            style={{
              position: "fixed",
              left,
              top,
              width: W,
              background: "var(--pet-color-card)",
              border: "1px solid var(--pet-color-border)",
              borderRadius: 6,
              boxShadow: "var(--pet-shadow-md)",
              padding: 4,
              zIndex: 50,
              fontFamily: "inherit",
            }}
          >
            <div
              style={{
                padding: "4px 10px 6px",
                fontSize: 11,
                color: "var(--pet-color-muted)",
                borderBottom: "1px solid var(--pet-color-border)",
                marginBottom: 4,
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
              title={m.title}
            >
              {m.title}
            </div>
            <button
              type="button"
              style={itemBtn}
              onMouseOver={hoverIn}
              onMouseOut={hoverOut}
              onClick={() => {
                setSessionTabCtxMenu(null);
                void handleTogglePinned(m.id, !m.pinned);
              }}
            >
              {m.pinned ? "📍 取消 pin" : "📌 pin 置顶"}
            </button>
            <button
              type="button"
              style={itemBtn}
              onMouseOver={hoverIn}
              onMouseOut={hoverOut}
              onClick={() => {
                setSessionTabCtxMenu(null);
                // 展开 dropdown 让用户看到 inline 编辑输入
                setShowSessionList(true);
                const s = sessionList.find((x) => x.id === m.id);
                if (s) startRename(s);
              }}
            >
              ✏ 改名…
            </button>
            <button
              type="button"
              style={itemBtn}
              onMouseOver={hoverIn}
              onMouseOut={hoverOut}
              onClick={async () => {
                setSessionTabCtxMenu(null);
                try {
                  await navigator.clipboard.writeText(m.title);
                  setExportToast(`已复制标题：${m.title}`);
                  setTimeout(() => setExportToast(""), 2500);
                } catch (e) {
                  setExportToast(`复制失败：${e}`);
                  setTimeout(() => setExportToast(""), 3000);
                }
              }}
            >
              📋 复制标题
            </button>
            {/* 🔑 复制会话 ID：debug / 上报 issue 时常需要 session id 定位
                到具体会话文件 / 后端日志。session.id 是 uuid 形态字符串，
                ~36 字符。复用 exportToast 通道反馈，错误透传 navigator.clipboard
                可能抛的权限错。 */}
            <button
              type="button"
              style={itemBtn}
              onMouseOver={hoverIn}
              onMouseOut={hoverOut}
              onClick={async () => {
                setSessionTabCtxMenu(null);
                try {
                  await navigator.clipboard.writeText(m.id);
                  setExportToast(`已复制会话 ID：${m.id.slice(0, 8)}…`);
                  setTimeout(() => setExportToast(""), 2500);
                } catch (e) {
                  setExportToast(`复制失败：${e}`);
                  setTimeout(() => setExportToast(""), 3000);
                }
              }}
              title={`把会话 ID 复制到剪贴板（用于 debug / 上报 issue 时定位具体会话文件）。完整 ID：${m.id}`}
            >
              🔑 复制会话 ID
            </button>
            {/* "重写标题"按钮：调 regenerate_session_title 走非流式 LLM 调用，
                让 LLM 看尾部 ~10 条 turn 给 ≤ 10 字概括。LLM 调用有延迟 +
                费用，故出 toast 表"进行中"避免用户误以为卡住；成功后刷
                session list 让新 title 立即可见。 */}
            <button
              type="button"
              style={itemBtn}
              onMouseOver={hoverIn}
              onMouseOut={hoverOut}
              onClick={async () => {
                setSessionTabCtxMenu(null);
                setExportToast(`✨ 正在让 LLM 重写「${m.title}」的标题…`);
                try {
                  const newTitle = await invoke<string>(
                    "regenerate_session_title",
                    { id: m.id },
                  );
                  // 刷 sessionList 让新 title 立即显
                  const idx = await invoke<SessionIndex>("list_sessions");
                  setSessionList(idx.sessions);
                  // 当前 session 命中时同步 setSessionTitle
                  if (m.id === sessionId) {
                    setSessionTitle(newTitle);
                  }
                  setExportToast(`✨ 已重写标题：${newTitle}`);
                  setTimeout(() => setExportToast(""), 3000);
                } catch (e) {
                  setExportToast(`重写失败：${e}`);
                  setTimeout(() => setExportToast(""), 4000);
                }
              }}
              title="让 LLM 看会话末尾 10 条 turn 自动取个 ≤ 10 字标题（替代默认的『首条 user 消息前 20 字』硬截）。会调一次非流式 LLM，约 1-3s 完成。"
            >
              ✨ LLM 重写标题
            </button>
            <div
              style={{
                height: 1,
                background: "var(--pet-color-border)",
                margin: "4px 0",
              }}
            />
            <button
              type="button"
              style={{ ...itemBtn, color: "var(--pet-tint-red-fg)" }}
              onMouseOver={hoverIn}
              onMouseOut={hoverOut}
              onClick={() => {
                setSessionTabCtxMenu(null);
                // 展开 dropdown 让用户看到"确定？"armed 红按钮状态
                setShowSessionList(true);
                handleDeleteClick(m.id);
              }}
            >
              🗑 删除…
            </button>
          </div>
        );
      })()}
    </div>
  );
}

/// `@` 提及浮窗的容器样式 —— 与 SlashCommandMenu 锚点一致（form relative +
/// bottom:100%）。宽度可以比 slash 菜单稍窄，专注一段任务标题列表。
const mentionMenuContainerStyle: React.CSSProperties = {
  position: "absolute",
  bottom: "100%",
  left: 0,
  right: 0,
  marginBottom: "6px",
  maxHeight: "200px",
  overflowY: "auto",
  background: "var(--pet-color-card)",
  border: "1px solid var(--pet-color-border)",
  borderRadius: "6px",
  boxShadow: "var(--pet-shadow-md)",
  zIndex: 20,
};

/// 折叠版 pending attachments：chip 显总数；hover chip 或 popover 任一时
/// popover 保持展开（用 single hovered state 跟踪整个容器的 mouseEnter /
/// Leave 避免缩略图与 chip 之间空隙翻车）。
function PendingAttachmentsChip({
  images,
  onOpen,
  onRemove,
  onClearAll,
}: {
  images: string[];
  onOpen: (src: string) => void;
  onRemove: (idx: number) => void;
  onClearAll: () => void;
}) {
  const [hovered, setHovered] = useState(false);
  return (
    <div
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        position: "absolute",
        left: 16,
        bottom: "calc(100% - 4px)",
        zIndex: 5,
      }}
    >
      {/* 折叠 chip */}
      <div
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 6,
          padding: "4px 10px",
          background: "var(--pet-color-card)",
          border: "1px solid var(--pet-color-accent)",
          borderRadius: 14,
          boxShadow: "var(--pet-shadow-sm)",
          fontSize: 12,
          color: "var(--pet-color-accent)",
          fontWeight: 600,
          cursor: "default",
          userSelect: "none",
        }}
        title="hover 查看 / 移除 / 清空附件"
      >
        📎 {images.length} 附件待发
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onClearAll();
          }}
          aria-label="清空全部附件"
          title="清空全部附件"
          style={{
            width: 16,
            height: 16,
            borderRadius: "50%",
            border: "none",
            background: "rgba(15,23,42,0.12)",
            color: "var(--pet-color-fg)",
            fontSize: 10,
            lineHeight: 1,
            cursor: "pointer",
            padding: 0,
            marginLeft: 2,
          }}
        >
          ✕
        </button>
      </div>
      {/* hover popover：完整缩略图条 + ✕ 移除按钮 */}
      {hovered && (
        <div
          style={{
            position: "absolute",
            top: "calc(100% + 4px)",
            left: 0,
            display: "flex",
            flexWrap: "wrap",
            gap: 6,
            padding: "6px 8px",
            background: "var(--pet-color-card)",
            border: "1px solid var(--pet-color-border)",
            borderRadius: 6,
            boxShadow: "var(--pet-shadow-md)",
            maxWidth: 380,
            minWidth: 200,
          }}
        >
          {images.map((src, i) => (
            <div key={i} style={{ position: "relative" }}>
              <img
                src={src}
                alt=""
                onClick={() => onOpen(src)}
                title="点击查看大图"
                style={{
                  width: 56,
                  height: 56,
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
                onClick={() => onRemove(i)}
                style={{
                  position: "absolute",
                  top: -6,
                  right: -6,
                  width: 18,
                  height: 18,
                  borderRadius: "50%",
                  border: "none",
                  background: "rgba(15,23,42,0.78)",
                  color: "#fff",
                  fontSize: 11,
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
    </div>
  );
}

/* ---------- Styles ---------- */

const sessionBarStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "8px",
  padding: "10px 16px",
  borderBottom: "1px solid var(--pet-color-border)",
  background: "var(--pet-color-card)",
  flexShrink: 0,
  boxShadow: "0 1px 0 rgba(15, 23, 42, 0.02)",
};

const newSessionBtnStyle: React.CSSProperties = {
  padding: "5px 12px",
  borderRadius: "6px",
  border: "1px solid var(--pet-color-border)",
  background: "var(--pet-color-bg)",
  color: "var(--pet-color-accent)",
  fontSize: "12px",
  fontWeight: 600,
  cursor: "pointer",
  whiteSpace: "nowrap",
};

const sessionDropdownStyle: React.CSSProperties = {
  maxHeight: "240px",
  overflowY: "auto",
  borderBottom: "1px solid var(--pet-color-border)",
  background: "var(--pet-color-card)",
  flexShrink: 0,
};

