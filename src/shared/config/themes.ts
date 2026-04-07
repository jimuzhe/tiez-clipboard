import type { Locale } from "../types";

export interface ThemeDefinition {
  id: string;
  labels: Record<Locale, string>;
  supportsCustomBackground?: boolean;
  supportsSurfaceOpacity?: boolean;
}

export const THEMES: ThemeDefinition[] = [
  {
    id: "retro",
    labels: {
      zh: "3D复古",
      en: "Retro 3D",
      tw: "3D復古"
    }
  },
  {
    id: "sticky-note",
    labels: {
      zh: "便利贴",
      en: "Sticky Note",
      tw: "便利貼"
    }
  },
  {
    id: "mica",
    labels: {
      zh: "云母",
      en: "Mica",
      tw: "雲母"
    },
    supportsCustomBackground: true,
    supportsSurfaceOpacity: true
  },
  {
    id: "acrylic",
    labels: {
      zh: "毛玻璃",
      en: "Acrylic",
      tw: "毛玻璃"
    },
    supportsCustomBackground: true,
    supportsSurfaceOpacity: true
  },
  {
    id: "paper",
    labels: {
      zh: "纸质书感",
      en: "Paper & Quill",
      tw: "紙質書感"
    }
  }
];

export const DEFAULT_THEME = "mica";

const THEME_BY_ID = new Map(THEMES.map((theme) => [theme.id, theme]));

export const THEME_CLASS_NAMES = THEMES.map((theme) => `theme-${theme.id}`);

export const getThemeDefinition = (themeId: string): ThemeDefinition =>
  THEME_BY_ID.get(themeId) ?? THEME_BY_ID.get(DEFAULT_THEME)!;

export const normalizeThemeId = (themeId: string): string => getThemeDefinition(themeId).id;

export const getThemeLabel = (themeId: string, locale: Locale): string =>
  getThemeDefinition(themeId).labels[locale];

export const supportsCustomBackground = (themeId: string): boolean =>
  Boolean(getThemeDefinition(themeId).supportsCustomBackground);

export const supportsSurfaceOpacity = (themeId: string): boolean =>
  Boolean(getThemeDefinition(themeId).supportsSurfaceOpacity);

export const clearThemeClasses = (...targets: Array<Element | null | undefined>) => {
  for (const target of targets) {
    if (!target) continue;
    target.classList.remove(...THEME_CLASS_NAMES);
  }
};

export const applyThemeClasses = (
  themeId: string,
  ...targets: Array<Element | null | undefined>
) => {
  const normalizedTheme = normalizeThemeId(themeId);
  clearThemeClasses(...targets);
  for (const target of targets) {
    if (!target) continue;
    target.classList.add(`theme-${normalizedTheme}`);
  }
};
