import ColorSwatchSelector from "../ColorSwatchSelector";
import { NATIVE_PALETTES, getCustomPaletteColor, normalizeCustomColor, normalizeNativePalette } from "../../../../shared/config/nativePalettes";
import { useEffect, useState, type ComponentType, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask, open, message } from "@tauri-apps/plugin-dialog";
import { withNativeDialog } from "../../../../shared/lib/focus";
import { ChevronDown, ChevronRight, X } from "lucide-react";
import {
    getDefaultTheme,
    getSelectableThemes,
    getThemeLabel,
    normalizeThemeId,
    supportsCustomBackground,
    supportsNativePalette,
    supportsSurfaceOpacity,
    MACOS_THEME_ID
} from "../../../../shared/config/themes";
import { isWindowsPlatform } from "../../../../shared/lib/platform";
import type { Locale } from "../../../../shared/types";
import {
    getInstalledThemeLabel,
    listInstalledThemes,
    removeInstalledThemeStyle,
    type InstalledThemeDefinition
} from "../../../../shared/lib/themePackages";
import { formatError } from "../../../../shared/lib/errorMessage";
import { SettingRange, SettingSwitch } from "../SettingControls";

interface LabelWithHintProps {
    label: string;
    hint?: string | ReactNode;
    hintKey: string;
}

interface AppearanceSettingsGroupProps {
    t: (key: string) => string;
    collapsed: boolean;
    onToggle: () => void;
    LabelWithHint: ComponentType<LabelWithHintProps>;
    theme: string;
    setTheme: (val: string) => void;
    nativePalette: string;
    nativeCustomColor?: string;
    setNativePalette: (val: string) => void;
    colorMode: string;
    setColorMode: (val: string) => void;
    language: Locale;
    setLanguage: (val: Locale) => void;
    showAppBorder: boolean;
    setShowAppBorder: (val: boolean) => void;
    showSourceAppIcon: boolean;
    setShowSourceAppIcon: (val: boolean) => void;
    hideUnselectedItemActions: boolean;
    setHideUnselectedItemActions: (val: boolean) => void;

    compactMode: boolean;
    setCompactMode: (val: boolean) => void;
    compactShowTime: boolean;
    setCompactShowTime: (val: boolean) => void;
    splitViewEnabled: boolean;
    setSplitViewEnabled: (val: boolean) => void;
    clipboardItemFontSize: number;
    setClipboardItemFontSize: (val: number) => void;
    clipboardTagFontSize: number;
    setClipboardTagFontSize: (val: number) => void;
    customBackground: string;
    setCustomBackground: (val: string) => void;
    customBackgroundOpacity: number;
    setCustomBackgroundOpacity: (val: number) => void;
    surfaceOpacity: number;
    setSurfaceOpacity: (val: number) => void;
    saveAppSetting: (key: string, val: string) => void;
}

