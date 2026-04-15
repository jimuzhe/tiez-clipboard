import { useEffect } from "react";

interface UseSearchFetchTriggerOptions {
  debouncedSearch: string;
  isComposing: boolean;
  typeFilter?: string | null;
  fetchHistory: (reset?: boolean) => void;
}

export const useSearchFetchTrigger = ({
  debouncedSearch,
  isComposing,
  typeFilter,
  fetchHistory
}: UseSearchFetchTriggerOptions) => {
  useEffect(() => {
    if (!isComposing) {
      fetchHistory(true);
    }
  }, [debouncedSearch, isComposing, fetchHistory]);

  useEffect(() => {
    fetchHistory(true);
  }, [typeFilter, fetchHistory]);
};
