export interface FileTransferChatViewProps {
  t: (key: string) => string;
  localIp: string;
  actualPort: string;
}

export type FileTransferMessageDirection = "in" | "out";

export interface FileTransferMessage {
  id: number;
  direction: FileTransferMessageDirection;
  msg_type: string;
  content: string;
  timestamp: number;
  sender_id?: string;
  sender_name?: string;
  file_path?: string;
  _preparing?: boolean;
  _fileName?: string;
  _fallbackSrc?: string;
}

export type FileTransferDragPayload = string[] | { paths: string[] };

export interface FileTransferDevice {
  id?: string;
  name?: string;
  [key: string]: unknown;
}

export interface FileTransferContextMenu {
  x: number;
  y: number;
  filePath?: string;
  content?: string;
  id?: number;
  type?: string;
}
