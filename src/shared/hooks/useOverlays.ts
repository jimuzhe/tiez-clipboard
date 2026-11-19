import { useCallback, useRef, useState } from "react";
import type { ConfirmDialogState, ToastItem } from "../types";
import { formatError } from "../lib/errorMessage";

const emptyConfirm: ConfirmDialogState = {
  show: false,
  title: "",
  message: "",
  onConfirm: () => {}
};

let nextToastId = 1;

export const useOverlays = (t?: (key: string) => string) => {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const [confirmDialog, setConfirmDialog] = useState<ConfirmDialogState>(emptyConfirm);
  const tRef = useRef(t);
  tRef.current = t;

  // Every toast goes through formatError, so raw backend errors, HTML error
  // pages or multi-KB strings are translated, stripped and shortened here.
  const pushToast = useCallback((msg: string, duration = 3000) => {
    const id = nextToastId++;
    setToasts((prev) => [...prev, { id, msg: formatError(msg, tRef.current) }]);
    if (duration > 0) {
      window.setTimeout(() => {
        setToasts((prev) => prev.filter((toast) => toast.id !== id));
      }, duration);
    }
    return id;
  }, []);

  const openConfirm = useCallback(
    (opts: { title: string; message: string; danger?: boolean; confirmLabel?: string; onConfirm: () => void }) => {
      setConfirmDialog({
        show: true,
        title: opts.title,
        message: opts.message,
        danger: opts.danger,
        confirmLabel: opts.confirmLabel,
        onConfirm: opts.onConfirm
      });
    },
    []
  );

  const closeConfirm = useCallback(() => {
    setConfirmDialog((prev) => ({ ...prev, show: false }));
  }, []);

  return {
    toasts,
    pushToast,
    confirmDialog,
    openConfirm,
    closeConfirm
  };
};
