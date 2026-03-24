import { useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Dispatch, SetStateAction } from "react";
import type { ClipboardEntry } from "../types";
import { HISTORY_SEARCH_LIMIT } from "../../features/app/constants/pagination";

interface UseHistoryFetchOptions {
  debouncedSearch: string;
  typeFilter: string | null;
  persistentLimitEnabled: boolean;
  persistentLimit: number;
  pageSize: number;
  currentOffset: number;
  historyLength: number;
  setHistory: Dispatch<SetStateAction<ClipboardEntry[]>>;
  setCurrentOffset: Dispatch<SetStateAction<number>>;
  setHasMore: Dispatch<SetStateAction<boolean>>;
  isLoadingMore: boolean;
  hasMore: boolean;
  setIsLoadingMore: Dispatch<SetStateAction<boolean>>;
}

export const useHistoryFetch = ({
  debouncedSearch,
  typeFilter,
  pageSize,
  currentOffset,
  historyLength,
  setHistory,
  setCurrentOffset,
  setHasMore,
  isLoadingMore,
  hasMore,
  setIsLoadingMore
}: UseHistoryFetchOptions) => {
  const loadingRef = useRef(false);
  const fetchSeqRef = useRef(0);
  const lastRequestedOffsetRef = useRef<number | null>(null);
  const currentOffsetRef = useRef(currentOffset);
  const historyLengthRef = useRef(historyLength);

  useEffect(() => {
    currentOffsetRef.current = currentOffset;
  }, [currentOffset]);

  useEffect(() => {
    historyLengthRef.current = historyLength;
  }, [historyLength]);
  const fetchHistory = useCallback(
    async (reset = false) => {
      const seq = ++fetchSeqRef.current;
      try {
        if (reset) {
          lastRequestedOffsetRef.current = null;
        }

        const baseOffset = reset
          ? 0
          : Math.min(currentOffsetRef.current, historyLengthRef.current);

        let data: ClipboardEntry[] = [];

        const hasSearch = debouncedSearch && debouncedSearch.trim().length > 0;

        if (hasSearch) {
          let term = debouncedSearch;
          if (term.startsWith("tag:")) {
            term = term.slice(4);
          }

          try {
            data = await invoke<ClipboardEntry[]>("search_clipboard_history", {
              searchTerm: term,
              limit: HISTORY_SEARCH_LIMIT
            });
          } catch (e) {
            console.error("Search failed, falling back", e);
            data = [];
          }

          if (seq !== fetchSeqRef.current) return;
          // Search results are not paginated; always replace list and stop infinite loading.
          setHistory(data);
          setCurrentOffset(data.length);
          setHasMore(false);
        } else {
          const requestedLimit = pageSize + 1; // Use standard page size for DB limit
          const rawData = await invoke<ClipboardEntry[]>("get_clipboard_history", {
            limit: requestedLimit,
            offset: baseOffset,
            content_type: typeFilter || undefined
          });

          if (seq !== fetchSeqRef.current) return;

          const hasMoreNow = rawData.length > pageSize;
          const data = hasMoreNow ? rawData.slice(0, pageSize) : rawData;

          // Calculate how many DB items we actually retrieved (id > 0)
          // This is critical for the next offset to be correct
          const dbItemsCount = data.filter(item => item.id > 0).length;

          if (reset) {
            setHistory(data);
            setCurrentOffset(dbItemsCount);
            setHasMore(hasMoreNow);
          } else {
            let nextItems: ClipboardEntry[] = [];
            setHistory((prev) => {
              const existingIds = new Set(prev.map((item) => item.id));
              nextItems = data.filter((item) => !existingIds.has(item.id) || item.id === 0);

              if (nextItems.length === 0) return prev;
              return [...prev, ...nextItems];
            });

            setCurrentOffset(prev => prev + dbItemsCount);
            // If we didn't add any NEW items but the backend says there are more,
            // it means the items we got were already in our list (maybe shifted due to sorting).
            // We should keep hasMore true so the user can try to load further.
            setHasMore(hasMoreNow);
          }
        }
      } catch (err) {
        console.error("无法获取历史记录", err);
        setHasMore(false);
      }
    },
    [
      debouncedSearch,
      typeFilter,
      pageSize,
      setCurrentOffset,
      setHasMore,
      setHistory
    ]
  );

  const loadMoreHistory = useCallback(async () => {
    if (loadingRef.current || isLoadingMore || !hasMore) return;
    if (debouncedSearch && debouncedSearch.trim().length > 0) return;

    const effectiveOffset = Math.min(currentOffsetRef.current, historyLengthRef.current);
    if (lastRequestedOffsetRef.current === effectiveOffset) return;
    lastRequestedOffsetRef.current = effectiveOffset;

    loadingRef.current = true;
    setIsLoadingMore(true);
    try {
      await fetchHistory(false);
    } finally {
      loadingRef.current = false;
      setIsLoadingMore(false);
    }
  }, [debouncedSearch, fetchHistory, hasMore, isLoadingMore, setIsLoadingMore]);

  return { fetchHistory, loadMoreHistory };
};

