import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import CompactPreviewWindow from "./features/clipboard/components/CompactPreviewWindow";
import "./index.css";
import "./styles/components/index.css";

const themeCssLoaders = import.meta.glob("./styles/themes/*.css");

const preloadBootTheme = () => {
  const defaultTheme = "mica";
  const bootTheme = localStorage.getItem("tiez_theme") || defaultTheme;
  const bootThemePath = `./styles/themes/${bootTheme}.css`;
  const bootLoader = themeCssLoaders[bootThemePath];
  if (bootLoader) {
    bootLoader();
    return;
  }
  const fallbackLoader = themeCssLoaders[`./styles/themes/${defaultTheme}.css`];
  if (fallbackLoader) {
    fallbackLoader();
  }
};

preloadBootTheme();

const params = new URLSearchParams(window.location.search);
const isCompactPreview = params.get("window") === "compact-preview";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isCompactPreview ? <CompactPreviewWindow /> : <App />}
  </React.StrictMode>,
);
