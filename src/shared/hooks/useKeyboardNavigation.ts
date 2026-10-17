import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { RefObject } from "react";
import { matchesHotkey } from "./useHotkeyMatching";
import { isMacPlatform } from "../lib/platform";
import { isTauriRuntime } from "../lib/tauriRuntime";
import type { ClipboardEntry } from "../types";

interface UseKeyboardNavigationOptions {
  filteredHistory: ClipboardEntry[];
  selectedIndex: number;
  selectedItemId: number | null;
  setSelectedIndex: (val: number | ((prev: number) => number)) => void;
  setSelectedItemId: (val: number | null) => void;
  isKeyboardMode: boolean;
  setIsKeyboardMode: (val: boolean | ((prev: boolean) => boolean)) => void;
  showSettings: boolean;
  showTagManager: boolean;
  chatMode: boolean;
  editingTagsId: number | null;
  editingNoteId: number | null;
  arrowKeySelection: boolean;
  plainPasteHotkey: string;
  searchInputRef: RefObject<HTMLInputElement | null>;
  copyToClipboard: (
    id: number,
    content: string,
    contentType: string,
    pasteWithFormat?: boolean,
    isPinned?: boolean,
    tags?: string[]
  ) => Promise<void>;
  setSearch: (val: string) => void;
  /** Deletes an entry from the keyboard; false if it was refused or failed. */
  deleteItem: (item: ClipboardEntry) => Promise<boolean>;
}

