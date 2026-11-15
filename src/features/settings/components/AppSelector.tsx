import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Search } from "lucide-react";
import type { InstalledAppOption } from "../../app/types";
import { isMacPlatform } from "../../../shared/lib/platform";
import { getSourceAppIcon, peekSourceAppIcon } from "../../../shared/lib/sourceAppIcon";

interface AppSelectorProps {
    type: string | null;
    installedApps: InstalledAppOption[];
    onSelect: (val: string) => void;
    theme: string;
    t: (key: string) => string;
    colorMode: string;
}

interface FlattenedAppItem {
    group: "recommended" | "all";
    app: InstalledAppOption;
}

const normalizeAppValue = (value: string) => value.trim().replace(/\//g, "\\").toLowerCase();

// Loads once the row scrolls into view: the list holds every installed app.
const AppOptionIcon = ({ path }: { path: string }) => {
    const [icon, setIcon] = useState(() => peekSourceAppIcon(path));
    const ref = useRef<HTMLSpanElement | null>(null);

    useEffect(() => {
        if (icon !== undefined) return;
        let cancelled = false;
        const load = () => {
            void getSourceAppIcon(path).then((value) => {
                if (!cancelled) setIcon(value);
            });
        };
        const node = ref.current;
        if (!node || typeof IntersectionObserver === "undefined") {
            load();
            return () => {
                cancelled = true;
            };
        }
        const observer = new IntersectionObserver((entries) => {
            if (!entries.some((entry) => entry.isIntersecting)) return;
            observer.disconnect();
            load();
        });
        observer.observe(node);
        return () => {
            cancelled = true;
            observer.disconnect();
        };
    }, [path, icon]);

    return (
        <span ref={ref} className="app-selector-option-icon" aria-hidden="true">
            {icon ? <img src={icon} alt="" /> : null}
        </span>
    );
};

const AppSelector = ({
    type,
    installedApps,
    onSelect,
    t
}: AppSelectorProps) => {
    const [recommended, setRecommended] = useState<InstalledAppOption[]>([]);
    const [loading, setLoading] = useState(false);
    const [query, setQuery] = useState("");
    const [selectedIndex, setSelectedIndex] = useState(0);
    const inputRef = useRef<HTMLInputElement | null>(null);
    const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
    const listboxId = useId();
    const isMac = useMemo(() => isMacPlatform(), []);

    useEffect(() => {
        let cancelled = false;

        if (!type) {
            setRecommended([]);
            setLoading(false);
            return () => {
                cancelled = true;
            };
        }

        const fetchRecommended = async () => {
            setLoading(true);
            try {
                let extension = "";
                let keywords: string[] = [];

                switch (type) {
                    case "image":
                        extension = ".png";
                        keywords = ["photo", "paint", "image", "adobe", "picture", "snip", "viewer", "画图", "照片", "看图"];
                        break;
                    case "text":
                    case "code":
                        extension = ".txt";
                        keywords = ["text", "note", "code", "edit", "write", "office", "word", "记事本", "文档"];
                        break;
                    case "html":
                    case "link":
                    case "url":
                        extension = ".html";
                        keywords = ["browser", "chrome", "edge", "firefox", "web", "internet"];
                        break;
                    case "rtf":
                        extension = ".rtf";
                        keywords = ["word", "office", "write"];
                        break;
                    case "rich_text":
                        extension = ".html";
                        keywords = ["word", "office", "write", "notes", "browser", "chrome", "edge", "firefox", "wps"];
                        break;
                    case "file":
                        extension = ".txt";
                        break;
                    default:
                        extension = "";
                }

                let associatedApps: InstalledAppOption[] = [];
                if (extension) {
                    try {
                        const associated = await invoke<{ name: string; path: string }[]>(
                            "get_associated_apps",
                            { extension, contentType: type }
                        );
                        associatedApps = associated.map((app) => ({
                            label: app.name,
                            value: app.path
                        }));
                    } catch {
                        // Recommendations are optional; the complete installed-app list remains usable.
                    }
                }

                const associatedValues = new Set(
                    associatedApps.map((app) => normalizeAppValue(app.value))
                );
                // macOS reports the apps that open the type; guessing from
                // names only fills in when it reports none.
                const guessByName = !isMac || associatedApps.length === 0;
                const localMatches = !guessByName ? [] : installedApps.filter((app) => {
                    const label = app.label.toLowerCase();
                    return keywords.some((keyword) => label.includes(keyword))
                        && !associatedValues.has(normalizeAppValue(app.value));
                });

                const seen = new Set<string>();
                const merged = [...associatedApps, ...localMatches].filter((app) => {
                    const key = normalizeAppValue(app.value);
                    if (!key || seen.has(key)) return false;
                    seen.add(key);
                    return true;
                });

                if (!cancelled) {
                    setRecommended(merged);
                }
            } finally {
                if (!cancelled) {
                    setLoading(false);
                }
            }
        };

        void fetchRecommended();
        return () => {
            cancelled = true;
        };
    }, [type, installedApps, isMac]);

    useEffect(() => {
        setQuery("");
        setSelectedIndex(0);
    }, [type]);

    const otherApps = useMemo(() => {
        const recommendedValues = new Set(
            recommended.map((app) => normalizeAppValue(app.value))
        );
        let others = installedApps.filter(
            (app) => !recommendedValues.has(normalizeAppValue(app.value))
        );

        if (type) {
            others = others.filter((app) => {
                const name = app.label.toLowerCase();
                if (type === "image") {
                    const blocked = ["music", "player", "sound", "video", "audio", "code", "terminal", "powershell", "cmd"];
                    return !blocked.some((keyword) => name.includes(keyword));
                }
                if (type === "audio" || type === "video") {
                    const blocked = ["photo", "image", "paint", "text", "note", "code", "word", "excel"];
                    return !blocked.some((keyword) => name.includes(keyword));
                }
                return true;
            });
        }

        return others;
    }, [installedApps, recommended, type]);

    const normalizedQuery = query.trim().toLowerCase();
    const filteredRecommended = useMemo(
        () => recommended.filter((app) => (
            !normalizedQuery
            || app.label.toLowerCase().includes(normalizedQuery)
            || app.value.toLowerCase().includes(normalizedQuery)
        )),
        [recommended, normalizedQuery]
    );
    const filteredOtherApps = useMemo(
        () => otherApps.filter((app) => (
            !normalizedQuery
            || app.label.toLowerCase().includes(normalizedQuery)
            || app.value.toLowerCase().includes(normalizedQuery)
        )),
        [otherApps, normalizedQuery]
    );

    const flattenedItems = useMemo<FlattenedAppItem[]>(
        () => [
            ...filteredRecommended.map((app) => ({ group: "recommended" as const, app })),
            ...filteredOtherApps.map((app) => ({ group: "all" as const, app }))
        ],
        [filteredRecommended, filteredOtherApps]
    );

    useEffect(() => {
        itemRefs.current.length = flattenedItems.length;
        if (flattenedItems.length === 0) {
            setSelectedIndex(0);
        } else {
            setSelectedIndex((current) => Math.min(current, flattenedItems.length - 1));
        }
    }, [flattenedItems]);

    useEffect(() => {
        itemRefs.current[selectedIndex]?.scrollIntoView({ block: "nearest" });
    }, [selectedIndex]);

    const handleSelect = (app: InstalledAppOption) => {
        onSelect(app.value);
    };

    const handleInputKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
        if (event.key === "ArrowDown") {
            event.preventDefault();
            setSelectedIndex((current) => (
                flattenedItems.length > 0
                    ? Math.min(current + 1, flattenedItems.length - 1)
                    : 0
            ));
            return;
        }
        if (event.key === "ArrowUp") {
            event.preventDefault();
            setSelectedIndex((current) => Math.max(current - 1, 0));
            return;
        }
        if (event.key === "Home" && flattenedItems.length > 0) {
            event.preventDefault();
            setSelectedIndex(0);
            return;
        }
        if (event.key === "End" && flattenedItems.length > 0) {
            event.preventDefault();
            setSelectedIndex(flattenedItems.length - 1);
            return;
        }
        if (event.key === "Enter") {
            const selected = flattenedItems[selectedIndex];
            if (selected) {
                event.preventDefault();
                handleSelect(selected.app);
            }
        }
    };

    const renderSection = (
        label: string,
        items: InstalledAppOption[],
        group: "recommended" | "all",
        startIndex: number
    ) => {
        if (items.length === 0) return null;

        return (
            <div className="app-selector-section" role="presentation">
                <div className="app-selector-section-label" role="presentation">
                    {label}
                </div>
                {items.map((app, offset) => {
                    const index = startIndex + offset;
                    const selected = index === selectedIndex;
                    const optionId = `${listboxId}-${group}-${index}`;
                    return (
                        <button
                            id={optionId}
                            key={`${group}-${normalizeAppValue(app.value)}`}
                            ref={(node) => {
                                itemRefs.current[index] = node;
                            }}
                            type="button"
                            role="option"
                            aria-selected={selected}
                            tabIndex={-1}
                            title={app.value}
                            className={`app-selector-option${isMac ? " has-icon" : ""}${selected ? " is-selected" : ""}`}
                            onMouseEnter={() => setSelectedIndex(index)}
                            onMouseDown={(event) => event.preventDefault()}
                            onClick={() => handleSelect(app)}
                        >
                            {isMac && <AppOptionIcon path={app.value} />}
                            {app.label}
                        </button>
                    );
                })}
            </div>
        );
    };

    const activeOption = flattenedItems[selectedIndex];
    const activeOptionId = activeOption
        ? `${listboxId}-${activeOption.group}-${selectedIndex}`
        : undefined;

    return (
        <div className="app-selector">
            <div className="app-selector-search">
                <Search size={15} className="app-selector-search-icon" aria-hidden />
                <input
                    ref={inputRef}
                    autoFocus
                    value={query}
                    role="combobox"
                    aria-autocomplete="list"
                    aria-expanded="true"
                    aria-controls={listboxId}
                    aria-activedescendant={activeOptionId}
                    aria-label={t("search_apps_placeholder")}
                    onFocus={() => {
                        invoke("focus_clipboard_window").catch(() => undefined);
                    }}
                    onChange={(event) => {
                        setQuery(event.target.value);
                        setSelectedIndex(0);
                    }}
                    onKeyDown={handleInputKeyDown}
                    placeholder={loading ? t("searching_apps") : t("search_apps_placeholder")}
                    spellCheck={false}
                />
            </div>

            <div id={listboxId} className="app-selector-list" role="listbox">
                {loading ? (
                    <div className="app-selector-status" role="status">
                        {t("searching_apps")}
                    </div>
                ) : flattenedItems.length === 0 ? (
                    <div className="app-selector-status" role="status">
                        {t("no_matching_apps")}
                    </div>
                ) : (
                    <>
                        {renderSection(t("system_recommended"), filteredRecommended, "recommended", 0)}
                        {renderSection(t("all_apps"), filteredOtherApps, "all", filteredRecommended.length)}
                    </>
                )}
            </div>
        </div>
    );
};

export default AppSelector;
