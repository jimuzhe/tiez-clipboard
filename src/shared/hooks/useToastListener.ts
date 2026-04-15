import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

interface UseToastListenerOptions {
  pushToast: (msg: string, duration?: number) => number;
}

export const useToastListener = ({ pushToast }: UseToastListenerOptions) => {
  useEffect(() => {
    const unlistenToast = listen<string>("toast", (event) => {
      pushToast(event.payload, 3000);
    });
    return () => {
      unlistenToast.then((f) => f());
    };
  }, [pushToast]);
};
