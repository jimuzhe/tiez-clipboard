import React, { Suspense, lazy } from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import "./styles/components/index.css";

const App = lazy(() => import("./App"));
const CompactPreviewWindow = lazy(() => import("./features/clipboard/components/CompactPreviewWindow"));

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
    <Suspense fallback={null}>
      {isCompactPreview ? <CompactPreviewWindow /> : <App />}
    </Suspense>
  </React.StrictMode>,
);
