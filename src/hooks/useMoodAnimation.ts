import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

/**
 * Drive Live2D motions from the pet's current mood. The miku model exposes four motion
 * groups: Tap, Flick, Flick3, Idle. We pick a group by keyword-matching the mood string
 * the LLM wrote to memory after the previous proactive turn.
 *
 * Match list is intentionally short — the LLM writes free-form Chinese, so we cover the
 * most-likely persona keywords and fall back to Idle. If nothing matches, we still play a
 * gentle motion so the pet visibly reacts when speaking proactively.
 */
type MotionGroup = "Tap" | "Flick" | "Flick3" | "Idle";

const VALID_GROUPS: ReadonlySet<MotionGroup> = new Set<MotionGroup>([
  "Tap",
  "Flick",
  "Flick3",
  "Idle",
]);

const HAPPY_KEYWORDS = ["开心", "兴奋", "愉快", "高兴", "期待", "喜欢", "好心情", "满足"];
const ENERGETIC_KEYWORDS = ["想分享", "活泼", "想说", "兴致", "好奇", "热闹"];
const RESTLESS_KEYWORDS = ["烦", "焦虑", "无聊", "急", "不安", "纠结"];
const QUIET_KEYWORDS = ["低落", "难过", "担心", "想念", "平静", "沉静", "安静", "累"];

/**
 * Map a mood string to a motion group via keyword matching. Used as the fallback when the
 * LLM's structured `[motion: X]` tag is missing or invalid.
 */
function pickMotionGroupFromMood(mood: string | null | undefined): MotionGroup {
  if (!mood) return "Idle";
  const m = mood.toLowerCase();
  if (HAPPY_KEYWORDS.some((k) => m.includes(k))) return "Tap";
  if (ENERGETIC_KEYWORDS.some((k) => m.includes(k))) return "Flick";
  if (RESTLESS_KEYWORDS.some((k) => m.includes(k))) return "Flick3";
  if (QUIET_KEYWORDS.some((k) => m.includes(k))) return "Idle";
  // No keyword hit — play a Tap to give the utterance a visible beat.
  return "Tap";
}

/**
 * Pick a motion group, preferring an explicit tag from the LLM. Validates the tag against
 * the model's known groups; an unknown tag is treated as missing and falls back to the
 * keyword matcher.
 */
function pickMotionGroup(
  motion: string | null | undefined,
  mood: string | null | undefined,
): MotionGroup {
  if (motion && VALID_GROUPS.has(motion as MotionGroup)) {
    return motion as MotionGroup;
  }
  return pickMotionGroupFromMood(mood);
}

interface ProactivePayload {
  text: string;
  timestamp: string;
  mood: string | null;
  motion: string | null;
}

interface ChatDonePayload {
  mood: string | null;
  motion: string | null;
  timestamp: string;
}

/// 用户自定义映射：把语义键（Tap / Flick / Flick3 / Idle）翻译成当前 model
/// 实际的 motion group 名。空字符串 / 缺键 → 用语义键本身（沿用既有 miku 行为）。
function resolveGroupName(
  semantic: MotionGroup,
  mapping: Record<string, string> | undefined,
): string {
  const mapped = mapping?.[semantic]?.trim();
  return mapped && mapped.length > 0 ? mapped : semantic;
}

function triggerMotion(
  model: any,
  motion: string | null | undefined,
  mood: string | null | undefined,
  mapping: Record<string, string> | undefined,
) {
  if (!model) return;
  const semantic = pickMotionGroup(motion, mood);
  const group = resolveGroupName(semantic, mapping);
  try {
    // pixi-live2d-display: model.motion(group, index?, priority?). Priority 2 = NORMAL,
    // letting the motion play through but not interrupting a higher-priority one.
    model.motion(group, undefined, 2);
  } catch (e) {
    // Some models throw if a group has no motions; safe to ignore.
    console.debug("motion trigger failed:", e);
  }
}

export function useMoodAnimation(
  modelRef: React.MutableRefObject<any>,
  /// 可选自定义映射；不传 / 空 map 时所有语义键直接走语义名（向前兼容）。
  /// 用 ref 模式让 mapping 变化即时生效，无需重订阅 Tauri listen。
  motionMapping?: Record<string, string>,
) {
  const mappingRef = useRef<Record<string, string> | undefined>(motionMapping);
  useEffect(() => {
    mappingRef.current = motionMapping;
  }, [motionMapping]);

  useEffect(() => {
    let unlistenProactive: (() => void) | undefined;
    let unlistenChatDone: (() => void) | undefined;
    (async () => {
      unlistenProactive = await listen<ProactivePayload>("proactive-message", (event) => {
        triggerMotion(
          modelRef.current,
          event.payload.motion,
          event.payload.mood,
          mappingRef.current,
        );
      });
      unlistenChatDone = await listen<ChatDonePayload>("chat-done", (event) => {
        triggerMotion(
          modelRef.current,
          event.payload.motion,
          event.payload.mood,
          mappingRef.current,
        );
      });
    })();
    return () => {
      if (unlistenProactive) unlistenProactive();
      if (unlistenChatDone) unlistenChatDone();
    };
  }, [modelRef]);
}
