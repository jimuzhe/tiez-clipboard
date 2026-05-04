import { memo } from "react";
import { THEME_CATEGORY_LABELS, type ThemeCategory } from "../types";
import type { Locale } from "../../../shared/types";

interface ThemeCategoryFilterProps {
  selected: string;
  language: Locale;
  onSelect: (category: string) => void;
}

const ThemeCategoryFilter = ({
  selected,
  language,
  onSelect,
}: ThemeCategoryFilterProps) => {
  const allCats: string[] = [
    "",
    "nature",
    "minimal",
    "retro",
    "dark",
    "cute",
    "other",
  ];
  return (
    <>
      {allCats.map((cat) => (
        <button
          key={cat}
          type="button"
          className={`theme-cat-chip${selected === cat ? " active" : ""}`}
          onClick={() => onSelect(cat)}
        >
          {cat
            ? THEME_CATEGORY_LABELS[cat as ThemeCategory]?.[language] || cat
            : language === "zh"
              ? "全部"
              : language === "tw"
                ? "全部"
                : "All"}
        </button>
      ))}
    </>
  );
};

export default memo(ThemeCategoryFilter);
