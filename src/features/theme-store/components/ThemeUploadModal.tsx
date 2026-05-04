import { memo, useState, useCallback, useRef } from "react";
import { X } from "lucide-react";
import { uploadTheme } from "../api";

interface ThemeUploadModalProps {
  onClose: () => void;
  onSuccess: () => void;
  t: (key: string) => string;
}

const ThemeUploadModal = ({ onClose, onSuccess, t }: ThemeUploadModalProps) => {
  const [file, setFile] = useState<File | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const handleFileChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const f = e.target.files?.[0];
      if (f) {
        if (!f.name.endsWith(".zip")) {
          setError(t("theme_store_zip_only"));
          return;
        }
        if (f.size > 5 * 1024 * 1024) {
          setError(t("theme_store_file_too_large"));
          return;
        }
        setFile(f);
        setError("");
      }
    },
    [t]
  );

  const handleUpload = useCallback(async () => {
    if (!file) return;
    setError("");
    setLoading(true);
    try {
      await uploadTheme(file);
      onSuccess();
      onClose();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Upload failed");
    } finally {
      setLoading(false);
    }
  }, [file, onSuccess, onClose]);

  return (
    <div className="theme-upload-overlay" onClick={onClose}>
      <div
        className="theme-upload-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span className="theme-upload-title">
            {t("theme_store_upload")}
          </span>
          <button
            type="button"
            className="theme-store-back"
            onClick={onClose}
          >
            <X size={14} />
          </button>
        </div>

        <div
          className="theme-upload-drop"
          onClick={() => inputRef.current?.click()}
        >
          {file
            ? `📦 ${file.name}`
            : t("theme_store_drop_zip")}
        </div>
        <input
          ref={inputRef}
          type="file"
          accept=".zip"
          style={{ display: "none" }}
          onChange={handleFileChange}
        />

        <div className="theme-upload-info">
          {t("theme_store_upload_hint")}
        </div>

        {error && <div className="theme-upload-error">{error}</div>}

        <div className="theme-upload-actions">
          <button type="button" onClick={onClose}>
            {t("cancel")}
          </button>
          <button
            type="button"
            className="primary"
            onClick={handleUpload}
            disabled={loading || !file}
          >
            {loading ? "..." : t("theme_store_upload_btn")}
          </button>
        </div>
      </div>
    </div>
  );
};

export default memo(ThemeUploadModal);
