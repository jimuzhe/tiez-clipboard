import React, { lazy, Suspense } from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import "./styles/components/index.css";
import "./styles/themes/load";
import "./styles/platform-windows.css";
import "./styles/platform-macos.css";
import "./styles/components/pin-sticker.css";
import { installBrowserAcceleratorBlocker } from "./shared/lib/blockBrowserAccelerators";
import ErrorBoundary from "./shared/components/ErrorBoundary";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTauriRuntime } from "./shared/lib/tauriRuntime";

// Suppress Chromium/WebView browser accelerators in every window (main, preview, settings…).
installBrowserAcceleratorBlocker();

const params = new URLSearchParams(window.location.search);
const isCompactPreview = params.get("window") === "compact-preview";
const isPinSticker = params.get("window") === "pin-sticker";
const RootComponent = isCompactPreview
  ? lazy(() => import("./features/clipboard/components/CompactPreviewWindow"))
  : isPinSticker
    ? lazy(() => import("./features/clipboard/components/PinStickerWindow"))
    : lazy(() => import("./App"));
const isMacPlatform =
  /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent) || /Mac/i.test(navigator.platform);

if (isPinSticker) {
  document.documentElement.classList.add("window-pin-sticker");
  document.body.classList.add("window-pin-sticker");
}

if (isMacPlatform) {
  document.documentElement.classList.add("platform-macos");
  document.body.classList.add("platform-macos");
} else {
  document.documentElement.classList.add("platform-windows");
  document.body.classList.add("platform-windows");
}

// macOS 26+ puts the 磨砂 theme on Liquid Glass, which gets its own layout
// (themes/acrylic.css); the web view can't tell the macOS version itself.
if (isMacPlatform && isTauriRuntime()) {
  invoke<{ has_liquid_glass: boolean; window_corner_radius: number }>("get_platform_info")
    .then(({ has_liquid_glass, window_corner_radius }) => {
      if (!has_liquid_glass) return;
      // The 磨砂 shell copies the system window corners (acrylic.css).
      if (window_corner_radius > 0) {
        document.documentElement.style.setProperty("--macos-window-radius", `${window_corner_radius}px`);
      }
      document.documentElement.classList.add("macos-liquid-glass");
      document.body.classList.add("macos-liquid-glass");
    })
    .catch(() => {});
}

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
root.render(
  <React.StrictMode>
    <ErrorBoundary>
      <Suspense fallback={null}>
        <RootComponent />
      </Suspense>
    </ErrorBoundary>
  </React.StrictMode>,
);

// Light mode is about to drop this page. Unmount first so every effect cleanup
// unregisters its Tauri event listener; Tauri keeps them across reloads
// otherwise. Rust waits for the ack (or gives up after a second).
if (!isCompactPreview && !isPinSticker && isTauriRuntime()) {
  const offRelease = listen("light-mode-release", async () => {
    root.unmount();
    (await offRelease)();
    // Let the unlisten calls from the cleanups go out before Rust navigates.
    await new Promise((resolve) => setTimeout(resolve, 100));
    await invoke("main_ui_released").catch(() => {});
  });
}
