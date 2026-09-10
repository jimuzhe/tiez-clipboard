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

      // 置顶项在 Virtuoso header 中，不能用 scrollToItem；需对 DOM 做 scrollIntoView。
      // 之前一律 scrollToTop，导致置顶超出视口时方向键切换不会跟随滚动。
      if (isPinned) {
        const el = document.getElementById(`${idPrefix}${item.id}`);
        if (el) {
          el.scrollIntoView({ behavior: "auto", block: "nearest" });
        } else if (virtualListRef?.current?.scrollToTop) {
          virtualListRef.current.scrollToTop();
        }
        return;
      }

      const targetIndex = selectedIndex - pinnedCount;
      if (virtualListRef?.current && targetIndex >= 0) {
        virtualListRef.current.scrollToItem(targetIndex);
        return;
      }

      const el = document.getElementById(`${idPrefix}${item.id}`);
      if (el) {
        el.scrollIntoView({ behavior: "auto", block: "nearest" });
      }
    }
  }, [filteredHistory, selectedIndex, isKeyboardMode, idPrefix, pinnedCount, virtualListRef]);
};


