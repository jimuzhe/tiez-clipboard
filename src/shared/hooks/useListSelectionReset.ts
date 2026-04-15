import { useEffect } from "react";
import type { ClipboardEntry } from "../types";

interface UseListSelectionResetOptions {
  filteredHistory: ClipboardEntry[];
  setSelectedIndex: (val: number) => void;
}

export const useListSelectionReset = ({
  filteredHistory,
  setSelectedIndex
}: UseListSelectionResetOptions) => {
  useEffect(() => {
    setSelectedIndex(0);
  }, [filteredHistory, setSelectedIndex]);
};


