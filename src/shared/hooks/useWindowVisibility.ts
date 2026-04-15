import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export const useWindowVisibility = () => {
  const isWindowVisibleRef = useRef(true);

  useEffect(() => {
    const unlistenBlur = listen("tauri://blur", () => { isWindowVisibleRef.current = false; });
    const unlistenFocus = listen("tauri://focus", () => { isWindowVisibleRef.current = true; });
    getCurrentWindow().isVisible().then(v => { isWindowVisibleRef.current = v; });

    return () => {
      unlistenBlur.then(f => f());
      unlistenFocus.then(f => f());
    };
  }, []);

  return isWindowVisibleRef;
};
