import { useCallback, useState } from "react";

export interface StatusMessage {
  text: string;
  ok: boolean;
}

/**
 * One inline status line for panel forms: the message plus its success/failure
 * flag. Render it with `<StatusText>`, so the green/red colors come from a
 * single place instead of being re-derived at each call site.
 */
export function useStatusMessage() {
  const [message, setMessage] = useState<StatusMessage | null>(null);
  const ok = useCallback((text: string) => setMessage({ text, ok: true }), []);
  const fail = useCallback((text: string) => setMessage({ text, ok: false }), []);
  const clear = useCallback(() => setMessage(null), []);
  return { message, ok, fail, clear };
}