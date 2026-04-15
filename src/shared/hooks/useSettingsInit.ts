import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DEFAULT_THEME, normalizeThemeId } from "../config/themes";
import type { Locale } from "../types";

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

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;

    const loadSettings = async () => {
      try {
        const result = await invoke<Record<string, string>>("get_settings");
        if (!alive) return;

        setAppSettings(result);
        if (result["app.hotkey"]) setHotkey(result["app.hotkey"]);

        const loadedTheme = normalizeThemeId(result["app.theme"] || DEFAULT_THEME);
        const loadedColorMode = result["app.color_mode"] || "system";

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
      } catch (e) {
        console.error(e);
      }
    };

    void loadSettings();

    (async () => {
      try {
        unlisten = await listen("settings-changed", () => {
          void loadSettings();
        });
      } catch (e) {
        console.error(e);
      }
    })();

    return () => {
      alive = false;
      if (unlisten) unlisten();
    };
  }, [setAppSettings, setHotkey, setTheme, setColorMode, setCompactMode, setLanguage]);

  return settings;
};
