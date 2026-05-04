import { memo, useCallback, useState } from "react";
import { Heart } from "lucide-react";
import type { StoreTheme } from "../types";
import type { Locale } from "../../../shared/types";

interface ThemeCardProps {
  theme: StoreTheme;
  language: Locale;
  isActive: boolean;
  variant?: "featured" | "popular";
  onApply: (theme: StoreTheme) => void;
  onDetail: (theme: StoreTheme) => void;
}

const ThemeCard = ({
  theme,
  language,
  isActive,
  variant = "featured",
  onApply,
  onDetail,
}: ThemeCardProps) => {
  const [imgError, setImgError] = useState(false);
  const name =
    theme.name[language] || theme.name.en || theme.name.zh || theme.id;

  const handleApply = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      onApply(theme);
    },
    [theme, onApply]
  );

  const applyLabel =
    isActive
      ? language === "zh" || language === "tw"
        ? "已应用"
        : "Applied"
      : language === "zh" || language === "tw"
        ? "应用"
        : "Apply";

  if (variant === "popular") {
    return (
      <div className="theme-card-popular" onClick={() => onDetail(theme)}>
        {!imgError ? (
          <img
            className="theme-card-popular-thumb"
            src={theme.previewLightUrl}
            alt={name}
            loading="lazy"
            onError={() => setImgError(true)}
          />
        ) : (
          <div className="theme-card-popular-thumb theme-card-popular-thumb-fallback">
            🎨
          </div>
        )}
        <div className="theme-card-popular-info">
          <div className="theme-card-popular-name" title={name}>
            {name}
          </div>
          <div className="theme-card-popular-author">{theme.author}</div>
        </div>
        <div className="theme-card-popular-likes">
          <Heart size={12} />
          <span>{theme.downloadCount}</span>
        </div>
        <button
          type="button"
          className={`theme-card-popular-apply${isActive ? " active" : ""}`}
          onClick={handleApply}
        >
          {applyLabel}
        </button>
      </div>
    );
  }

  return (
    <div className="theme-card-featured" onClick={() => onDetail(theme)}>
      {!imgError ? (
        <img
          className="theme-card-featured-preview"
          src={theme.previewLightUrl}
          alt={name}
          loading="lazy"
          onError={() => setImgError(true)}
        />
      ) : (
        <div className="theme-card-featured-preview theme-card-featured-preview-fallback">
          🎨
        </div>
      )}
      <div className="theme-card-featured-overlay">
        <div className="theme-card-featured-info">
          <div className="theme-card-featured-name" title={name}>
            {name}
          </div>
          <div className="theme-card-featured-author">{theme.author}</div>
        </div>
        <div className="theme-card-featured-actions">
          <span className="theme-card-featured-likes">
            <Heart size={12} />
            <span>{theme.downloadCount}</span>
          </span>
          <button
            type="button"
            className={`theme-card-featured-apply${isActive ? " active" : ""}`}
            onClick={handleApply}
          >
            {applyLabel}
          </button>
        </div>
      </div>
    </div>
  );
};

export default memo(ThemeCard);