const AppearanceSettingsGroup = ({
    t,
    collapsed,
    onToggle,
    LabelWithHint,
    theme,
    setTheme,
    nativePalette,
    nativeCustomColor,
    setNativePalette,
    colorMode,
    setColorMode,
    language,
    setLanguage,
    showAppBorder,
    setShowAppBorder,
    showSourceAppIcon,
    setShowSourceAppIcon,
    hideUnselectedItemActions,
    setHideUnselectedItemActions,

    compactMode,
    setCompactMode,
    compactShowTime,
    setCompactShowTime,
    splitViewEnabled,
    setSplitViewEnabled,
    clipboardItemFontSize,
    setClipboardItemFontSize,
    clipboardTagFontSize,
    setClipboardTagFontSize,
    customBackground,
    setCustomBackground,
    customBackgroundOpacity,
    setCustomBackgroundOpacity,
    surfaceOpacity,
    setSurfaceOpacity,
    saveAppSetting
}: AppearanceSettingsGroupProps) => {
    const [customColor, setCustomColor] = useState(() => getCustomPaletteColor(nativePalette)
        || normalizeCustomColor(nativeCustomColor) || "#8655bb");
    useEffect(() => {
        setCustomColor(getCustomPaletteColor(nativePalette) || normalizeCustomColor(nativeCustomColor) || "#8655bb");
    }, [nativePalette, nativeCustomColor]);
    const choosePalette = (value: string) => {
        setNativePalette(value);
        saveAppSetting('native_palette', value);
    };
    const [installedThemes, setInstalledThemes] = useState<InstalledThemeDefinition[]>([]);
    const selectedInstalledTheme = installedThemes.find(item => item.id === theme);
    const showCustomBackgroundControls = selectedInstalledTheme
        ? selectedInstalledTheme.supportsCustomBackground
        : supportsCustomBackground(theme);
    // The 系统 theme has no see-through window material: there the setting only
    // decides how much of a background image shows, so it needs one to act on.
    // A 配色 other than the default does read it (native-palette.css), so it
    // stays reachable there even without an image.
    const usesNativePalette = normalizeNativePalette(nativePalette) !== "default";
    const showSurfaceOpacityControls = selectedInstalledTheme
        ? selectedInstalledTheme.supportsSurfaceOpacity
        : supportsSurfaceOpacity(theme) &&
          (theme !== MACOS_THEME_ID || Boolean(customBackground) || usesNativePalette);

    useEffect(() => {
        let disposed = false;
        listInstalledThemes()
            .then(items => {
                if (!disposed) setInstalledThemes(items);
            })
            .catch(error => console.error("Failed to list installed themes:", error));
        return () => {
            disposed = true;
        };
    }, []);

    const uninstallTheme = async (item: InstalledThemeDefinition) => {
        try {
            const confirmed = await withNativeDialog(async () => ask(
                t('theme_package_confirm_delete'),
                { title: t('theme_package_uninstall'), kind: 'warning' }
            ), "settings:uninstall-theme");
            if (!confirmed) return;

            await invoke('uninstall_theme_package', { themeId: item.id });
            removeInstalledThemeStyle(item.id);
            setInstalledThemes(current => current.filter(themeItem => themeItem.id !== item.id));
            if (theme === item.id) {
                const fallbackTheme = getDefaultTheme();
                setTheme(fallbackTheme);
                saveAppSetting('theme', fallbackTheme);
            }
        } catch (error) {
            console.error("Failed to uninstall theme package:", error);
            await message(`${t('theme_package_uninstall_failed')}\n${formatError(error, t)}`, {
                title: t('error'),
                kind: 'error'
            });
        }
    };

    const chooseBackground = async () => {
        try {
            const selected = await withNativeDialog(async () => {
                const path = await open({
                    multiple: false,
                    title: t('choose_background'),
                    filters: [{
                        name: 'Image',
                        extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif']
                    }]
                });
                if (!path || typeof path !== 'string') return null;

                try {
                    const stats = await invoke<{ size: number }>('get_file_size', { path });
                    const maxSize = 10 * 1024 * 1024;
                    if (stats.size > maxSize) {
                        await message(
                            t('background_size_error'),
                            { title: t('error'), kind: 'error' }
                        );
                        return null;
                    }
                } catch (e) { console.warn(e); }

                return path;
            }, "settings:choose-background");
            if (selected && typeof selected === 'string') {
                setCustomBackground(selected);
                saveAppSetting('custom_background', selected);
            }
        } catch (err) {
            console.error(err);
        }
    };

    return (
    <div className={`settings-group ${collapsed ? 'collapsed' : ''}`}>
        <button type="button" className="group-header" aria-expanded={!collapsed} onClick={onToggle}>
            <h3>{t('appearance_settings')}</h3>
            {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
        </button>
        {!collapsed && (
            <div className="group-content">
                <h4 className="settings-section-title">{t('settings_section_theme')}</h4>
                <div className="setting-item column">
                    <div className="item-label-group">
                        <span className="item-label">{t('visual_theme')}</span>
                    </div>
                    <div className="settings-choice-grid theme-choice-grid">
                        {getSelectableThemes().map(themeItem => (
                            <button
                                key={themeItem.id}
                                onClick={() => {
                                    setTheme(themeItem.id);
                                    saveAppSetting('theme', themeItem.id);
                                }}
                                className={`btn-icon theme-choice-btn ${normalizeThemeId(theme) === themeItem.id ? 'active' : ''}`}
                                type="button"
                            >
                                <span className="theme-choice-title">
                                    {getThemeLabel(themeItem.id, t)}
                                </span>
                            </button>
                        ))}
                    </div>
                    {installedThemes.length > 0 && (
                        <>
                            <div className="item-label-group installed-theme-label">
                                <span className="item-label">{t('installed_themes')}</span>
                            </div>
                            <div className="settings-choice-grid theme-choice-grid">
                                {installedThemes.map(themeItem => (
                                    <div className="installed-theme-choice" key={themeItem.id}>
                                        <button
                                            onClick={() => {
                                                setTheme(themeItem.id);
                                                saveAppSetting('theme', themeItem.id);
                                            }}
                                            className={`btn-icon theme-choice-btn ${theme === themeItem.id ? 'active' : ''}`}
                                            type="button"
                                        >
                                            <span className="theme-choice-title">
                                                {getInstalledThemeLabel(themeItem, language)}
                                            </span>
                                        </button>
                                        <button
                                            type="button"
                                            className="btn-icon installed-theme-remove"
                                            aria-label={t('delete')}
                                            title={t('delete')}
                                            onClick={() => void uninstallTheme(themeItem)}
                                        >
                                            <X size={14} />
                                        </button>
                                    </div>
                                ))}
                            </div>
                        </>
                    )}
                </div>

                {supportsNativePalette(theme) && (
                    <div className="setting-item">
                        <div className="item-label-group">
                            <span className="item-label">{t('native_palette')}</span>
                        </div>
                        <ColorSwatchSelector
                            label={t('native_palette')}
                            value={nativePalette}
                            options={NATIVE_PALETTES.map(palette => ({ ...palette, label: t(palette.labelKey) }))}
                            customLabel={t('native_palette_custom')}
                            t={t}
                            customColor={customColor}
                            onChange={choosePalette}
                            onCustomColorChange={color => {
                                setCustomColor(color);
                                saveAppSetting('native_custom_color', color);
                                choosePalette(`custom:${color}`);
                            }}
                        />
                    </div>
                )}

                <div className="setting-item">
                    <div className="item-label-group">
                        <span className="item-label">{t('color_mode')}</span>
                    </div>
                    <div className="settings-inline-choice-row" role="group" aria-label={t('color_mode')}>
                        {[
                            { id: 'system', name: t('mode_system') },
                            { id: 'light', name: t('mode_light') },
                            { id: 'dark', name: t('mode_dark') }
                        ].map(modeItem => (
                            <button
                                key={modeItem.id}
                                type="button"
                                aria-pressed={colorMode === modeItem.id}
                                onClick={() => {
                                    console.log('[THEME DEBUG] Saving color_mode:', modeItem.id);
                                    setColorMode(modeItem.id);
                                    saveAppSetting('color_mode', modeItem.id);
                                }}
                                className={`btn-icon settings-inline-choice-btn ${colorMode === modeItem.id ? 'active' : ''}`}
                            >
                                {modeItem.name}
                            </button>
                        ))}
                    </div>
                </div>

                <div className="setting-item">
                    <div className="item-label-group">
                        <span className="item-label">{t('language')}</span>
                    </div>
                    <div className="settings-inline-choice-row" role="group" aria-label={t('language')}>
                        {[
                            { id: 'zh', name: '简体' },
                            { id: 'tw', name: '繁體' },
                            { id: 'en', name: 'English' }
                        ].map(lang => (
                            <button
                                key={lang.id}
                                type="button"
                                aria-pressed={language === lang.id}
                                onClick={() => {
                                    setLanguage(lang.id as Locale);
                                    saveAppSetting('language', lang.id);
                                }}
                                className={`btn-icon settings-inline-choice-btn ${language === lang.id ? 'active' : ''}`}
                            >
                                {lang.name}
                            </button>
                        ))}
                    </div>
                </div>

                <h4 className="settings-section-title">{t('settings_section_list')}</h4>
                {isWindowsPlatform() && (
                    <div className="setting-item">
                        <LabelWithHint
                            label={t('show_app_border')}
                            hint={t('show_app_border_hint')}
                            hintKey="show_app_border"
                        />
                        <SettingSwitch
                            checked={showAppBorder}
                            onChange={(e) => {
                                const val = e.target.checked;
                                setShowAppBorder(val);
                                saveAppSetting('show_app_border', String(val));
                            }}
                        />
                    </div>
                )}

                <div className="setting-item">
                    <LabelWithHint
                        label={t('show_source_app_icon')}
                        hint={t('show_source_app_icon_hint')}
                        hintKey="show_source_app_icon"
                    />
                    <SettingSwitch
                        checked={showSourceAppIcon}
                        onChange={(e) => {
                            const val = e.target.checked;
                            setShowSourceAppIcon(val);
                            saveAppSetting('show_source_app_icon', String(val));
                        }}
                    />
                </div>

                <div className="setting-item">
                    <LabelWithHint
                        label={t('hide_unselected_item_actions')}
                        hint={t('hide_unselected_item_actions_hint')}
                        hintKey="hide_unselected_item_actions"
                    />
                    <SettingSwitch
                        checked={hideUnselectedItemActions}
                        onChange={(e) => {
                            const val = e.target.checked;
                            setHideUnselectedItemActions(val);
                            saveAppSetting('hide_unselected_item_actions', String(val));
                        }}
                    />
                </div>

                <div className="setting-item">
                    <LabelWithHint
                        label={t('compact_mode')}
                        hint={t('compact_mode_hint')}
                        hintKey="compact_mode"
                    />
                    <SettingSwitch
                        checked={compactMode}
                        onChange={(e) => {
                            const val = e.target.checked;
                            setCompactMode(val);
                            saveAppSetting('compact_mode', String(val));
                        }}
                    />
                </div>

                {compactMode && (
                    <div className="setting-item is-child">
                        <div className="item-label-group">
                            <span className="item-label">{t('compact_show_time')}</span>
                        </div>
                        <SettingSwitch
                            checked={compactShowTime}
                            onChange={(e) => {
                                const val = e.target.checked;
                                setCompactShowTime(val);
                                saveAppSetting('compact_show_time', String(val));
                            }}
                        />
                    </div>
                )}

                <div className="setting-item">
                    <LabelWithHint
                        label={t('split_view')}
                        hint={t('split_view_hint')}
                        hintKey="split_view"
                    />
                    <SettingSwitch
                        aria-label={t('split_view')}
                        checked={splitViewEnabled}
                        onChange={(e) => {
                            const val = e.target.checked;
                            setSplitViewEnabled(val);
                            saveAppSetting('split_view', String(val));
                        }}
                    />
                </div>

                <div className="setting-item">
                    <LabelWithHint
                        label={t('clipboard_item_font_size')}
                        hint={t('clipboard_item_font_size_hint')}
                        hintKey="clipboard_item_font_size"
                    />
                    <SettingRange
                        aria-label={t('clipboard_item_font_size')}
                        min={11}
                        max={18}
                        step={1}
                        value={clipboardItemFontSize}
                        valueLabel={`${clipboardItemFontSize}px`}
                        onChange={(e) => {
                            const val = parseInt(e.target.value);
                            setClipboardItemFontSize(val);
                            saveAppSetting('clipboard_item_font_size', String(val));
                        }}
                    />
                </div>

                <div className="setting-item">
                    <LabelWithHint
                        label={t('clipboard_tag_font_size')}
                        hint={t('clipboard_tag_font_size_hint')}
                        hintKey="clipboard_tag_font_size"
                    />
                    <SettingRange
                        aria-label={t('clipboard_tag_font_size')}
                        min={8}
                        max={14}
                        step={1}
                        value={clipboardTagFontSize}
                        valueLabel={`${clipboardTagFontSize}px`}
                        onChange={(e) => {
                            const val = parseInt(e.target.value);
                            setClipboardTagFontSize(val);
                            saveAppSetting('clipboard_tag_font_size', String(val));
                        }}
                    />
                </div>

                {(showCustomBackgroundControls || showSurfaceOpacityControls) && (
                    <>
                        <h4 className="settings-section-title">{t('settings_section_background')}</h4>
                        {showCustomBackgroundControls && (
                            <div className="setting-item">
                                <div className="item-label-group">
                                    <span className="item-label">{t('custom_background')}</span>
                                    {customBackground && (
                                        <span className="item-desc settings-file-name">
                                            {customBackground.split(/[/\\]/).pop()}
                                        </span>
                                    )}
                                </div>
                                <div className="settings-action-group">
                                    <button
                                        type="button"
                                        onClick={() => void chooseBackground()}
                                        className="settings-action-button settings-action-button--small"
                                    >
                                        {customBackground ? t('change_background') : t('choose_background')}
                                    </button>
                                    {customBackground && (
                                        <button
                                            type="button"
                                            onClick={() => {
                                                setCustomBackground('');
                                                saveAppSetting('custom_background', '');
                                            }}
                                            className="settings-action-button settings-action-button--small settings-action-button--icon"
                                            title={t('clear_background')}
                                            aria-label={t('clear_background')}
                                        >
                                            <X size={14} />
                                        </button>
                                    )}
                                </div>
                            </div>
                        )}

                        {showCustomBackgroundControls && customBackground && (
                            <div className="setting-item is-child">
                                <div className="item-label-group">
                                    <span className="item-label">{t('background_opacity')}</span>
                                </div>
                                <SettingRange
                                    aria-label={t('background_opacity')}
                                    min={0}
                                    max={100}
                                    value={customBackgroundOpacity}
                                    valueLabel={`${customBackgroundOpacity}%`}
                                    onChange={(e) => {
                                        const val = parseInt(e.target.value);
                                        setCustomBackgroundOpacity(val);
                                        saveAppSetting('custom_background_opacity', String(val));
                                    }}
                                />
                            </div>
                        )}

                        {showSurfaceOpacityControls && (
                            <div className="setting-item">
                                <div className="item-label-group">
                                    <span className="item-label">{t('surface_opacity')}</span>
                                </div>
                                <SettingRange
                                    aria-label={t('surface_opacity')}
                                    min={0}
                                    max={100}
                                    value={surfaceOpacity}
                                    valueLabel={`${surfaceOpacity}%`}
                                    onChange={(e) => {
                                        const val = parseInt(e.target.value);
                                        setSurfaceOpacity(val);
                                        saveAppSetting('surface_opacity', String(val));
                                    }}
                                />
                            </div>
                        )}
                    </>
                )}
            </div>
        )}
    </div>
    );
};

export default AppearanceSettingsGroup;
