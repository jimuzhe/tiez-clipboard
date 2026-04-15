import { ChevronDown, ChevronRight } from "lucide-react";
import type { DefaultAppsMap, InstalledAppOption } from "../../../app/types";

interface DefaultAppsSettingsGroupProps {
    t: (key: string) => string;
    collapsed: boolean;
    onToggle: () => void;
    installedApps: InstalledAppOption[];
    appSettings: Record<string, string>;
    defaultApps: DefaultAppsMap;
    setShowAppSelector: (val: string | null) => void;
}

const DefaultAppsSettingsGroup = ({
    t,
    collapsed,
    onToggle,
    installedApps,
    appSettings,
    defaultApps,
    setShowAppSelector
}: DefaultAppsSettingsGroupProps) => {
    const APP_TYPES = ['text', 'rich_text', 'image', 'video', 'code', 'url'] as const;
    const getTypeName = (type: string) => {
        switch (type) {
            case "code": return t('type_code');
            case "link":
            case "url": return t('type_url');
            case "file": return t('type_file');
            case "image": return t('type_image');
            case "video": return t('type_video');
            case "rich_text": return t('type_rich_text');
            default: return t('type_text') || 'Text';
        }
    };

    return (
        <div className={`settings-group ${collapsed ? 'collapsed' : ''}`}>
            <div className="group-header" onClick={onToggle}>
                <h3 style={{ margin: 0 }}>{t('default_apps')}</h3>
                {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
            </div>
            {!collapsed && (
                <div className="group-content">
                    {APP_TYPES.map((type, idx, arr) => (
                        <div key={type} className={`setting-item column ${idx === arr.length - 1 ? 'no-border' : ''}`}>
                            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '8px' }}>
                                <span className="item-label" style={{ textTransform: 'uppercase', fontSize: '11px', opacity: 0.8 }}>{getTypeName(type)}</span>
                                <button
                                    className="btn-icon"
                                    onClick={() => setShowAppSelector(type)}
                                    title={t('change_app')}
                                    style={{ width: 'auto', padding: '4px 12px', fontSize: '10px', textTransform: 'uppercase', height: '24px' }}
                                >
                                    {t('change_app')}
                                </button>
                            </div>

                            <div onClick={() => setShowAppSelector(type)} className="data-panel" style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                                {appSettings[`app.${type}`]
                                    ? (() => {
                                        const path = appSettings[`app.${type}`];
                                        // Try to find friendly name from installed apps list
                                        const found = installedApps.find(app => app.value === path);
                                        if (found) return found.label;
                                        // Fallback: extract filename without .exe
                                        const filename = path.split(/[/\\]/).pop() || path;
                                        return filename.replace(/\.exe$/i, '');
                                    })()
                                    : (defaultApps[type] ? `${t('system_default')} (${defaultApps[type].replace(/\.exe$/i, '')})` : t('not_configured'))}
                            </div>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
};

export default DefaultAppsSettingsGroup;
