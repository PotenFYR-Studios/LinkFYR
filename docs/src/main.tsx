import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import "./index.css";

/**
 * LinkFYR docs site (discord-botlists docs architecture): Vite + React +
 * react-router + prerendered HTML per route. Content lives in
 * src/docs/content.ts as typed blocks; there is no markdown.
 */

const base = import.meta.env.BASE_URL;
const basename = base === "/" ? undefined : base.replace(/\/$/, "");

const rootEl = document.getElementById("root");
if (rootEl) {
  createRoot(rootEl).render(
    <BrowserRouter basename={basename}>
      <App />
    </BrowserRouter>,
  );
}