export const useKeyboardNavigation = ({
  filteredHistory,
  selectedIndex,
  selectedItemId,
  setSelectedIndex,
  setSelectedItemId,
  isKeyboardMode,
  setIsKeyboardMode,
  showSettings,
  showTagManager,
  chatMode,
  editingTagsId,
  editingNoteId,
  arrowKeySelection,
  plainPasteHotkey,
  searchInputRef,
  copyToClipboard,
  setSearch,
  deleteItem
}: UseKeyboardNavigationOptions) => {
  const filteredHistoryRef = useRef(filteredHistory);
  const selectedIndexRef = useRef(selectedIndex);
  const selectedItemIdRef = useRef<number | null>(selectedItemId);
  const selectedItemSnapshotRef = useRef<ClipboardEntry | null>(null);
  const isKeyboardModeRef = useRef(isKeyboardMode);
  const showSettingsRef = useRef(showSettings);
  const showTagManagerRef = useRef(showTagManager);
  const chatModeRef = useRef(chatMode);
  const editingTagsIdRef = useRef(editingTagsId);
  const editingNoteIdRef = useRef(editingNoteId);
  const arrowKeySelectionRef = useRef(arrowKeySelection);
  const copyToClipboardRef = useRef(copyToClipboard);
  const plainPasteHotkeyRef = useRef(plainPasteHotkey);
  const lastNavigationKeyRef = useRef<{ key: string; time: number }>({ key: "", time: 0 });
  const pasteInFlightRef = useRef(false);

  filteredHistoryRef.current = filteredHistory;
  isKeyboardModeRef.current = isKeyboardMode;
  showSettingsRef.current = showSettings;
  showTagManagerRef.current = showTagManager;
  chatModeRef.current = chatMode;
  editingTagsIdRef.current = editingTagsId;
  editingNoteIdRef.current = editingNoteId;
  arrowKeySelectionRef.current = arrowKeySelection;
  copyToClipboardRef.current = copyToClipboard;
  plainPasteHotkeyRef.current = plainPasteHotkey;

  useEffect(() => {
    selectedIndexRef.current = selectedIndex;
    selectedItemIdRef.current = selectedItemId;
    selectedItemSnapshotRef.current =
      filteredHistoryRef.current.find((entry) => entry.id === selectedItemId) ??
      filteredHistoryRef.current[selectedIndex] ??
      null;
  }, [selectedIndex, selectedItemId]);

  // macOS NSPanel is non-key on show; arrow DOM navigation works after make_key_window.
  // Native HID navigation is only needed on Windows (and legacy Mac non-key panels).
  const usesNativeNavigation = () =>
    isTauriRuntime() && arrowKeySelectionRef.current && !isMacPlatform();

  const applySelection = (index: number, enableKeyboard = true) => {
    const history = filteredHistoryRef.current;
    const maxIndex = Math.max(0, history.length - 1);
    const nextIndex = Math.max(0, Math.min(index, maxIndex));
    const item = history[nextIndex];

    selectedIndexRef.current = nextIndex;
    selectedItemIdRef.current = item?.id ?? null;
    selectedItemSnapshotRef.current = item ?? null;

    if (enableKeyboard) {
      setIsKeyboardMode(true);
    }
    setSelectedIndex(nextIndex);
    setSelectedItemId(item?.id ?? null);
  };

  const resolveSelectedItem = (history: ClipboardEntry[]) => {
    const snapshot = selectedItemSnapshotRef.current;
    if (snapshot) {
      const bySnapshotId = history.find((entry) => entry.id === snapshot.id);
      if (bySnapshotId) return bySnapshotId;
    }

    const itemId = selectedItemIdRef.current;
    if (itemId != null) {
      const byId = history.find((entry) => entry.id === itemId);
      if (byId) return byId;
    }

    const currentIndex = selectedIndexRef.current;
    if (currentIndex >= 0 && currentIndex < history.length) {
      return history[currentIndex];
    }

    const selectedElement = document.querySelector<HTMLElement>(
      ".history-item.selected[id^='clipboard-item-']"
    );
    const visualItemId = selectedElement
      ? Number(selectedElement.id.replace("clipboard-item-", ""))
      : NaN;
    if (!Number.isNaN(visualItemId)) {
      const visualItem = history.find((entry) => entry.id === visualItemId);
      if (visualItem) return visualItem;
    }

    return null;
  };

  const deleteInFlightRef = useRef(false);
  const deleteItemRef = useRef(deleteItem);
  deleteItemRef.current = deleteItem;

  // Delete the selected entry and keep the keyboard selection on the entry
  // that takes its place (the next one, or the previous one at the end).
  const deleteSelectedItem = async () => {
    if (deleteInFlightRef.current) return;
    const history = filteredHistoryRef.current;
    const item = resolveSelectedItem(history);
    if (!item) return;
    const index = history.findIndex((entry) => entry.id === item.id);
    const neighbour = history[index + 1] ?? history[index - 1] ?? null;

    deleteInFlightRef.current = true;
    try {
      if (!(await deleteItemRef.current(item))) return;
      const nextIndex = Math.max(0, Math.min(index, history.length - 2));
      selectedIndexRef.current = nextIndex;
      selectedItemIdRef.current = neighbour?.id ?? null;
      selectedItemSnapshotRef.current = neighbour;
      setSelectedIndex(nextIndex);
      setSelectedItemId(neighbour?.id ?? null);
    } finally {
      deleteInFlightRef.current = false;
    }
  };

  // Delete (any platform) or Command+Backspace (macOS, as in Finder). In the
  // search box only when it can't mean text editing: Delete with the caret at
  // the end, Command+Backspace with the box empty. Plain Backspace never
  // deletes an entry: it is how people correct the search.
  const isDeleteKey = (e: KeyboardEvent, target: HTMLElement) => {
    const isForwardDelete = e.key === "Delete" && !e.metaKey && !e.ctrlKey && !e.altKey;
    const isCommandBackspace = isMacPlatform() && e.metaKey && e.key === "Backspace";
    if (!isForwardDelete && !isCommandBackspace) return false;
    if (!(target instanceof HTMLInputElement)) return true;
    const { value, selectionStart, selectionEnd } = target;
    if (isForwardDelete) return selectionStart === value.length && selectionEnd === value.length;
    return value.length === 0;
  };

  const resetSelectionToTop = () => {
    const history = filteredHistoryRef.current;
    const firstId = history[0]?.id ?? null;
    selectedIndexRef.current = 0;
    selectedItemIdRef.current = firstId;
    selectedItemSnapshotRef.current = history[0] ?? null;
    isKeyboardModeRef.current = false;

    setIsKeyboardMode(false);
    setSelectedIndex(0);
    setSelectedItemId(firstId);
  };

  const pasteSelectedItem = async (isRich: boolean, explicitItem?: ClipboardEntry) => {
    if (pasteInFlightRef.current) return;

    const history = filteredHistoryRef.current;
    const item = explicitItem ?? resolveSelectedItem(history);
    if (!item) return;

    pasteInFlightRef.current = true;

    const { id, content, content_type, is_pinned, tags } = item;

    try {
      if (copyToClipboardRef.current) {
        await copyToClipboardRef.current(
          id,
          content,
          content_type,
          isRich,
          is_pinned,
          tags || []
        );
      }
    } finally {
      resetSelectionToTop();
      window.setTimeout(() => {
        pasteInFlightRef.current = false;
      }, 500);
    }
  };

  useEffect(() => {
    if (!isTauriRuntime()) return;
    invoke("set_navigation_mode", { active: isKeyboardMode }).catch(console.error);
  }, [isKeyboardMode]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const appWindow = getCurrentWindow();
    const resetKeyboardNavigation = () => {
      resetSelectionToTop();
    };

    const unlistenHide = appWindow.listen("tauri://hide", resetKeyboardNavigation);

    return () => {
      unlistenHide.then(f => f());
    };
  }, [setIsKeyboardMode, setSelectedIndex, setSelectedItemId]);

  useEffect(() => {
    const handleKeyDown = async (e: KeyboardEvent) => {
      // A real keydown already means this window is receiving input; do not gate
      // on Tauri isVisible() (NSPanel can report false while the panel is key).
      if (e.defaultPrevented) return;
      // Never steal IME / printable input from focused fields.
      if (e.isComposing || e.key === "Process") return;

      if (
        showSettingsRef.current ||
        showTagManagerRef.current ||
        chatModeRef.current ||
        editingTagsIdRef.current !== null ||
        editingNoteIdRef.current !== null
      ) {
        return;
      }

      const target = e.target as HTMLElement;
      const tagName = target.tagName;
      const isSearchInput = target.classList.contains("search-input");
      const isAnyInput = tagName === "INPUT" || tagName === "TEXTAREA";
      const isEditable = isAnyInput || target.isContentEditable === true;

      if (e.key === "Escape") {
        e.preventDefault();
        if (isEditable) {
          searchInputRef.current?.blur();
        } else if (usesNativeNavigation()) {
          return;
        } else {
          const isClipboardAtTop = !isKeyboardModeRef.current || selectedIndexRef.current <= 0;
          if (isClipboardAtTop) {
            resetSelectionToTop();
            if (isTauriRuntime()) {
              invoke("hide_window_cmd").catch(console.error);
            }
          } else {
            applySelection(0);
          }
        }
        return;
      }

      if (isKeyboardModeRef.current && (!isEditable || isSearchInput) && isDeleteKey(e, target)) {
        // Windows' keyboard hook sends this as a navigation action instead.
        if (usesNativeNavigation()) return;
        e.preventDefault();
        e.stopPropagation();
        await deleteSelectedItem();
        return;
      }

      // In inputs, only allow search ↑/↓ selection and search Enter-to-paste.
      // Printable keys / Backspace / IME must reach the field unchanged.
      if (isEditable) {
        const isSearchArrowNav =
          isSearchInput &&
          arrowKeySelectionRef.current &&
          (e.key === "ArrowDown" || e.key === "ArrowUp");
        const isSearchEnter = isSearchInput && e.key === "Enter";
        if (!isSearchArrowNav && !isSearchEnter) {
          return;
        }
      }

      if (arrowKeySelectionRef.current && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
        if (usesNativeNavigation()) {
          e.preventDefault();
          e.stopPropagation();
          return;
        }

        e.preventDefault();
        e.stopPropagation();

        const now = performance.now();
        const lastNavigationKey = lastNavigationKeyRef.current;
        if (lastNavigationKey.key === e.key && now - lastNavigationKey.time < 90) {
          return;
        }
        lastNavigationKeyRef.current = { key: e.key, time: now };

        const maxIndex = Math.max(0, filteredHistoryRef.current.length - 1);
        const current = isKeyboardModeRef.current ? selectedIndexRef.current : -1;
        const nextIndex = e.key === "ArrowDown"
          ? Math.min(current + 1, maxIndex)
          : Math.max(current <= 0 ? 0 : current - 1, 0);

        isKeyboardModeRef.current = true;
        applySelection(nextIndex);
        return;
      }

      const matchesPlainHotkey = matchesHotkey(e, plainPasteHotkeyRef.current);
      const shouldHandleEnter = e.key === "Enter" && isKeyboardModeRef.current;
      if (shouldHandleEnter || matchesPlainHotkey) {
        if (shouldHandleEnter && !matchesPlainHotkey && usesNativeNavigation()) {
          e.preventDefault();
          e.stopPropagation();
          return;
        }

        const isRich = !matchesPlainHotkey;
        e.preventDefault();
        e.stopPropagation();

        const history = filteredHistoryRef.current;
        const item = resolveSelectedItem(history);
        if (!item) return;

        await pasteSelectedItem(isRich, item);
        return;
      }
    };

    const handleInteraction = () => {
      isKeyboardModeRef.current = false;
      setIsKeyboardMode(false);
    };

    window.addEventListener("keydown", handleKeyDown, true);
    window.addEventListener("mousedown", handleInteraction);

    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
      window.removeEventListener("mousedown", handleInteraction);
    };
  }, [searchInputRef, setIsKeyboardMode, setSelectedIndex, setSelectedItemId]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const unlisten = listen<string>("navigation-action", async (event) => {
      // Prefer document focus over Tauri isVisible — NSPanel visibility can lag.
      let windowLooksOpen = document.hasFocus() || document.visibilityState === "visible";
      if (!windowLooksOpen) {
        try {
          windowLooksOpen = await getCurrentWindow().isVisible();
        } catch (err) {
          console.warn("Failed to check window visibility:", err);
        }
      }
      if (!windowLooksOpen) return;

      if (showSettingsRef.current || showTagManagerRef.current || chatModeRef.current || editingTagsIdRef.current !== null || editingNoteIdRef.current !== null) {
        return;
      }

      const action = event.payload;
      const history = filteredHistoryRef.current;
      const currentIndex = selectedIndexRef.current;
      const isNavMode = isKeyboardModeRef.current;

      if ((action === "up" || action === "down") && !arrowKeySelectionRef.current) {
        return;
      }

      if (action === "up") {
        const now = performance.now();
        const lastNavigationKey = lastNavigationKeyRef.current;
        if (lastNavigationKey.key === action && now - lastNavigationKey.time < 90) {
          return;
        }
        lastNavigationKeyRef.current = { key: action, time: now };

        const nextIndex = isNavMode ? Math.max(currentIndex - 1, 0) : 0;
        isKeyboardModeRef.current = true;
        applySelection(nextIndex);
      } else if (action === "down") {
        const now = performance.now();
        const lastNavigationKey = lastNavigationKeyRef.current;
        if (lastNavigationKey.key === action && now - lastNavigationKey.time < 90) {
          return;
        }
        lastNavigationKeyRef.current = { key: action, time: now };

        const maxIndex = Math.max(0, history.length - 1);
        const nextIndex = isNavMode ? Math.min(currentIndex + 1, maxIndex) : 0;
        isKeyboardModeRef.current = true;
        applySelection(nextIndex);
      } else if (action === "enter") {
        if (!isNavMode) return;

        const now = performance.now();
        const lastNavigationKey = lastNavigationKeyRef.current;
        if (lastNavigationKey.key === action && now - lastNavigationKey.time < 350) {
          return;
        }
        lastNavigationKeyRef.current = { key: action, time: now };

        const item = resolveSelectedItem(history);
        if (!item) return;
        await pasteSelectedItem(true, item);
      } else if (action === "delete") {
        if (!isNavMode) return;
        await deleteSelectedItem();
      } else if (action === "escape") {
        setSearch("");
        resetSelectionToTop();
      }
    });

    return () => { unlisten.then(f => f()); };
  }, [setIsKeyboardMode, setSearch, setSelectedIndex, setSelectedItemId]);

  const selectItemByIndex = (index: number) => {
    // Remember the clicked paste target without leaving a persistent highlight.
    // Hover is purely visual and must not overwrite a keyboard-selected target.
    applySelection(index, false);
  };

  return {
    selectItemByIndex
  };
};
