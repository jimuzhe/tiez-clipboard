export interface StoreTheme {
  id: string;
  name: Record<string, string>;
  description: Record<string, string>;
  author: string;
  category: string;
  version: string;
  supportsCustomBackground: boolean;
  supportsSurfaceOpacity: boolean;
  downloadCount: number;
  avgRating: number;
  ratingCount: number;
  userRating?: number;
  previewLightUrl: string;
  previewDarkUrl: string;
  createdAt: string;
}

export interface ThemeListResponse {
  themes: StoreTheme[];
  total: number;
  page: number;
  limit: number;
  totalPages: number;
}

export type ThemeSort = "newest" | "popular" | "top_rated" | "trending";

export type ThemeCategory =
  | "nature"
  | "minimal"
  | "retro"
  | "dark"
  | "cute"
  | "other";

export const THEME_CATEGORY_LABELS: Record<
  ThemeCategory,
  Record<string, string>
> = {
  nature: { zh: "自然", en: "Nature", tw: "自然" },
  minimal: { zh: "极简", en: "Minimal", tw: "極簡" },
  retro: { zh: "复古", en: "Retro", tw: "復古" },
  dark: { zh: "暗黑", en: "Dark", tw: "暗黑" },
  cute: { zh: "可爱", en: "Cute", tw: "可愛" },
  other: { zh: "其他", en: "Other", tw: "其他" },
};
