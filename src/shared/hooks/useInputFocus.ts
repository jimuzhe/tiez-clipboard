import { useEffect, useRef, type MutableRefObject } from "react";
import { focusClipboardWindow, restoreLastFocus } from "../lib/focus";

type FocusState = "normal" | "clipboard";

type UseInputFocusOptions = {
  enableDelay?: number;
  restoreDelay?: number;
};

const isEditableElement = (el: Element | null) => {
  if (!el) return false;
  const tagName = el.tagName;
  return (
    tagName === "INPUT" ||
    tagName === "TEXTAREA" ||
    (el as HTMLElement).isContentEditable === true
  );
};

export function useInputFocus<T extends HTMLElement = HTMLInputElement>(
  options: UseInputFocusOptions = {}
) {
  const { enableDelay = 60, restoreDelay = 120 } = options;
  const inputRef = useRef<T | null>(null);
  const focusTimer = useRef<number | null>(null);
  const blurTimer = useRef<number | null>(null);
  const focusState = useRef<FocusState>("normal");

  const clearTimer = (timerRef: MutableRefObject<number | null>) => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  const debouncedEnableFocus = () => {
    clearTimer(focusTimer);
    focusTimer.current = window.setTimeout(async () => {
      try {
        await focusClipboardWindow();
        focusState.current = "clipboard";
      } catch {
        // Ignore focus errors
      }
    }, enableDelay);
  };

  const debouncedRestoreFocus = () => {
    if (focusState.current === "normal") {
      return;
    }

    clearTimer(blurTimer);
    blurTimer.current = window.setTimeout(async () => {
      if (isEditableElement(document.activeElement)) {
        return;
      }

      try {
        await restoreLastFocus();
        focusState.current = "normal";
      } catch {
        // Ignore restore errors
      }
    }, restoreDelay);
  };

  useEffect(() => {
    const element = inputRef.current;
    if (!element) return;

    const handleFocus = () => {
      debouncedEnableFocus();
    };

    const handleBlur = () => {
      debouncedRestoreFocus();
    };

    element.addEventListener("focus", handleFocus);
    element.addEventListener("blur", handleBlur);

    const checkInitialFocus = window.setTimeout(() => {
      if (document.activeElement === element) {
        debouncedEnableFocus();
      }
    }, 0);

    return () => {
      element.removeEventListener("focus", handleFocus);
      element.removeEventListener("blur", handleBlur);
      clearTimeout(checkInitialFocus);
      clearTimer(focusTimer);
      clearTimer(blurTimer);
    };
  }, [enableDelay, restoreDelay]);

  return inputRef;
}
