import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/app.css";

// Paint the correct theme before first frame to avoid a flash.
const root = document.documentElement;
root.dataset.theme = window.matchMedia("(prefers-color-scheme: light)").matches
  ? "light"
  : "dark";
root.dataset.motion = "full";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
