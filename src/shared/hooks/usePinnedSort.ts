import { useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Dispatch, SetStateAction } from "react";
import type { ClipboardEntry } from "../types";

interface UsePinnedSortOptions {
  filteredHistory: ClipboardEntry[];
  history: ClipboardEntry[];
  setHistory: Dispatch<SetStateAction<ClipboardEntry[]>>;
}

export const usePinnedSort = ({
  filteredHistory,
  history,
  setHistory
}: UsePinnedSortOptions) => {
  const { pinnedItems, unpinnedItems } = useMemo(() => {
    return {
      pinnedItems: filteredHistory.filter((item) => item.is_pinned),
      unpinnedItems: filteredHistory.filter((item) => !item.is_pinned)
    };
  }, [filteredHistory]);

  const handlePinnedReorder = useCallback(
    async (newOrderIds: number[]) => {
      const orderMap = new Map<number, number>();
      newOrderIds.forEach((id, index) => {
        orderMap.set(id, newOrderIds.length - index);
      });

      const nextHistory = history.map((item) => {
        const nextOrder = orderMap.get(item.id);
        if (nextOrder !== undefined) {
          return { ...item, pinned_order: nextOrder };
        }
        return item;
      });

      nextHistory.sort((a, b) => {
        if (a.is_pinned !== b.is_pinned) return a.is_pinned ? -1 : 1;
        if (a.is_pinned) {
          if ((a.pinned_order || 0) !== (b.pinned_order || 0)) {
            return (b.pinned_order || 0) - (a.pinned_order || 0);
          }
        }
        return b.timestamp - a.timestamp;
      });

      setHistory(nextHistory);

      const dbOrders = newOrderIds.map((id, index) => [id, newOrderIds.length - index]);
      invoke("update_pinned_order", { orders: dbOrders }).catch(console.error);
    },
    [history, setHistory]
  );

  return {
    pinnedItems,
    unpinnedItems,
    handlePinnedReorder
  };
};


