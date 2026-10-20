import { useCallback, useEffect, useState } from "react";

interface Options {
  storageKey: string;
  initialWidth: number;
  minWidth: number;
  maxWidth: number;
}

function clampWidth(width: number, minWidth: number, maxWidth: number): number {
  return Math.min(maxWidth, Math.max(minWidth, width));
}

export function useResizableWidth({ storageKey, initialWidth, minWidth, maxWidth }: Options) {
  const [width, setWidthState] = useState(() => {
    const saved = Number(localStorage.getItem(`panel-width:${storageKey}`));
    return clampWidth(Number.isFinite(saved) && saved > 0 ? saved : initialWidth, minWidth, maxWidth);
  });

  const setWidth = useCallback(
    (next: number) => setWidthState(clampWidth(next, minWidth, maxWidth)),
    [minWidth, maxWidth],
  );

  useEffect(() => {
    localStorage.setItem(`panel-width:${storageKey}`, String(width));
  }, [storageKey, width]);

  return { width, setWidth };
}
