export interface ClipboardEntry {
  id: number;
  content_type: string;
  content: string;
  html_content?: string;
  source_app: string;
  source_app_path?: string;
  timestamp: number;
  preview: string;
  is_pinned: boolean;
  tags: string[];
  isInputting?: boolean;
  questionCount?: number;
  use_count?: number;
  is_external?: boolean;
  pinned_order?: number;
  file_preview_exists?: boolean;
}
