import { useEffect } from "react";
import type { Dispatch, SetStateAction } from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../../../shared/lib/tauriRuntime";

/**
 * A revealed sensitive entry stays revealed only while the window is up.
 * The backend sends `clipboard-hidden` whenever it hides the main window;
 * `clipboard-shown` catches a hide that did not say so.
 */
export const REVEAL_RESET_EVENTS = ["clipboard-hidden", "clipboard-shown"] as const;

const clearReveals = (prev: Set<number>) => (prev.size === 0 ? prev : new Set<number>());

export const useRevealReset = (setRevealedIds: Dispatch<SetStateAction<Set<number>>>) => {
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const unlisteners = REVEAL_RESET_EVENTS.map((name) =>
      listen(name, () => setRevealedIds(clearReveals))
    );
    return () => {
      unlisteners.forEach((unlisten) => unlisten.then((off) => off()).catch(() => {}));
    };
  }, [setRevealedIds]);
};
