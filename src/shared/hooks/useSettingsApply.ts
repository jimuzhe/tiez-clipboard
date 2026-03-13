import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

type PlatformInfo = {
  platform: string;
  is_windows_10: boolean;
  is_windows_11: boolean;
};

const clearThemeClasses = (element: HTMLElement) => {
  Array.from(element.classList)
    .filter(className => className.startsWith("theme-"))
    .forEach(className => element.classList.remove(className));
};

interface UseSettingsApplyOptions {
  theme: string;
  colorMode: string;
  showAppBorder: boolean;
  compactMode: boolean;
  settingsLoaded: boolean;
  clipboardItemFontSize: number;
  clipboardTagFontSize: number;
  surfaceOpacity: number;
}

export const useSettingsApply = ({
  theme,
  colorMode,
  showAppBorder,
  compactMode,
  settingsLoaded,
  clipboardItemFontSize,
  clipboardTagFontSize,
  surfaceOpacity
}: UseSettingsApplyOptions) => {
  useEffect(() => {
    if (!settingsLoaded) return;

    const root = document.documentElement;
    const body = document.body;

    let disposed = false;

    const applyExplicitMode = (mode: "light" | "dark") => {
      if (disposed) return;
      root.classList.remove("light-mode", "dark-mode");
      body.classList.remove("light-mode", "dark-mode");
      if (mode === "dark") {
        root.classList.add("dark-mode");
        body.classList.add("dark-mode");
      } else {
        root.classList.add("light-mode");
        body.classList.add("light-mode");
      }
    };

    const applySystemMode = async () => {
      try {
        const current = await getCurrentWindow().theme();
        if (disposed) return;
        applyExplicitMode(current === "dark" ? "dark" : "light");
      } catch {
        if (disposed) return;
        const isDark =
          window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
        applyExplicitMode(isDark ? "dark" : "light");
      }
    };

    clearThemeClasses(root);
    clearThemeClasses(body);
    root.classList.add(`theme-${theme}`);
    body.classList.add(`theme-${theme}`);
    invoke<PlatformInfo>("get_platform_info")
      .then((info) => {
        if (disposed) return;
        root.classList.toggle("windows-10", !!info?.is_windows_10);
        body.classList.toggle("windows-10", !!info?.is_windows_10);
        root.classList.toggle("windows-11", !!info?.is_windows_11);
        body.classList.toggle("windows-11", !!info?.is_windows_11);
      })
      .catch(() => {
        if (disposed) return;
        root.classList.remove("windows-10", "windows-11");
        body.classList.remove("windows-10", "windows-11");
      });
    root.classList.toggle("hide-app-border", !showAppBorder);
    body.classList.toggle("hide-app-border", !showAppBorder);

    if (compactMode) {
      body.classList.add("compact-mode");
    } else {
      body.classList.remove("compact-mode");
    }

    if (colorMode === "light") {
      applyExplicitMode("light");
    } else if (colorMode === "dark") {
      applyExplicitMode("dark");
    } else {
      applySystemMode();
    }

    invoke("set_theme", {
      theme,
      color_mode: colorMode,
      show_app_border: showAppBorder
    }).catch(console.error);

    let unlistenThemeChanged: (() => void) | null = null;
    let cleanupMedia: (() => void) | null = null;

    getCurrentWindow()
      .onThemeChanged((event) => {
        if (disposed) return;

        if (colorMode === "system") {
          const next = event?.payload === "dark" ? "dark" : "light";
          applyExplicitMode(next);
        } else {
          applyExplicitMode(colorMode === "dark" ? "dark" : "light");
        }

        // Native mica/acrylic may be refreshed by the OS when system theme changes.
        // Re-apply the user's selected mode so the window background stays locked.
        invoke("set_theme", {
          theme,
          color_mode: colorMode,
          show_app_border: showAppBorder
        }).catch(console.error);
      })
      .then((f) => {
        if (disposed) {
          f();
          return;
        }
        unlistenThemeChanged = f;
      });

    if (colorMode === "system") {
      if (window.matchMedia) {
        const media = window.matchMedia("(prefers-color-scheme: dark)");
        const onChange = () => applyExplicitMode(media.matches ? "dark" : "light");
        if (media.addEventListener) {
          media.addEventListener("change", onChange);
          cleanupMedia = () => media.removeEventListener("change", onChange);
        } else {
          media.addListener(onChange);
          cleanupMedia = () => media.removeListener(onChange);
        }
      }
    }

    return () => {
      disposed = true;
      if (unlistenThemeChanged) unlistenThemeChanged();
      if (cleanupMedia) cleanupMedia();
    };
  }, [theme, colorMode, showAppBorder, settingsLoaded, compactMode]);

  useEffect(() => {
    if (!settingsLoaded) return;
    const root = document.documentElement;
    root.style.setProperty("--clipboard-item-font-size", `${clipboardItemFontSize}px`);
    root.style.setProperty("--clipboard-tag-font-size", `${clipboardTagFontSize}px`);
    const scale = Math.min(2, Math.max(0, surfaceOpacity / 50));
    root.style.setProperty("--surface-opacity-scale", scale.toString());
  }, [clipboardItemFontSize, clipboardTagFontSize, surfaceOpacity, settingsLoaded]);
};
