import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { MobileGate } from "./MobileGate";
import "../../desktop/src/styles/app.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <MobileGate />
  </StrictMode>,
);
