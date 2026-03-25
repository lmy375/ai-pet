import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { PanelApp } from "./PanelApp";
import { DebugApp } from "./DebugApp";
import "./styles/app.css";

const params = new URLSearchParams(window.location.search);
const windowType = params.get("window");

function Root() {
  if (windowType === "panel") return <PanelApp />;
  if (windowType === "debug") return <DebugApp />;
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode><Root /></React.StrictMode>,
);
