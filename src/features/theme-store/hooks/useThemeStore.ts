import { useState, useCallback, useRef } from "react";
import * as api from "../api";
import type { StoreTheme, ThemeListResponse, ThemeSort } from "../types";

export function useThemeStore() {
  const [themes, setThemes] = useState<StoreTheme[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [totalPages, setTotalPages] = useState(0);
  const [sort, setSort] = useState<ThemeSort>("newest");
  const [category, setCategory] = useState<string>("");
  const [searchQuery, setSearchQuery] = useState("");
  const abortRef = useRef<AbortController | null>(null);

  const loadThemes = useCallback(
    async (opts?: {
      page?: number;
      sort?: ThemeSort;
      category?: string;
      q?: string;
      append?: boolean;
    }) => {
      abortRef.current?.abort();
      abortRef.current = new AbortController();

      const p = opts?.page ?? 1;
      const s = opts?.sort ?? sort;
      const c = opts?.category ?? category;
      const q = opts?.q ?? searchQuery;

      setLoading(true);
      setError(null);
      try {
        const data: ThemeListResponse = await api.fetchThemes({
          page: p,
          limit: 20,
          sort: s,
          category: c || undefined,
          q: q || undefined,
        });
        if (opts?.append) {
          setThemes((prev) => [...prev, ...data.themes]);
        } else {
          setThemes(data.themes);
        }
        setTotal(data.total);
        setPage(data.page);
        setTotalPages(data.totalPages);
      } catch (err: unknown) {
        if (err instanceof Error && err.name === "AbortError") return;
        setError(err instanceof Error ? err.message : "Failed to load themes");
      } finally {
        setLoading(false);
      }
    },
    [sort, category, searchQuery]
  );

  const loadMore = useCallback(() => {
    if (page < totalPages && !loading) {
      loadThemes({ page: page + 1, append: true });
    }
  }, [page, totalPages, loading, loadThemes]);

  const changeSort = useCallback(
    (newSort: ThemeSort) => {
      setSort(newSort);
      loadThemes({ sort: newSort, page: 1 });
    },
    [loadThemes]
  );

  const changeCategory = useCallback(
    (newCategory: string) => {
      setCategory(newCategory);
      loadThemes({ category: newCategory, page: 1 });
    },
    [loadThemes]
  );

  const search = useCallback(
    (q: string) => {
      setSearchQuery(q);
      loadThemes({ q, page: 1 });
    },
    [loadThemes]
  );

  return {
    themes,
    loading,
    error,
    total,
    page,
    totalPages,
    sort,
    category,
    searchQuery,
    loadThemes,
    loadMore,
    changeSort,
    changeCategory,
    search,
  };
}
