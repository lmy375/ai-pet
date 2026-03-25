import { PanelDebug } from "./components/panel/PanelDebug";

export function DebugApp() {
  return (
    <div style={{ width: "100%", height: "100vh", display: "flex", flexDirection: "column", background: "#f8fafc" }}>
      <PanelDebug />
    </div>
  );
}
