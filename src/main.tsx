import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import CompactPreviewWindow from "./features/clipboard/components/CompactPreviewWindow";
import "./index.css";
import "./styles/components/index.css";
import "./styles/themes/load";

const params = new URLSearchParams(window.location.search);
const isCompactPreview = params.get("window") === "compact-preview";
const isScrollBench = params.get("bench") === "scroll";
const isMacPlatform =
  /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent) || /Mac/i.test(navigator.platform);

if (isCompactPreview) {
  document.body.classList.add("compact-preview");
}

if (isMacPlatform || isScrollBench) {
  document.documentElement.classList.add("platform-macos");
  document.body.classList.add("platform-macos");
  if (!document.getElementById("shell-backdrop")) {
    const veil = document.createElement("div");
    veil.id = "shell-backdrop";
    veil.setAttribute("aria-hidden", "true");
    const rootEl = document.getElementById("root");
    if (rootEl) document.body.insertBefore(veil, rootEl);
  }
}

if (isScrollBench) {
  document.documentElement.classList.add("theme-sakura", "light-mode");
  document.documentElement.classList.remove("theme-mica", "theme-acrylic", "theme-paper", "theme-sticky-note", "dark-mode");
  document.body.classList.add("theme-sakura", "light-mode");
}

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);

if (isScrollBench) {
  void import("./features/clipboard/components/ScrollBench").then(({ ScrollBench }) => {
    root.render(
      <React.StrictMode>
        <ScrollBench />
      </React.StrictMode>,
    );
  });
} else {
  root.render(
    <React.StrictMode>
      {isCompactPreview ? <CompactPreviewWindow /> : <App />}
    </React.StrictMode>,
  );
}
