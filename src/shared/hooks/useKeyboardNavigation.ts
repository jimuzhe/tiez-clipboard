import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { RefObject } from "react";
import { matchesHotkey } from "./useHotkeyMatching";
import { useWindowVisibility } from "./useWindowVisibility";
import type { ClipboardEntry } from "../types";

interface UseKeyboardNavigationOptions {
  filteredHistory: ClipboardEntry[];
  selectedIndex: number;
  setSelectedIndex: (val: number | ((prev: number) => number)) => void;
  isKeyboardMode: boolean;
  setIsKeyboardMode: (val: boolean | ((prev: boolean) => boolean)) => void;
  showSettings: boolean;
  showTagManager: boolean;
  chatMode: boolean;
  editingTagsId: number | null;
  arrowKeySelection: boolean;
  richPasteHotkey: string;
  searchInputRef: RefObject<HTMLInputElement | null>;
  copyToClipboard: (id: number, content: string, contentType: string, pasteWithFormat?: boolean) => Promise<void>;
  setSearch: (val: string) => void;
}

export const useKeyboardNavigation = ({
  filteredHistory,
  selectedIndex,
  setSelectedIndex,
  isKeyboardMode,
  setIsKeyboardMode,
  showSettings,
  showTagManager,
  chatMode,
  editingTagsId,
  arrowKeySelection,
  richPasteHotkey,
  searchInputRef,
  copyToClipboard,
  setSearch
}: UseKeyboardNavigationOptions) => {
  const filteredHistoryRef = useRef(filteredHistory);
  const selectedIndexRef = useRef(selectedIndex);
  const isKeyboardModeRef = useRef(isKeyboardMode);
  const isWindowVisibleRef = useWindowVisibility();
  const showSettingsRef = useRef(showSettings);
  const showTagManagerRef = useRef(showTagManager);
  const chatModeRef = useRef(chatMode);
  const editingTagsIdRef = useRef(editingTagsId);
  const arrowKeySelectionRef = useRef(arrowKeySelection);
  const copyToClipboardRef = useRef(copyToClipboard);
  const richPasteHotkeyRef = useRef(richPasteHotkey);

  useEffect(() => { filteredHistoryRef.current = filteredHistory; }, [filteredHistory]);
  useEffect(() => { selectedIndexRef.current = selectedIndex; }, [selectedIndex]);
  useEffect(() => { isKeyboardModeRef.current = isKeyboardMode; }, [isKeyboardMode]);
  useEffect(() => { showSettingsRef.current = showSettings; }, [showSettings]);
  useEffect(() => { showTagManagerRef.current = showTagManager; }, [showTagManager]);
  useEffect(() => { chatModeRef.current = chatMode; }, [chatMode]);
  useEffect(() => { editingTagsIdRef.current = editingTagsId; }, [editingTagsId]);
  useEffect(() => { arrowKeySelectionRef.current = arrowKeySelection; }, [arrowKeySelection]);
  useEffect(() => { copyToClipboardRef.current = copyToClipboard; }, [copyToClipboard]);
  useEffect(() => { richPasteHotkeyRef.current = richPasteHotkey; }, [richPasteHotkey]);
  useEffect(() => {
    invoke("set_navigation_mode", { active: isKeyboardMode }).catch(console.error);
  }, [isKeyboardMode]);

  useEffect(() => {
    let isPastingLocal = false;

    const handleKeyDown = async (e: KeyboardEvent) => {
      if (!isWindowVisibleRef.current) return;
      if (isPastingLocal) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }

      if (
        showSettingsRef.current ||
        showTagManagerRef.current ||
        chatModeRef.current ||
        editingTagsIdRef.current !== null
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
          } else {
              const isClipboardAtTop = !isKeyboardModeRef.current || selectedIndexRef.current <= 0;
              if (isClipboardAtTop) {
                invoke("hide_window_cmd");
              } else {
                setIsKeyboardMode(true);
                setSelectedIndex(0);
              }
          }
          return;
      }

      if (isEditable) {
          if (e.key === "Enter" && isSearchInput) {
              // Fall through
          } else {
              if (!arrowKeySelectionRef.current) return;
              if (!isSearchInput) return;
          }
      }

      if (arrowKeySelectionRef.current && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
        e.preventDefault();
        e.stopPropagation();

        setIsKeyboardMode((prev) => {
          if (!prev) {
            setSelectedIndex(0);
            return true;
          }

          if (e.key === "ArrowDown") {
            setSelectedIndex((s) => Math.min(s + 1, filteredHistoryRef.current.length - 1));
          } else {
            setSelectedIndex((s) => Math.max(s - 1, 0));
          }
          return true;
        });
        return;
      }

      const matchesRichHotkey = matchesHotkey(e, richPasteHotkeyRef.current);
      const shouldHandleEnter = e.key === "Enter" && isKeyboardModeRef.current;
      if (shouldHandleEnter || matchesRichHotkey) {
        const isRich = matchesRichHotkey;
        e.preventDefault();
        e.stopPropagation();

        const currentIndex = selectedIndexRef.current;
        const currentHistory = filteredHistoryRef.current;

        if (currentIndex >= 0 && currentIndex < currentHistory.length) {
          isPastingLocal = true;
          const item = currentHistory[currentIndex];

          setIsKeyboardMode(false);
          setSelectedIndex(0);

          if (copyToClipboardRef.current) {
            await copyToClipboardRef.current(
              item.id,
              item.content,
              item.content_type,
              isRich
            );
          }

          setTimeout(() => {
            isPastingLocal = false;
          }, 500);
        }
        return;
      }
    };

    const handleInteraction = () => {
      setIsKeyboardMode(false);
    };

    window.addEventListener("keydown", handleKeyDown, true);
    window.addEventListener("mousedown", handleInteraction);

    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
      window.removeEventListener("mousedown", handleInteraction);
    };
  }, [searchInputRef, setIsKeyboardMode, setSelectedIndex]);

  useEffect(() => {
    const unlisten = listen<string>("navigation-action", async (event) => {
      try {
        const isVisible = await getCurrentWindow().isVisible();
        if (!isVisible) return;
      } catch (err) {
        console.warn("Failed to check window visibility:", err);
      }

      if (showSettings || showTagManager || chatMode || editingTagsId !== null) return;

      const action = event.payload;
      const history = filteredHistoryRef.current;
      const currentIndex = selectedIndexRef.current;
      const isNavMode = isKeyboardModeRef.current;

      if ((action === "up" || action === "down") && !arrowKeySelection) {
        return;
      }

      if (action === "up") {
        if (!isNavMode) {
          setIsKeyboardMode(true);
          setSelectedIndex(0);
        } else {
          setSelectedIndex((prev) => Math.max(prev - 1, 0));
        }
      } else if (action === "down") {
        if (!isNavMode) {
          setIsKeyboardMode(true);
          setSelectedIndex(0);
        } else {
          setSelectedIndex((prev) => Math.min(prev + 1, history.length - 1));
        }
      } else if (action === "enter") {
        if (!isNavMode) return;
        if (currentIndex >= 0 && currentIndex < history.length) {
          const item = history[currentIndex];
          copyToClipboard(item.id, item.content, item.content_type, false);
        }
      } else if (action === "escape") {
        setSearch("");
        setIsKeyboardMode(false);
      }
    });

    return () => { unlisten.then(f => f()); };
  }, [
    arrowKeySelection,
    chatMode,
    copyToClipboard,
    editingTagsId,
    setIsKeyboardMode,
    setSearch,
    setSelectedIndex,
    showSettings,
    showTagManager
  ]);
};


