import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { PanelApp } from "./PanelApp";
import "./styles/app.css";

const params = new URLSearchParams(window.location.search);
const isPanel = params.get("window") === "panel";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isPanel ? <PanelApp /> : <App />}</React.StrictMode>,
);
