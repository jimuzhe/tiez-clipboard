import { memo, useState, useEffect, useCallback, useMemo } from "react";
import { ChevronLeft, SlidersHorizontal } from "lucide-react";
import type { Locale } from "../../../shared/types";
import type { StoreTheme, ThemeSort } from "../types";
import { useThemeAuth } from "../hooks/useThemeAuth";
import { useThemeStore } from "../hooks/useThemeStore";
import { useThemeApply } from "../hooks/useThemeApply";
import ThemeCard from "./ThemeCard";
import ThemeDetailModal from "./ThemeDetailModal";
import ThemeAuthModal from "./ThemeAuthModal";
import ThemeUploadModal from "./ThemeUploadModal";
import ThemeCategoryFilter from "./ThemeCategoryFilter";
import "../css/ThemeStorePanel.css";

interface ThemeStorePanelProps {
  t: (key: string) => string;
  theme: string;
  setTheme: (val: string) => void;
  saveAppSetting: (key: string, val: string) => void;
  language: Locale;
  onBack: () => void;
}

const FEATURED_COUNT = 4;

const SORT_OPTIONS: { value: ThemeSort; label: Record<string, string> }[] = [
  { value: "newest", label: { zh: "最新", en: "Newest", tw: "最新" } },
  { value: "popular", label: { zh: "最热", en: "Popular", tw: "最熱" } },
  { value: "top_rated", label: { zh: "好评", en: "Top Rated", tw: "好評" } },
  { value: "trending", label: { zh: "趋势", en: "Trending", tw: "趨勢" } },
];

