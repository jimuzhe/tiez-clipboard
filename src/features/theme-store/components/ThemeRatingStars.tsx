import { memo } from "react";

interface ThemeRatingStarsProps {
  score: number;
  max?: number;
  readonly?: boolean;
  onRate?: (score: number) => void;
}

const ThemeRatingStars = ({
  score,
  max = 5,
  readonly = true,
  onRate,
}: ThemeRatingStarsProps) => {
  return (
    <span className="theme-rating-stars">
      {Array.from({ length: max }, (_, i) => (
        <button
          key={i}
          type="button"
          className={`theme-rating-star${i < score ? " filled" : ""}${readonly ? " readonly" : ""}`}
          onClick={readonly ? undefined : () => onRate?.(i + 1)}
          title={readonly ? undefined : `${i + 1}`}
        >
          ★
        </button>
      ))}
    </span>
  );
};

export default memo(ThemeRatingStars);
