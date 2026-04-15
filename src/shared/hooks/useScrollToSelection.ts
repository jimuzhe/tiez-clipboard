import { useEffect } from "react";
import type { RefObject } from "react";
import type { ClipboardEntry } from "../types";
import type { VirtualClipboardListHandle } from "../../features/clipboard/types";

interface UseScrollToSelectionOptions {
  filteredHistory: ClipboardEntry[];
  selectedIndex: number;
  isKeyboardMode: boolean;
  idPrefix?: string;
  pinnedCount?: number;
  virtualListRef?: RefObject<VirtualClipboardListHandle | null>;
}

export const useScrollToSelection = ({
  filteredHistory,
  selectedIndex,
  isKeyboardMode,
  idPrefix = "clipboard-item-",
  pinnedCount = 0,
  virtualListRef
}: UseScrollToSelectionOptions) => {
  useEffect(() => {
    if (isKeyboardMode && selectedIndex >= 0 && selectedIndex < filteredHistory.length) {
      const item = filteredHistory[selectedIndex];
      const isPinned = selectedIndex < pinnedCount;
      if (isPinned && virtualListRef?.current?.scrollToTop) {
        virtualListRef.current.scrollToTop();
        return;
      }

      const targetIndex = selectedIndex - pinnedCount;
      if (virtualListRef?.current && targetIndex >= 0) {
        virtualListRef.current.scrollToItem(targetIndex);
        return;
      } else if (virtualListRef?.current && pinnedCount > 0) {
        virtualListRef.current.scrollToItem(0);
        return;
      }

      const el = document.getElementById(`${idPrefix}${item.id}`);
      if (el) {
        el.scrollIntoView({ behavior: "auto", block: "nearest" });
      }
    }
  }, [filteredHistory, selectedIndex, isKeyboardMode, idPrefix, pinnedCount, virtualListRef]);
};


