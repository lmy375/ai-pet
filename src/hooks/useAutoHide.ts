import { useEffect, useRef, useState } from "react";

const IDLE_TIMEOUT = 1000;

export function useAutoHide() {
  const [minimized, setMinimized] = useState(false);
  const state = useRef({
    minimized: false,
    timer: null as ReturnType<typeof setTimeout> | null,
  });

  const startIdleTimer = () => {
    const s = state.current;
    if (s.timer) clearTimeout(s.timer);
    s.timer = setTimeout(() => {
      if (!s.minimized) {
        s.minimized = true;
        setMinimized(true);
      }
    }, IDLE_TIMEOUT);
  };

  const handleActivity = () => {
    const s = state.current;
    if (s.minimized) {
      s.minimized = false;
      setMinimized(false);
      startIdleTimer();
      return;
    }
    startIdleTimer();
  };

  useEffect(() => {
    startIdleTimer();
    return () => {
      if (state.current.timer) clearTimeout(state.current.timer);
    };
  }, []);

  return { minimized, handleActivity };
}
