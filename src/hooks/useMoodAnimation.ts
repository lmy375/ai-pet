import { useEffect } from "react";
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

const HAPPY_KEYWORDS = ["开心", "兴奋", "愉快", "高兴", "期待", "喜欢", "好心情", "满足"];
const ENERGETIC_KEYWORDS = ["想分享", "活泼", "想说", "兴致", "好奇", "热闹"];
const RESTLESS_KEYWORDS = ["烦", "焦虑", "无聊", "急", "不安", "纠结"];
const QUIET_KEYWORDS = ["低落", "难过", "担心", "想念", "平静", "沉静", "安静", "累"];

function pickMotionGroup(mood: string | null | undefined): MotionGroup {
  if (!mood) return "Idle";
  const m = mood.toLowerCase();
  if (HAPPY_KEYWORDS.some((k) => m.includes(k))) return "Tap";
  if (ENERGETIC_KEYWORDS.some((k) => m.includes(k))) return "Flick";
  if (RESTLESS_KEYWORDS.some((k) => m.includes(k))) return "Flick3";
  if (QUIET_KEYWORDS.some((k) => m.includes(k))) return "Idle";
  // No keyword hit — play a Tap to give the proactive utterance a visible beat.
  return "Tap";
}

interface ProactivePayload {
  text: string;
  timestamp: string;
  mood: string | null;
}

interface ChatDonePayload {
  mood: string | null;
  timestamp: string;
}

function triggerMotion(model: any, mood: string | null | undefined) {
  if (!model) return;
  const group = pickMotionGroup(mood);
  try {
    // pixi-live2d-display: model.motion(group, index?, priority?). Priority 2 = NORMAL,
    // letting the motion play through but not interrupting a higher-priority one.
    model.motion(group, undefined, 2);
  } catch (e) {
    // Some models throw if a group has no motions; safe to ignore.
    console.debug("motion trigger failed:", e);
  }
}

export function useMoodAnimation(modelRef: React.MutableRefObject<any>) {
  useEffect(() => {
    let unlistenProactive: (() => void) | undefined;
    let unlistenChatDone: (() => void) | undefined;
    (async () => {
      unlistenProactive = await listen<ProactivePayload>("proactive-message", (event) => {
        triggerMotion(modelRef.current, event.payload.mood);
      });
      unlistenChatDone = await listen<ChatDonePayload>("chat-done", (event) => {
        triggerMotion(modelRef.current, event.payload.mood);
      });
    })();
    return () => {
      if (unlistenProactive) unlistenProactive();
      if (unlistenChatDone) unlistenChatDone();
    };
  }, [modelRef]);
}
