import type { ComponentType, ReactNode, CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, message } from "@tauri-apps/plugin-dialog";
import { ChevronDown, ChevronRight, X } from "lucide-react";
import {
    THEMES,
    getThemeLabel,
    supportsCustomBackground,
    supportsSurfaceOpacity
} from "../../../../shared/config/themes";
import type { Locale } from "../../../../shared/types";

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
    colorMode: string;
    setColorMode: (val: string) => void;
    language: Locale;
    setLanguage: (val: Locale) => void;
    showAppBorder: boolean;
    setShowAppBorder: (val: boolean) => void;
    showSourceAppIcon: boolean;
    setShowSourceAppIcon: (val: boolean) => void;
    compactMode: boolean;
    setCompactMode: (val: boolean) => void;
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

const clampProgress = (value: number, min: number, max: number) => {
    if (max <= min) return "0%";
    const pct = ((value - min) / (max - min)) * 100;
    return `${Math.min(100, Math.max(0, pct))}%`;
};

const buildRangeStyle = (value: number, min: number, max: number) =>
    ({
        width: '100%',
        cursor: 'pointer',
        accentColor: 'var(--accent-color)',
        "--range-progress": clampProgress(value, min, max)
    }) as CSSProperties;

const AppearanceSettingsGroup = ({
    t,
    collapsed,
    onToggle,
    LabelWithHint,
    theme,
    setTheme,
    colorMode,
    setColorMode,
    language,
    setLanguage,
    showAppBorder,
    setShowAppBorder,
    showSourceAppIcon,
    setShowSourceAppIcon,
    compactMode,
    setCompactMode,
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
    const showCustomBackgroundControls = supportsCustomBackground(theme);
    const showSurfaceOpacityControls = supportsSurfaceOpacity(theme);

    return (
    <div className={`settings-group ${collapsed ? 'collapsed' : ''}`}>
        <div className="group-header" onClick={onToggle}>
            <h3 style={{ margin: 0 }}>{t('appearance_settings')}</h3>
            {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
        </div>
        {!collapsed && (
            <div className="group-content">
                <div className="setting-item column">
                    <div className="item-label-group" style={{ marginBottom: '8px' }}>
                        <span className="item-label">{t('visual_theme')}</span>
                    </div>
                    <div className="settings-choice-grid theme-choice-grid">
                        {THEMES.map(themeItem => (
                            <button
                                key={themeItem.id}
                                onClick={() => {
                                    setTheme(themeItem.id);
                                    saveAppSetting('theme', themeItem.id);
                                }}
                                className={`btn-icon theme-choice-btn ${theme === themeItem.id ? 'active' : ''}`}
                                type="button"
                            >
                                <span className="theme-choice-title">
                                    {getThemeLabel(themeItem.id, language)}
                                </span>
                            </button>
                        ))}
                    </div>
                </div>

                <div className="setting-item column">
                    <div className="item-label-group" style={{ marginBottom: '8px' }}>
                        <span className="item-label">{t('color_mode')}</span>
                    </div>
                    <div className="settings-inline-choice-row">
                        {[
                            { id: 'system', name: t('mode_system') },
                            { id: 'light', name: t('mode_light') },
                            { id: 'dark', name: t('mode_dark') }
                        ].map(modeItem => (
                            <button
                                key={modeItem.id}
                                onClick={() => {
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

                <div className="setting-item column no-border">
                    <div className="item-label-group" style={{ marginBottom: '8px' }}>
                        <span className="item-label">{t('language')}</span>
                    </div>
                    <div className="settings-inline-choice-row">
                        {[
                            { id: 'zh', name: '简体' },
                            { id: 'tw', name: '繁體' },
                            { id: 'en', name: 'English' }
                        ].map(lang => (
                            <button
                                key={lang.id}
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

                <div className="setting-item">
                    <LabelWithHint
                        label={t('show_app_border') || '显示应用边框'}
                        hint={t('show_app_border_hint') || '关闭后隐藏主窗口边框和阴影'}
                        hintKey="show_app_border"
                    />
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={showAppBorder}
                            onChange={(e) => {
                                const val = e.target.checked;
                                setShowAppBorder(val);
                                saveAppSetting('show_app_border', String(val));
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>

                <div className="setting-item">
                    <LabelWithHint
                        label={t('show_source_app_icon') || '显示来源应用图标'}
                        hint={t('show_source_app_icon_hint') || '关闭后不显示来源应用图标，改为显示剪贴板条目类型图标'}
                        hintKey="show_source_app_icon"
                    />
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={showSourceAppIcon}
                            onChange={(e) => {
                                const val = e.target.checked;
                                setShowSourceAppIcon(val);
                                saveAppSetting('show_source_app_icon', String(val));
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>

                <div className="setting-item">
                    <LabelWithHint
                        label={t('compact_mode') || 'Compact Mode'}
                        hint={t('compact_mode_hint') || 'When enabled, clipboard list displays more densely with more entries visible. Hover to preview.'}
                        hintKey="compact_mode"
                    />
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={compactMode}
                            onChange={(e) => {
                                const val = e.target.checked;
                                setCompactMode(val);
                                saveAppSetting('compact_mode', String(val));
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>

                <div className="setting-item column">
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: '8px' }}>
                        <LabelWithHint
                            label={t('clipboard_item_font_size') || '条目字体大小'}
                            hint={t('clipboard_item_font_size_hint') || '调整剪贴板首页条目内容的字体大小'}
                            hintKey="clipboard_item_font_size"
                        />
                        <span className="hint" style={{ fontSize: '10px', color: 'var(--text-secondary)', whiteSpace: 'nowrap' }}>
                            {clipboardItemFontSize}px
                        </span>
                    </div>
                    <input
                        type="range"
                        min="11"
                        max="18"
                        step="1"
                        value={clipboardItemFontSize}
                        onChange={(e) => {
                            const val = parseInt(e.target.value);
                            setClipboardItemFontSize(val);
                            saveAppSetting('clipboard_item_font_size', String(val));
                        }}
                        style={buildRangeStyle(clipboardItemFontSize, 11, 18)}
                    />
                </div>

                <div className="setting-item column">
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: '8px' }}>
                        <LabelWithHint
                            label={t('clipboard_tag_font_size') || '标签字体大小'}
                            hint={t('clipboard_tag_font_size_hint') || '调整剪贴板条目标签的字体大小'}
                            hintKey="clipboard_tag_font_size"
                        />
                        <span className="hint" style={{ fontSize: '10px', color: 'var(--text-secondary)', whiteSpace: 'nowrap' }}>
                            {clipboardTagFontSize}px
                        </span>
                    </div>
                    <input
                        type="range"
                        min="8"
                        max="14"
                        step="1"
                        value={clipboardTagFontSize}
                        onChange={(e) => {
                            const val = parseInt(e.target.value);
                            setClipboardTagFontSize(val);
                            saveAppSetting('clipboard_tag_font_size', String(val));
                        }}
                        style={buildRangeStyle(clipboardTagFontSize, 8, 14)}
                    />
                </div>

                {(showCustomBackgroundControls || showSurfaceOpacityControls) && (
                    <>
                        {showCustomBackgroundControls && (
                        <div className="setting-item column no-border">
                            <div className="item-label-group" style={{ marginBottom: '8px' }}>
                                <span className="item-label">{t('custom_background') || '自定义背景'}</span>
                            </div>
                            <div style={{ display: 'flex', gap: '8px', width: '100%', alignItems: 'center' }}>
                                <button
                                    onClick={async () => {
                                        try {
                                            const selected = await open({
                                                multiple: false,
                                                filters: [{
                                                    name: 'Image',
                                                    extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif']
                                                }]
                                            });
                                            if (selected && typeof selected === 'string') {
                                                try {
                                                    const stats = await invoke<{ size: number }>('get_file_size', { path: selected });
                                                    const maxSize = 10 * 1024 * 1024;
                                                    if (stats.size > maxSize) {
                                                        await message(
                                                            t('background_size_error') || `图片文件过大！请选择小于 ${Math.round(maxSize / 1024 / 1024)}MB 的图片。`,
                                                            { title: t('error') || '错误', kind: 'error' }
                                                        );
                                                        return;
                                                    }
                                                } catch (e) { console.warn(e); }
                                                setCustomBackground(selected);
                                                saveAppSetting('custom_background', selected);
                                            }
                                        } catch (err) { console.error(err); }
                                    }}
                                    className="btn-icon"
                                    style={{ flex: 1, height: '36px', fontSize: '12px', fontWeight: 'bold' }}
                                >
                                    {customBackground ? (t('change_background') || '更换背景') : (t('choose_background') || '选择背景')}
                                </button>
                                {customBackground && (
                                    <button
                                        onClick={() => {
                                            setCustomBackground('');
                                            saveAppSetting('custom_background', '');
                                        }}
                                        className="btn-icon"
                                        style={{ height: '36px', fontSize: '12px', fontWeight: 'bold', padding: '0 12px' }}
                                        title={t('clear_background') || '清除背景'}
                                    >
                                        <X size={16} />
                                    </button>
                                )}
                            </div>
                            {customBackground && (
                                <div style={{ fontSize: '11px', color: 'var(--text-secondary)', marginTop: '4px', wordBreak: 'break-all' }}>
                                    {customBackground.split(/[/\\]/).pop()}
                                </div>
                            )}

                            {customBackground && (
                                <div className="setting-item column no-border" style={{ marginTop: '12px' }}>
                                    <div className="item-label-group" style={{ marginBottom: '4px', flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}>
                                        <span className="item-label">{t('background_opacity')}</span>
                                        <span className="hint" style={{ fontSize: '10px', color: 'var(--text-secondary)' }}>{customBackgroundOpacity}%</span>
                                    </div>
                                    <input
                                        type="range"
                                        min="0"
                                        max="100"
                                        value={customBackgroundOpacity}
                                        onChange={(e) => {
                                            const val = parseInt(e.target.value);
                                            setCustomBackgroundOpacity(val);
                                            saveAppSetting('custom_background_opacity', String(val));
                                        }}
                                        style={buildRangeStyle(customBackgroundOpacity, 0, 100)}
                                    />
                                </div>
                            )}
                        </div>
                        )}

                        {showSurfaceOpacityControls && (
                        <div className="setting-item column">
                            <div className="item-label-group" style={{ marginBottom: '4px', flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}>
                                <span className="item-label">{t('surface_opacity') || '界面底板透明度'}</span>
                                <span className="hint" style={{ fontSize: '10px', color: 'var(--text-secondary)' }}>{surfaceOpacity}%</span>
                            </div>
                            <input
                                type="range"
                                min="0"
                                max="100"
                                value={surfaceOpacity}
                                onChange={(e) => {
                                    const val = parseInt(e.target.value);
                                    setSurfaceOpacity(val);
                                    saveAppSetting('surface_opacity', String(val));
                                }}
                                style={buildRangeStyle(surfaceOpacity, 0, 100)}
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
