/** Shared status colors (text + dot) for connected/error/idle indicators. */
export type Tone = "success" | "error" | "muted";

const text: Record<Tone, string> = {
  success: "text-green-600",
  error: "text-red-500",
  muted: "text-ink-faint",
};

const dot: Record<Tone, string> = {
  success: "bg-green-500",
  error: "bg-red-500",
  muted: "bg-ink-faint",
};

export const toneText = (tone: Tone) => text[tone];
export const toneDot = (tone: Tone) => dot[tone];

/** Map a connected/error state to a tone. */
export const connTone = (connected?: boolean, error?: boolean | string | null): Tone =>
  connected ? "success" : error ? "error" : "muted";
