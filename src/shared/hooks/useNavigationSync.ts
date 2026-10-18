import { useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauriRuntime } from "../lib/tauriRuntime";

interface UseNavigationSyncOptions {
  showSettings: boolean;
  showTagManager: boolean;
  chatMode: boolean;
  showEmojiPanel: boolean;
  /** A note or tag is being edited in the list or the detail pane. */
  editingInline: boolean;
}

const blurStaleActionFocus = () => {
  const activeElement = document.activeElement;
  if (!(activeElement instanceof HTMLElement)) return;

  const isEditable =
    activeElement.matches("input, textarea, select") ||
    activeElement.isContentEditable ||
    Boolean(activeElement.closest("[contenteditable='true']"));
  if (isEditable) return;

  if (activeElement.matches("button, a[href], [role='button'], [tabindex]")) {
    activeElement.blur();
  }
};

const clearRestoredActionFocus = () => {
  // WebKit can restore the previous control focus while the NSPanel is being
  // ordered to the front, so clear once now and once after the next paint.
  blurStaleActionFocus();
  requestAnimationFrame(blurStaleActionFocus);
};

export const useNavigationSync = ({
  showSettings,
  showTagManager,
  chatMode,
  showEmojiPanel,
  editingInline
}: UseNavigationSyncOptions) => {
  const syncNavigationEnabled = useCallback(() => {
    if (!isTauriRuntime()) return;

    // Editing also turns it off: Windows' keyboard hook took Up/Down, Enter
    // and Esc from the text box, and Esc hid the window instead of
    // cancelling the edit.
    const shouldDisableNavigation =
      showSettings || showTagManager || chatMode || showEmojiPanel || editingInline;
    if (shouldDisableNavigation) {
      invoke("set_navigation_enabled", { enabled: false }).catch(console.error);
      return;
    }

    // Clipboard list mode: only enable keyboard/mouse routing when the window is visible.
    getCurrentWindow()
      .isVisible()
      .then((visible) => {
        invoke("set_navigation_enabled", { enabled: visible }).catch(console.error);
      })
      .catch(() => {
        invoke("set_navigation_enabled", { enabled: false }).catch(console.error);
      });
  }, [showSettings, showTagManager, chatMode, showEmojiPanel, editingInline]);

  useEffect(() => {
    if (!isTauriRuntime()) return;

    const onClipboardShown = () => {
      if (!isTauriRuntime()) return;
      clearRestoredActionFocus();
      const shouldDisableNavigation =
        showSettings || showTagManager || chatMode || showEmojiPanel || editingInline;
      if (shouldDisableNavigation) {
        invoke("set_navigation_enabled", { enabled: false }).catch(console.error);
        return;
      }
      // NSPanel may not report isVisible() immediately; always arm outside-click routing.
      invoke("set_navigation_enabled", { enabled: true }).catch(console.error);
    };

    syncNavigationEnabled();

    const appWindow = getCurrentWindow();
    const unlistenShow = appWindow.listen("tauri://show", () => {
      clearRestoredActionFocus();
      syncNavigationEnabled();
    });
    const unlistenClipboardShown = listen("clipboard-shown", onClipboardShown);
    const unlistenHide = appWindow.listen("tauri://hide", () => {
      invoke("set_navigation_enabled", { enabled: false }).catch(console.error);
    });

    return () => {
      unlistenShow.then((unlisten) => unlisten()).catch(() => {});
      unlistenClipboardShown.then((unlisten) => unlisten()).catch(() => {});
      unlistenHide.then((unlisten) => unlisten()).catch(() => {});
    };
  }, [syncNavigationEnabled, showSettings, showTagManager, chatMode, showEmojiPanel, editingInline]);
};
