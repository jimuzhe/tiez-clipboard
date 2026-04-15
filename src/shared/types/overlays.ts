export type ToastItem = {
  id: number;
  msg: string;
};

export type ConfirmDialogState = {
  show: boolean;
  title: string;
  message: string;
  onConfirm: () => void;
};
