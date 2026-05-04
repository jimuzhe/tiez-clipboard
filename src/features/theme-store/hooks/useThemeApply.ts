import { useCallback, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  applyThemeClasses,
  isStoreTheme,
  DEFAULT_THEME,
} from "../../../shared/config/themes";
import * as api from "../api";

const STYLE_PREFIX = "store-theme-";

function getStyleElement(themeId: string): HTMLStyleElement {
  const id = `${STYLE_PREFIX}${themeId}`;
  let el = document.getElementById(id) as HTMLStyleElement | null;
  if (!el) {
    el = document.createElement("style");
    el.id = id;
    document.head.appendChild(el);
  }
  return el;
}

function removeStyleElement(themeId: string) {
  const id = `${STYLE_PREFIX}${themeId}`;
  const el = document.getElementById(id);
  if (el) el.remove();
}

export function injectStoreThemeCSS(themeId: string, css: string) {
  const el = getStyleElement(themeId);
  el.textContent = css;
}

export function removeStoreThemeCSS(themeId: string) {
  removeStyleElement(themeId);
}

export async function fetchAndCacheStoreTheme(
  themeId: string
): Promise<string | null> {
  try {
    const css = await api.fetchThemeCSS(themeId);
    localStorage.setItem(`tiez_store_css_${themeId}`, css);
    return css;
  } catch {
    return null;
  }
}

interface UseThemeApplyOptions {
  theme: string;
  setTheme: (val: string) => void;
  saveAppSetting: (key: string, val: string) => void;
  showToast?: (msg: string) => void;
}

export function useThemeApply({
  theme,
  setTheme,
  saveAppSetting,
  showToast,
}: UseThemeApplyOptions) {
  const currentStoreRef = useRef<string | null>(null);

  // Clean up previous store theme style when theme changes
  useEffect(() => {
    if (currentStoreRef.current && currentStoreRef.current !== theme) {
      // Don't remove - keep in cache for quick re-apply
    }
  }, [theme]);

  const applyStoreTheme = useCallback(
    async (themeId: string) => {
      try {
        // Try cache first
        const cached = localStorage.getItem(`tiez_store_css_${themeId}`);
        if (cached) {
          injectStoreThemeCSS(themeId, cached);
        }

        // Fetch fresh CSS
        const css = await fetchAndCacheStoreTheme(themeId);
        if (css) {
          injectStoreThemeCSS(themeId, css);
        } else if (!cached) {
          throw new Error("Failed to load theme");
        }

        // Apply theme class
        applyThemeClasses(themeId, document.documentElement, document.body);

        // Persist
        setTheme(themeId);
        saveAppSetting("theme", themeId);
        localStorage.setItem("tiez_theme", themeId);

        // Notify Rust backend
        try {
          await invoke("set_theme", { theme: themeId });
        } catch {
          // Non-critical
        }

        currentStoreRef.current = themeId;
      } catch (err) {
        console.error("Failed to apply store theme:", err);
        showToast?.("Failed to apply theme, falling back to default");
        // Fall back to default
        applyThemeClasses(
          DEFAULT_THEME,
          document.documentElement,
          document.body
        );
        setTheme(DEFAULT_THEME);
        saveAppSetting("theme", DEFAULT_THEME);
        localStorage.setItem("tiez_theme", DEFAULT_THEME);
        try {
          await invoke("set_theme", { theme: DEFAULT_THEME });
        } catch {
          // Non-critical
        }
      }
    },
    [setTheme, saveAppSetting, showToast]
  );

  // On mount: if current theme is a store theme, inject cached CSS
  useEffect(() => {
    if (isStoreTheme(theme)) {
      const cached = localStorage.getItem(`tiez_store_css_${theme}`);
      if (cached) {
        injectStoreThemeCSS(theme, cached);
        currentStoreRef.current = theme;
      }
      // Re-fetch in background
      fetchAndCacheStoreTheme(theme).then((css) => {
        if (css) injectStoreThemeCSS(theme, css);
      });
    }
  }, [theme]);

  return { applyStoreTheme };
}
