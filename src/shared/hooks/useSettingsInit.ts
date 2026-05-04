import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DEFAULT_THEME, normalizeThemeId, isStoreTheme } from "../config/themes";
import type { Locale } from "../types";
import { isTauriRuntime } from "../lib/tauriRuntime";
import { fetchAndCacheStoreTheme, injectStoreThemeCSS } from "../../features/theme-store/hooks/useThemeApply";

interface UseSettingsInitOptions {
  setAppSettings: (settings: Record<string, string>) => void;
  setHotkey: (val: string) => void;
  setTheme: (val: string) => void;
  setColorMode: (val: string) => void;
  setCompactMode: (val: boolean) => void;
  setLanguage: (val: Locale) => void;
}

export const useSettingsInit = ({
  setAppSettings,
  setHotkey,
  setTheme,
  setColorMode,
  setCompactMode,
  setLanguage
}: UseSettingsInitOptions) => {
  const [settings, setSettings] = useState<Record<string, string> | null>(null);
  const settingsEffectCount = useRef(0);

  useEffect(() => {
    if (!isTauriRuntime()) return;

    let disposed = false;

    const loadSettings = () => {
      settingsEffectCount.current++;
      console.log(`[THEME DEBUG] Settings useEffect run #${settingsEffectCount.current}`);

      invoke<Record<string, string>>("get_settings")
        .then((result) => {
          if (disposed) return;

          console.log(
            `[THEME DEBUG] get_settings response (run #${settingsEffectCount.current}):`,
            result
          );
          console.log("[THEME DEBUG] app.color_mode from DB:", result["app.color_mode"]);

          setAppSettings(result);
          if (result["app.hotkey"]) setHotkey(result["app.hotkey"]);

          const loadedTheme = normalizeThemeId(result["app.theme"] || DEFAULT_THEME);
          const loadedColorMode = result["app.color_mode"] || "system";
          console.log("[THEME DEBUG] loadedColorMode:", loadedColorMode);

          // If store theme, inject cached CSS before applying class
          if (isStoreTheme(loadedTheme)) {
            const cached = localStorage.getItem(`tiez_store_css_${loadedTheme}`);
            if (cached) {
              injectStoreThemeCSS(loadedTheme, cached);
            }
            // Re-fetch in background to update cache
            fetchAndCacheStoreTheme(loadedTheme).then((css) => {
              if (css) injectStoreThemeCSS(loadedTheme, css);
            }).catch(() => {});
          }

          setTheme(loadedTheme);
          setColorMode(loadedColorMode);
          setCompactMode(result["app.compact_mode"] === "true");

          try {
            localStorage.setItem("tiez_theme", loadedTheme);
            localStorage.setItem("tiez_color_mode", loadedColorMode);
            localStorage.setItem(
              "tiez_compact_mode",
              result["app.compact_mode"] === "true" ? "true" : "false"
            );
          } catch {
            // Ignore localStorage errors
          }

          if (result["app.language"]) {
            setLanguage(result["app.language"] as Locale);
          }

          setSettings(result);
        })
        .catch(console.error);
    };

    loadSettings();

    const unlisten = listen("settings-changed", () => {
      loadSettings();
    });

    return () => {
      disposed = true;
      unlisten.then((off) => off());
    };
  }, [setAppSettings, setHotkey, setTheme, setColorMode, setCompactMode, setLanguage]);

  return settings;
};
