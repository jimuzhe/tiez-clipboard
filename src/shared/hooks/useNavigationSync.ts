import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface UseNavigationSyncOptions {
  showSettings: boolean;
  showTagManager: boolean;
  showEmojiPanel: boolean;
}

export const useNavigationSync = ({
  showSettings,
  showTagManager,
  showEmojiPanel
}: UseNavigationSyncOptions) => {
  useEffect(() => {
    const shouldDisableNavigation = showSettings || showTagManager || showEmojiPanel;
    if (shouldDisableNavigation) {
      invoke("set_navigation_enabled", { enabled: false }).catch(console.error);
      return;
    }

    // Only enable global navigation when the window is actually visible.
    getCurrentWindow()
      .isVisible()
      .then((visible) => {
        invoke("set_navigation_enabled", { enabled: visible }).catch(console.error);
      })
      .catch(() => {
        invoke("set_navigation_enabled", { enabled: false }).catch(console.error);
      });
  }, [showSettings, showTagManager, showEmojiPanel]);
};
