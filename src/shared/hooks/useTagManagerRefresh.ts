import { useEffect } from "react";

interface UseTagManagerRefreshOptions {
  showTagManager: boolean;
  settingsLoaded: boolean;
  persistentLimitEnabled: boolean;
  persistentLimit: number;
  fetchHistory: (reset?: boolean) => void;
}

export const useTagManagerRefresh = ({
  showTagManager,
  settingsLoaded,
  persistentLimitEnabled,
  persistentLimit,
  fetchHistory
}: UseTagManagerRefreshOptions) => {
  useEffect(() => {
    if (!settingsLoaded) return;
    if (!showTagManager) {
      fetchHistory(true);
    }
  }, [showTagManager, settingsLoaded, persistentLimitEnabled, persistentLimit, fetchHistory]);
};
