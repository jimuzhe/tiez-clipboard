import { useCallback, useEffect, useState, type RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauriRuntime } from "../lib/tauriRuntime";
import { isMacPlatform } from "../lib/platform";
import { isListCovered, isOtherTextField } from "../lib/listKeys";

interface UseNavigationSyncOptions {
  showSettings: boolean;
  showTagManager: boolean;
  chatMode: boolean;
  showEmojiPanel: boolean;
  /** A note or tag is being edited in the list or the detail pane. */
  editingInline: boolean;
  /** A modal dialog (confirm, update) is open over the list. */
  dialogOpen: boolean;
  searchInputRef: RefObject<HTMLInputElement | null>;
}

/**
 * Whether a text field other than the search box has focus. Only tracked
 * where Windows' keyboard hook takes Up/Down/Enter/Delete before the page.
 */
const useOtherTextFieldFocused = (searchInputRef: RefObject<HTMLInputElement | null>) => {
  const [focused, setFocused] = useState(false);

  useEffect(() => {
    if (!isTauriRuntime() || isMacPlatform()) return;

    // A focused field that is removed from the page (e.g. the AI follow-up
    // after it is answered) may get no focusout: watch for that.
    let observer: MutationObserver | null = null;
    const update = (element: EventTarget | null) => {
      observer?.disconnect();
      observer = null;
      const isOther = isOtherTextField(element, searchInputRef.current);
      setFocused(isOther);
      if (!isOther) return;
      const field = element as HTMLElement;
      observer = new MutationObserver(() => {
        if (field.isConnected) return;
        observer?.disconnect();
        observer = null;
        setFocused(false);
      });
      observer.observe(document.body, { childList: true, subtree: true });
    };
    const onFocusIn = (event: FocusEvent) => update(event.target);
    const onFocusOut = (event: FocusEvent) => update(event.relatedTarget);

    update(document.activeElement);
    document.addEventListener("focusin", onFocusIn);
    document.addEventListener("focusout", onFocusOut);
    return () => {
      observer?.disconnect();
      document.removeEventListener("focusin", onFocusIn);
      document.removeEventListener("focusout", onFocusOut);
    };
  }, [searchInputRef]);

  return focused;
};

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
  editingInline,
  dialogOpen,
  searchInputRef
}: UseNavigationSyncOptions) => {
  const textFieldFocused = useOtherTextFieldFocused(searchInputRef);
  // Editing, a dialog or another focused text field also turn it off:
  // Windows' keyboard hook took Up/Down, Enter and Esc from them, so the
  // list moved or pasted behind a dialog and Esc hid the window instead of
  // cancelling the edit.
  const shouldDisableNavigation = isListCovered({
    showSettings,
    showTagManager,
    chatMode,
    showEmojiPanel,
    editingInline,
    dialogOpen,
    textFieldFocused
  });

  const syncNavigationEnabled = useCallback(() => {
    if (!isTauriRuntime()) return;

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
  }, [shouldDisableNavigation]);

  useEffect(() => {
    if (!isTauriRuntime()) return;

    const onClipboardShown = () => {
      if (!isTauriRuntime()) return;
      clearRestoredActionFocus();
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
  }, [syncNavigationEnabled, shouldDisableNavigation]);
};
