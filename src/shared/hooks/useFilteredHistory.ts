import { useMemo } from "react";
import type { ClipboardEntry } from "../types";

interface UseFilteredHistoryOptions {
  history: ClipboardEntry[];
  debouncedSearch: string;
  search: string;
  typeFilter: string | null;
}

export const useFilteredHistory = ({
  history,
  debouncedSearch,
  search,
  typeFilter
}: UseFilteredHistoryOptions) => {
  return useMemo(() => {
    const lowerSearch = search.toLowerCase();

    const filtered = history.filter((item) => {
      if (typeFilter && item.content_type !== typeFilter) {
        return false;
      }

      let effectiveSearch = lowerSearch;
      if (effectiveSearch.startsWith("tag:")) {
        effectiveSearch = effectiveSearch.slice(4);
      }

      if (debouncedSearch && debouncedSearch === search) {
        return true;
      }

      if (!effectiveSearch) return true;

      return (
        item.content?.toLowerCase().includes(effectiveSearch) ||
        item.tags?.some((tag) => tag.toLowerCase().includes(effectiveSearch))
      );
    });

    return filtered.sort((a, b) => {
      if (a.is_pinned !== b.is_pinned) {
        return a.is_pinned ? -1 : 1;
      }
      if (a.is_pinned) {
        if ((a.pinned_order || 0) !== (b.pinned_order || 0)) {
          return (b.pinned_order || 0) - (a.pinned_order || 0);
        }
        return b.timestamp - a.timestamp;
      }
      return b.timestamp - a.timestamp;
    });
  }, [history, debouncedSearch, search, typeFilter]);
};