const ThemeStorePanel = ({
  t,
  theme,
  setTheme,
  saveAppSetting,
  language,
  onBack,
}: ThemeStorePanelProps) => {
  const auth = useThemeAuth();
  const store = useThemeStore();
  const { applyStoreTheme } = useThemeApply({
    theme,
    setTheme,
    saveAppSetting,
  });

  const [detailTheme, setDetailTheme] = useState<StoreTheme | null>(null);
  const [showAuth, setShowAuth] = useState(false);
  const [showUpload, setShowUpload] = useState(false);
  const [searchValue, setSearchValue] = useState("");
  const [showFilters, setShowFilters] = useState(false);

  useEffect(() => {
    store.loadThemes();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const featuredThemes = useMemo(
    () => store.themes.slice(0, FEATURED_COUNT),
    [store.themes]
  );
  const popularThemes = useMemo(
    () => store.themes.slice(FEATURED_COUNT),
    [store.themes]
  );

  const handleApply = useCallback(
    async (targetTheme: StoreTheme) => {
      await applyStoreTheme(targetTheme.id);
    },
    [applyStoreTheme]
  );

  const handleDetail = useCallback((targetTheme: StoreTheme) => {
    setDetailTheme(targetTheme);
  }, []);

  const handleRefresh = useCallback(() => {
    store.loadThemes();
  }, [store]);

  const handleSearchKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        store.search(searchValue.trim());
      }
    },
    [searchValue, store]
  );

  return (
    <div className="theme-store-panel">
      {/* Header */}
      <div className="theme-store-header">
        <button type="button" className="theme-store-back" onClick={onBack}>
          <ChevronLeft size={16} />
        </button>
        <span className="theme-store-title">{t("theme_store")}</span>
      </div>

      {/* Search bar with filter */}
      <div className="theme-store-search-row">
        <div className="theme-store-search">
          <input
            type="text"
            value={searchValue}
            onChange={(e) => setSearchValue(e.target.value)}
            onKeyDown={handleSearchKeyDown}
            placeholder={t("theme_store_search")}
          />
        </div>
        <button
          type="button"
          className={`theme-store-filter-btn${showFilters ? " active" : ""}`}
          onClick={() => setShowFilters(!showFilters)}
        >
          <SlidersHorizontal size={14} />
        </button>
      </div>

      {/* Category filters + Sort (collapsible) */}
      {showFilters && (
        <div className="theme-store-filters">
          <ThemeCategoryFilter
            selected={store.category}
            language={language}
            onSelect={store.changeCategory}
          />
          <div className="theme-store-sort-chips">
            {SORT_OPTIONS.map((opt) => (
              <button
                key={opt.value}
                type="button"
                className={`theme-cat-chip${store.sort === opt.value ? " active" : ""}`}
                onClick={() => store.changeSort(opt.value)}
              >
                {opt.label[language] || opt.label.en}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Scrollable content */}
      <div className="theme-store-content">
        {store.loading && store.themes.length === 0 ? (
          <div className="theme-store-loading">{t("theme_store_loading")}</div>
        ) : store.themes.length === 0 ? (
          <div className="theme-store-empty">{t("theme_store_empty")}</div>
        ) : (
          <>
            {/* Creator Picks */}
            {featuredThemes.length > 0 && (
              <section className="theme-store-section">
                <div className="theme-store-section-header">
                  <span className="theme-store-section-title">
                    {language === "zh" || language === "tw"
                      ? "创作者精选"
                      : "Creator Picks"}
                  </span>
                  <button type="button" className="theme-store-see-all">
                    {language === "zh" || language === "tw"
                      ? "查看全部"
                      : "See All"}
                  </button>
                </div>
                <div className="theme-store-featured-grid">
                  {featuredThemes.map((item) => (
                    <ThemeCard
                      key={item.id}
                      theme={item}
                      language={language}
                      variant="featured"
                      isActive={theme === item.id}
                      onApply={handleApply}
                      onDetail={handleDetail}
                    />
                  ))}
                </div>
              </section>
            )}

            {/* Popular Themes */}
            {popularThemes.length > 0 && (
              <section className="theme-store-section">
                <div className="theme-store-section-header">
                  <span className="theme-store-section-title">
                    {language === "zh" || language === "tw"
                      ? "热门主题"
                      : "Popular Themes"}
                  </span>
                  <button type="button" className="theme-store-see-all">
                    {language === "zh" || language === "tw"
                      ? "查看全部"
                      : "See All"}
                  </button>
                </div>
                <div className="theme-store-popular-list">
                  {popularThemes.map((item) => (
                    <ThemeCard
                      key={item.id}
                      theme={item}
                      language={language}
                      variant="popular"
                      isActive={theme === item.id}
                      onApply={handleApply}
                      onDetail={handleDetail}
                    />
                  ))}
                </div>
              </section>
            )}
          </>
        )}

        {/* Load more */}
        {store.page < store.totalPages && (
          <div className="theme-store-load-more">
            <button
              type="button"
              onClick={store.loadMore}
              disabled={store.loading}
            >
              {store.loading ? "..." : t("theme_store_load_more")}
            </button>
          </div>
        )}
      </div>

      {/* Bottom bar */}
      <div className="theme-store-bottom">
        {auth.isLoggedIn ? (
          <>
            <button type="button" onClick={() => setShowUpload(true)}>
              {t("theme_store_upload")}
            </button>
            <button type="button" onClick={auth.logout}>
              {t("theme_store_logout")}
            </button>
          </>
        ) : (
          <button
            type="button"
            className="primary"
            onClick={() => setShowAuth(true)}
          >
            {t("theme_store_login_to_upload")}
          </button>
        )}
      </div>

      {/* Modals */}
      {detailTheme && (
        <ThemeDetailModal
          theme={detailTheme}
          language={language}
          isActive={theme === detailTheme.id}
          isLoggedIn={auth.isLoggedIn}
          currentUsername={auth.username}
          onApply={handleApply}
          onClose={() => setDetailTheme(null)}
          onDeleted={handleRefresh}
          t={t}
        />
      )}

      {showAuth && (
        <ThemeAuthModal
          onClose={() => setShowAuth(false)}
          onLogin={auth.login}
          onRegister={auth.register}
          t={t}
        />
      )}

      {showUpload && (
        <ThemeUploadModal
          onClose={() => setShowUpload(false)}
          onSuccess={handleRefresh}
          t={t}
        />
      )}
    </div>
  );
};

export default memo(ThemeStorePanel);
