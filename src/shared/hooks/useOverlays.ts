import { useCallback, useState } from "react";
import type { ConfirmDialogState, ToastItem } from "../types";

const emptyConfirm: ConfirmDialogState = {
  show: false,
  title: "",
  message: "",
  onConfirm: () => {}
};

export const useOverlays = () => {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const [confirmDialog, setConfirmDialog] = useState<ConfirmDialogState>(emptyConfirm);

  const pushToast = useCallback((msg: string, duration = 3000) => {
    const id = Date.now();
    setToasts((prev) => [...prev, { id, msg }]);
    if (duration > 0) {
      window.setTimeout(() => {
        setToasts((prev) => prev.filter((t) => t.id !== id));
      }, duration);
    }
    return id;
  }, []);

  const openConfirm = useCallback(
    (opts: { title: string; message: string; onConfirm: () => void }) => {
      setConfirmDialog({
        show: true,
        title: opts.title,
        message: opts.message,
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
