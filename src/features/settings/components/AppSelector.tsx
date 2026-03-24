import { useState, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import Select from "react-select";
import type { SingleValue } from "react-select";
import type { InstalledAppOption } from "../../app/types";

const AppSelector = ({ type, installedApps, onSelect, t }: { type: string | null, installedApps: InstalledAppOption[], onSelect: (val: string) => void, t: (key: string) => string }) => {
    const [recommended, setRecommended] = useState<InstalledAppOption[]>([]);
    const [loading, setLoading] = useState(false);

    useEffect(() => {
        if (!type) {
            setRecommended([]);
            return;
        }

        const fetchRecommended = async () => {
            setLoading(true);
            try {
                let ext = "";
                let keywords: string[] = [];

                switch (type) {
                    case "image":
                        ext = ".png";
                        keywords = ["photo", "paint", "image", "adobe", "picture", "snip", "viewer", "画图", "照片", "看图"];
                        break;
                    case "text": case "code":
                        ext = ".txt";
                        keywords = ["text", "note", "code", "edit", "write", "office", "word", "记事本", "文档"];
                        break;
                    case "html": case "link": case "url":
                        ext = ".html";
                        keywords = ["browser", "chrome", "edge", "firefox", "web", "internet"];
                        break;
                    case "rtf":
                        ext = ".rtf";
                        keywords = ["word", "office", "write"];
                        break;
                    case "file":
                        ext = ".txt";
                        break;
                    default: ext = "";
                }

                let recApps: InstalledAppOption[] = [];

                // 1. Fetch from System Registry (Backend)
                if (ext) {
                    try {
                        const rec = await invoke<{ name: string; path: string }[]>("get_associated_apps", { extension: ext });
                        recApps = rec.map((app) => ({ label: app.name, value: app.path }));
                    } catch (e) {
                        // Silent fail for feature recommendations
                    }
                }

                // 2. Client-side Keyword Match (Augmentation)
                // Find installed apps that match keywords but aren't in registry list
                const localMatches = installedApps.filter(app => {
                    const lower = app.label.toLowerCase();
                    const isMatch = keywords.some(k => lower.includes(k));
                    // Avoid duplicates
                    const alreadyIn = recApps.some(r => r.value === app.value);
                    return isMatch && !alreadyIn;
                });

                // Merge: Registry first, then Keywords
                setRecommended([...recApps, ...localMatches]);

            } catch (e) {
                // Silent fail for feature recommendations
            } finally {
                setLoading(false);
            }
        };

        fetchRecommended();
    }, [type, installedApps]);

    // Filter "Other Apps"
    const otherApps = useMemo(() => {
        // 1. Remove if present in recommended
        let others = installedApps.filter(app => !recommended.some(r => r.value === app.value));

        // 2. Apply Soft Filter (Blacklist) to clean up noise
        if (type) {
            const n_type = type;
            others = others.filter(app => {
                const name = app.label.toLowerCase();
                if (n_type === 'image') {
                    const block = ["music", "player", "sound", "video", "audio", "code", "terminal", "powershell", "cmd"];
                    if (block.some(k => name.includes(k))) return false;
                }
                else if (n_type === 'audio' || n_type === 'video') {
                    const block = ["photo", "image", "paint", "text", "note", "code", "word", "excel"];
                    if (block.some(k => name.includes(k))) return false;
                }
                return true;
            });
        }
        return others;
    }, [installedApps, recommended, type]);

    const options = [
        { label: t('system_recommended'), options: recommended },
        { label: t('all_apps'), options: otherApps }
    ];

    return (
        <Select
            options={options}
            isLoading={loading}
            onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
            placeholder={loading ? t('searching_apps') : t('search_apps_placeholder')}
            menuPortalTarget={document.body}
            menuPosition="fixed"
            onChange={(option: SingleValue<InstalledAppOption>) => {
                if (option) onSelect(option.value);
            }}
            styles={{
                control: (base) => ({
                    ...base,
                    background: 'var(--select-control-bg)',
                    border: 'var(--select-control-border)',
                    borderRadius: 'var(--select-control-radius)',
                    boxShadow: 'var(--select-control-shadow)',
                    minHeight: '32px',
                }),
                menuPortal: (base) => ({
                    ...base,
                    zIndex: 99999,
                }),
                menu: (base) => ({
                    ...base,
                    background: 'var(--select-menu-bg)',
                    borderRadius: 'var(--select-control-radius)',
                    border: 'var(--select-menu-border)',
                    marginTop: '4px',
                    zIndex: 99999,
                    boxShadow: 'var(--select-menu-shadow)',
                    maxHeight: '300px',
                }),
                menuList: (base) => ({
                    ...base,
                    maxHeight: '280px',
                    overflowY: 'auto',
                }),
                option: (base, state) => ({
                    ...base,
                    background: state.isSelected
                        ? 'var(--select-option-selected-bg)'
                        : state.isFocused
                            ? 'var(--select-option-focus-bg)'
                            : 'transparent',
                    color: state.isSelected
                        ? 'var(--select-option-selected-color)'
                        : state.isFocused
                            ? 'var(--select-option-focus-color)'
                            : 'var(--select-option-color)',
                    cursor: 'pointer',
                    fontFamily: 'inherit',
                    fontSize: '12px'
                }),
                groupHeading: (base) => ({
                    ...base,
                    color: 'var(--select-group-heading-color)',
                    fontWeight: 'bold',
                    fontSize: '11px',
                    textTransform: 'uppercase',
                    borderBottom: 'var(--select-group-heading-border)',
                    marginBottom: '4px'
                }),
                placeholder: (base) => ({ ...base, fontSize: '12px', color: 'var(--select-placeholder-color)' }),
                input: (base) => ({ ...base, color: 'var(--select-input-color)' }),
                singleValue: (base) => ({ ...base, color: 'var(--select-single-value-color)' })
            }}
        />
    );
};

export default AppSelector;
