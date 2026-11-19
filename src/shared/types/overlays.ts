export type ToastItem = {
  id: number;
  msg: string;
};

export type ConfirmDialogState = {
  show: boolean;
  title: string;
  message: string;
  danger?: boolean;
  /** Names the action ("Clear", "Reset"); a plain "Confirm" if absent. */
  confirmLabel?: string;
  onConfirm: () => void;
};
