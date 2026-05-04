import { memo, useState, useCallback } from "react";
import { X } from "lucide-react";
import type { StoreTheme } from "../types";
import type { Locale } from "../../../shared/types";
import ThemeRatingStars from "./ThemeRatingStars";
import { getPreviewUrl, rateTheme, deleteTheme } from "../api";

interface ThemeDetailModalProps {
  theme: StoreTheme;
  language: Locale;
  isActive: boolean;
  isLoggedIn: boolean;
  currentUsername: string | null;
  onApply: (theme: StoreTheme) => void;
  onClose: () => void;
  onDeleted: () => void;
  t: (key: string) => string;
}

const ThemeDetailModal = ({
  theme,
  language,
  isActive,
  isLoggedIn,
  currentUsername,
  onApply,
  onClose,
  onDeleted,
  t,
}: ThemeDetailModalProps) => {
  const [previewMode, setPreviewMode] = useState<"light" | "dark">("light");
  const [userRating, setUserRating] = useState(theme.userRating || 0);
  const [avgRating, setAvgRating] = useState(theme.avgRating);
  const [ratingCount, setRatingCount] = useState(theme.ratingCount);
  const [deleting, setDeleting] = useState(false);

  const name =
    theme.name[language] || theme.name.en || theme.name.zh || theme.id;
  const desc =
    theme.description[language] ||
    theme.description.en ||
    theme.description.zh ||
    "";

  const handleRate = useCallback(
    async (score: number) => {
      if (!isLoggedIn) return;
      try {
        const result = await rateTheme(theme.id, score);
        setUserRating(score);
        setAvgRating(result.avgRating);
        setRatingCount(result.ratingCount);
      } catch {
        // ignore
      }
    },
    [theme.id, isLoggedIn]
  );

  const handleDelete = useCallback(async () => {
    if (!confirm(t("theme_store_confirm_delete"))) return;
    setDeleting(true);
    try {
      await deleteTheme(theme.id);
      onDeleted();
      onClose();
    } catch {
      setDeleting(false);
    }
  }, [theme.id, onDeleted, onClose, t]);

  const isOwner = currentUsername === theme.author;

  return (
    <div className="theme-detail-overlay" onClick={onClose}>
      <div
        className="theme-detail-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div style={{ position: "relative" }}>
          <img
            className="theme-detail-preview"
            src={getPreviewUrl(theme.id, previewMode)}
            alt={name}
          />
          <button
            type="button"
            className="theme-store-back"
            style={{ position: "absolute", top: 4, right: 4, background: "rgba(0,0,0,0.4)", color: "#fff" }}
            onClick={onClose}
          >
            <X size={14} />
          </button>
          <div className="theme-detail-preview-toggle">
            <button
              type="button"
              className={previewMode === "light" ? "active" : ""}
              onClick={() => setPreviewMode("light")}
            >
              {t("theme_store_light")}
            </button>
            <button
              type="button"
              className={previewMode === "dark" ? "active" : ""}
              onClick={() => setPreviewMode("dark")}
            >
              {t("theme_store_dark")}
            </button>
          </div>
        </div>

        <div className="theme-detail-body">
          <div className="theme-detail-name">{name}</div>
          <div className="theme-detail-author">
            {t("theme_store_by")} {theme.author} · v{theme.version}
          </div>

          {desc && <div className="theme-detail-desc">{desc}</div>}

          <div className="theme-detail-stats">
            <span>
              {t("theme_store_downloads")}: {theme.downloadCount}
            </span>
            <span>{t("theme_store_category")}: {theme.category}</span>
          </div>

          <div className="theme-detail-flags">
            {theme.supportsCustomBackground && (
              <span className="theme-detail-flag">
                {t("theme_store_custom_bg")}
              </span>
            )}
            {theme.supportsSurfaceOpacity && (
              <span className="theme-detail-flag">
                {t("theme_store_surface_opacity")}
              </span>
            )}
          </div>

          <div className="theme-detail-rating">
            <ThemeRatingStars
              score={Math.round(avgRating)}
              readonly={!isLoggedIn}
              onRate={handleRate}
            />
            <span style={{ fontSize: "10px", color: "var(--text-muted)" }}>
              {avgRating.toFixed(1)} ({ratingCount})
            </span>
            {isLoggedIn && userRating > 0 && (
              <span style={{ fontSize: "9px", color: "var(--accent-color)" }}>
                · {t("theme_store_your_rating")}: {userRating}★
              </span>
            )}
          </div>

          <div className="theme-detail-actions">
            {isOwner && (
              <button
                type="button"
                className="danger"
                onClick={handleDelete}
                disabled={deleting}
              >
                {deleting ? "..." : t("theme_store_delete")}
              </button>
            )}
            <button
              type="button"
              className="primary"
              onClick={() => onApply(theme)}
            >
              {isActive
                ? t("theme_store_current")
                : t("theme_store_apply")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default memo(ThemeDetailModal);
