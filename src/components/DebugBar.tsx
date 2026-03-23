import { useState } from "react";
import { getCurrentWindow, currentMonitor } from "@tauri-apps/api/window";
import { PhysicalPosition } from "@tauri-apps/api/dpi";

/**
 * Debug toolbar for testing Tauri window APIs.
 * Usage: <DebugBar /> — add to App.tsx when debugging, remove when done.
 */
export function DebugBar() {
  const [info, setInfo] = useState("ready");

  const testMove = async (dx: number) => {
    const win = getCurrentWindow();
    try {
      const pos = await win.outerPosition();
      const newX = pos.x + dx;
      await win.setPosition(new PhysicalPosition(newX, pos.y));
      setInfo(`moved to x=${newX}, y=${pos.y}`);
    } catch (e: any) {
      setInfo(`ERROR: ${e.message || e}`);
    }
  };

  const testMonitor = async () => {
    try {
      const monitor = await currentMonitor();
      if (!monitor) {
        setInfo("no monitor!");
        return;
      }
      const win = getCurrentWindow();
      const pos = await win.outerPosition();
      const size = await win.outerSize();
      setInfo(
        `win: ${pos.x},${pos.y} ${size.width}x${size.height} | screen: ${monitor.position.x},${monitor.position.y} ${monitor.size.width}x${monitor.size.height} scale:${monitor.scaleFactor}`,
      );
    } catch (e: any) {
      setInfo(`ERROR: ${e.message || e}`);
    }
  };

  const testReset = async () => {
    const win = getCurrentWindow();
    try {
      await win.setPosition(new PhysicalPosition(100, 100));
      setInfo("reset to 100,100");
    } catch (e: any) {
      setInfo(`ERROR: ${e.message || e}`);
    }
  };

  return (
    <div
      onMouseDown={(e) => e.stopPropagation()}
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        right: 0,
        background: "rgba(0,0,0,0.85)",
        color: "#0f0",
        fontSize: "10px",
        padding: "4px 8px",
        zIndex: 100,
        display: "flex",
        gap: "6px",
        alignItems: "center",
        flexWrap: "wrap",
      }}
    >
      <button onClick={() => testMove(-100)} style={btnStyle}>
        ← 100
      </button>
      <button onClick={() => testMove(100)} style={btnStyle}>
        → 100
      </button>
      <button onClick={testMonitor} style={btnStyle}>
        Monitor
      </button>
      <button onClick={testReset} style={btnStyle}>
        Reset
      </button>
      <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }}>
        {info}
      </span>
    </div>
  );
}

const btnStyle: React.CSSProperties = {
  fontSize: "10px",
  padding: "2px 6px",
  cursor: "pointer",
};
