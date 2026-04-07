import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../lib/tauriRuntime";

interface UseToastListenerOptions {
  pushToast: (msg: string, duration?: number) => number;
}

export const useToastListener = ({ pushToast }: UseToastListenerOptions) => {
  useEffect(() => {
    if (!isTauriRuntime()) return;

    const unlistenToast = listen<string>("toast", (event) => {
      pushToast(event.payload, 3000);
    });
    return () => {
      unlistenToast.then((f) => f());
    };
  }, [pushToast]);
};
