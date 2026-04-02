import { useEffect, useMemo, useRef, useState } from "react";
import type { ComponentType, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ChevronDown, ChevronRight, Plus, Trash2 } from "lucide-react";
import type { InstalledAppOption } from "../../../app/types";
import type { AppCleanupPolicy } from "../../types";
import { getSourceAppIcon, peekSourceAppIcon } from "../../../../shared/lib/sourceAppIcon";

interface LabelWithHintProps {
    label: string;
    hint?: string | ReactNode;
    hintKey: string;
}

interface AdvancedSettingsGroupProps {
    t: (key: string) => string;
    LabelWithHint: ComponentType<LabelWithHintProps>;
    cleanupRules: string;
    setCleanupRules: (val: string) => void;
    appCleanupPolicies: AppCleanupPolicy[];
    setAppCleanupPolicies: (val: AppCleanupPolicy[]) => void;
    installedApps: InstalledAppOption[];
}

interface EditableRule {
    match: string;
    replace: string;
}

interface SourceTarget {
    id: string;
    kind: "global" | "app";
    label: string;
    appPath?: string;
    policyId?: string;
    ruleCount: number;
    rawRules: string;
}

const DEFAULT_POLICY_CONTENT_TYPES = ["text", "code", "url", "rich_text"];

const createPolicyId = () =>
    `policy_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;

const parseRules = (rawRules: string): EditableRule[] =>
    rawRules
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter((line) => line.length > 0 && !line.startsWith("#"))
        .map((line) => {
            const [matchPart, replacePart = ""] = line.split(/=>/, 2);
            return {
                match: matchPart.trim(),
                replace: replacePart.trim()
            };
        });

const serializeRules = (rules: EditableRule[]): string =>
    rules
        .filter((rule) => rule.match.trim().length > 0)
        .map((rule) => `${rule.match.trim()} => ${rule.replace}`)
        .join("\n");

const focusEditorWindow = () => {
    getCurrentWindow()
        .setFocus()
        .catch(() => invoke("focus_clipboard_window").catch(console.error));
};

const AdvancedSettingsGroup = ({
    t,
    LabelWithHint,
    cleanupRules,
    setCleanupRules,
    appCleanupPolicies,
    setAppCleanupPolicies,
    installedApps
}: AdvancedSettingsGroupProps) => {
    const [searchText, setSearchText] = useState("");
    const [selectedSourceId, setSelectedSourceId] = useState("global");
    const [expandedRuleIndex, setExpandedRuleIndex] = useState<number | null>(0);
    const [draftRules, setDraftRules] = useState<EditableRule[]>(parseRules(cleanupRules));
    const [sidebarWidth, setSidebarWidth] = useState(308);
    const [isResizing, setIsResizing] = useState(false);
    const [appIcons, setAppIcons] = useState<Record<string, string | null>>({});
    const workbenchRef = useRef<HTMLElement | null>(null);

    const configuredAppPolicies = useMemo(
        () => appCleanupPolicies.filter((policy) => policy.action !== "ignore"),
        [appCleanupPolicies]
    );

    const sourceTargets = useMemo(() => {
        const targets: SourceTarget[] = [
            {
                id: "global",
                kind: "global",
                label: t("advanced_target_global"),
                ruleCount: parseRules(cleanupRules).length,
                rawRules: cleanupRules
            }
        ];

        configuredAppPolicies.forEach((policy) => {
            const appPath = policy.appPath.trim();
            const appLabel = policy.appName.trim()
                || installedApps.find((app) => app.value === appPath)?.label
                || t("advanced_target_unknown_app");
            const rawRules = policy.cleanupRules ?? "";
            targets.push({
                id: appPath ? `app:${appPath}` : `legacy:${policy.id}`,
                kind: "app",
                label: appLabel,
                appPath,
                policyId: policy.id,
                ruleCount: parseRules(rawRules).length,
                rawRules
            });
        });

        return targets;
    }, [cleanupRules, configuredAppPolicies, installedApps, t]);

    const filteredTargets = useMemo(
        () => sourceTargets,
        [sourceTargets]
    );

    const searchResults = useMemo(() => {
        const keyword = searchText.trim().toLowerCase();
        if (!keyword) {
            return [];
        }

        const existingPaths = new Set(
            sourceTargets
                .filter((target) => target.kind === "app" && target.appPath)
                .map((target) => target.appPath as string)
        );

        return installedApps
            .filter((app) => app.label.toLowerCase().includes(keyword))
            .map((app) => ({
                ...app,
                added: existingPaths.has(app.value)
            }))
            .slice(0, 8);
    }, [installedApps, searchText, sourceTargets]);

    const selectedTarget = useMemo(
        () => sourceTargets.find((target) => target.id === selectedSourceId) ?? sourceTargets[0],
        [selectedSourceId, sourceTargets]
    );

    useEffect(() => {
        if (!selectedTarget) return;
        const nextRules = parseRules(selectedTarget.rawRules);
        setDraftRules(nextRules);
        setExpandedRuleIndex(nextRules.length > 0 ? 0 : null);
    }, [selectedTarget?.id]);

    useEffect(() => {
        const paths = new Set<string>();
        sourceTargets.forEach((target) => {
            if (target.appPath) paths.add(target.appPath);
        });
        searchResults.forEach((app) => {
            if (app.value) paths.add(app.value);
        });

        paths.forEach((path) => {
            const cached = peekSourceAppIcon(path);
            if (cached !== undefined) {
                setAppIcons((prev) => (prev[path] === cached ? prev : { ...prev, [path]: cached ?? null }));
                return;
            }

            getSourceAppIcon(path).then((icon) => {
                setAppIcons((prev) => (prev[path] === icon ? prev : { ...prev, [path]: icon }));
            });
        });
    }, [searchResults, sourceTargets]);

    useEffect(() => {
        if (!isResizing) return;

        const handleMouseMove = (event: MouseEvent) => {
            const bounds = workbenchRef.current?.getBoundingClientRect();
            if (!bounds) return;
            const nextWidth = Math.min(Math.max(event.clientX - bounds.left, 220), 420);
            setSidebarWidth(nextWidth);
        };

        const handleMouseUp = () => {
            setIsResizing(false);
            document.body.style.cursor = "";
            document.body.style.userSelect = "";
        };

        document.body.style.cursor = "col-resize";
        document.body.style.userSelect = "none";
        window.addEventListener("mousemove", handleMouseMove);
        window.addEventListener("mouseup", handleMouseUp);

        return () => {
            window.removeEventListener("mousemove", handleMouseMove);
            window.removeEventListener("mouseup", handleMouseUp);
            document.body.style.cursor = "";
            document.body.style.userSelect = "";
        };
    }, [isResizing]);

    const persistAppPolicies = (nextPolicies: AppCleanupPolicy[]) => {
        setAppCleanupPolicies(nextPolicies);
        invoke("set_app_cleanup_policies", { policies: JSON.stringify(nextPolicies) }).catch(console.error);
    };

    const persistRulesForTarget = (target: SourceTarget, nextRules: EditableRule[]) => {
        const serialized = serializeRules(nextRules);

        if (target.kind === "global") {
            setCleanupRules(serialized);
            invoke("set_cleanup_rules", { rules: serialized }).catch(console.error);
            return;
        }

        const appPath = target.appPath ?? "";
        const existingIndex = appCleanupPolicies.findIndex((policy) => (
            target.policyId ? policy.id === target.policyId : policy.appPath === appPath
        ));
        const nextPolicies = [...appCleanupPolicies];

        const nextPolicy: AppCleanupPolicy = {
            id: existingIndex >= 0 ? nextPolicies[existingIndex].id : (target.policyId ?? createPolicyId()),
            enabled: existingIndex >= 0 ? nextPolicies[existingIndex].enabled : true,
            appName: target.label,
            appPath,
            action: "clean",
            contentTypes: existingIndex >= 0
                ? nextPolicies[existingIndex].contentTypes
                : [...DEFAULT_POLICY_CONTENT_TYPES],
            cleanupRules: serialized
        };

        if (existingIndex >= 0) {
            nextPolicies[existingIndex] = nextPolicy;
        } else {
            nextPolicies.push(nextPolicy);
        }

        persistAppPolicies(nextPolicies);
    };

    const updateRule = (ruleIndex: number, patch: Partial<EditableRule>) => {
        if (!selectedTarget) return;
        const nextRules = draftRules.map((rule, index) => (
            index === ruleIndex ? { ...rule, ...patch } : rule
        ));
        setDraftRules(nextRules);
        persistRulesForTarget(selectedTarget, nextRules);
    };

    const addRule = () => {
        if (!selectedTarget) return;
        const nextRules = [...draftRules, { match: "", replace: "" }];
        setDraftRules(nextRules);
        persistRulesForTarget(selectedTarget, nextRules);
        setExpandedRuleIndex(nextRules.length - 1);
    };

    const deleteRule = (ruleIndex: number) => {
        if (!selectedTarget) return;
        const nextRules = draftRules.filter((_, index) => index !== ruleIndex);
        setDraftRules(nextRules);
        persistRulesForTarget(selectedTarget, nextRules);
        if (expandedRuleIndex === ruleIndex) {
            setExpandedRuleIndex(nextRules.length > 0 ? Math.max(0, ruleIndex - 1) : null);
        }
    };

    const handleAddApp = (app: InstalledAppOption) => {
        const existing = sourceTargets.find((target) => target.kind === "app" && target.appPath === app.value);
        if (existing) {
            setSelectedSourceId(existing.id);
            setSearchText("");
            return;
        }

        const nextPolicy: AppCleanupPolicy = {
            id: createPolicyId(),
            enabled: true,
            appName: app.label,
            appPath: app.value,
            action: "clean",
            contentTypes: [...DEFAULT_POLICY_CONTENT_TYPES],
            cleanupRules: ""
        };
        persistAppPolicies([...appCleanupPolicies, nextPolicy]);
        setSelectedSourceId(`app:${app.value}`);
        setSearchText("");
    };

    return (
        <div className="settings-subpage advanced-settings-page">
            <div className="settings-subpage-header advanced-settings-header">
                <h3 style={{ margin: 0 }}>{t("advanced_settings")}</h3>
                <div className="settings-subpage-note">{t("advanced_settings_entry_desc")}</div>
            </div>

            <section
                ref={workbenchRef}
                className="advanced-workbench"
                style={{ ["--advanced-sidebar-width" as string]: `${sidebarWidth}px` }}
            >
                <aside className="advanced-sidebar">
                    <div className="advanced-sidebar-search">
                        <input
                            className="search-input advanced-search-input"
                            placeholder={t("search_apps_placeholder")}
                            value={searchText}
                            onFocus={focusEditorWindow}
                            onChange={(e) => setSearchText(e.target.value)}
                        />
                        {searchResults.length > 0 && (
                            <div className="advanced-search-results">
                                {searchResults.map((app) => (
                                    <button
                                        key={app.value}
                                        type="button"
                                        className="advanced-search-result-item"
                                        onClick={() => handleAddApp(app)}
                                    >
                                        <span className="advanced-search-result-main">
                                            {appIcons[app.value] ? (
                                                <img
                                                    src={appIcons[app.value] ?? ""}
                                                    alt={`${app.label} icon`}
                                                    className="advanced-app-icon"
                                                />
                                            ) : (
                                                <span className="advanced-target-icon advanced-target-icon-fallback">
                                                    {app.label[0] ?? "?"}
                                                </span>
                                            )}
                                            <span className="advanced-search-result-name">{app.label}</span>
                                        </span>
                                        <span className="advanced-search-result-action">
                                            {app.added ? t("advanced_open_added_app") : t("advanced_add_app")}
                                        </span>
                                    </button>
                                ))}
                            </div>
                        )}
                    </div>

                    <div className="advanced-target-list">
                        {filteredTargets.map((target) => (
                            <button
                                key={target.id}
                                type="button"
                                className={`advanced-target-item ${selectedTarget?.id === target.id ? "active" : ""}`}
                                onClick={() => setSelectedSourceId(target.id)}
                            >
                                {target.kind === "app" && target.appPath && appIcons[target.appPath] ? (
                                    <img
                                        src={appIcons[target.appPath] ?? ""}
                                        alt={`${target.label} icon`}
                                        className="advanced-app-icon advanced-target-app-icon"
                                    />
                                ) : (
                                    <span className={`advanced-target-icon ${target.kind === "global" ? "global" : ""}`}>
                                        {target.kind === "global" ? "ALL" : (target.label[0] ?? "?")}
                                    </span>
                                )}
                                <span className="advanced-target-meta">
                                    <span className="advanced-target-name">{target.label}</span>
                                    <span className="advanced-target-sub">
                                        {target.ruleCount > 0
                                            ? `${target.ruleCount} ${t("advanced_rule_count_suffix")}`
                                            : t("advanced_no_rules")}
                                    </span>
                                </span>
                            </button>
                        ))}
                    </div>
                </aside>

                <div
                    className={`advanced-divider ${isResizing ? "active" : ""}`}
                    onMouseDown={() => setIsResizing(true)}
                >
                    <span className="advanced-divider-handle" />
                </div>

                <div className="advanced-editor">
                    <div className="advanced-editor-toolbar">
                        <div>
                            <div className="advanced-editor-title">
                                {selectedTarget?.label ?? t("advanced_target_global")}
                            </div>
                            <div className="advanced-editor-subtitle">
                                {selectedTarget?.kind === "global"
                                    ? t("advanced_global_rules_hint")
                                    : t("advanced_app_rules_hint")}
                            </div>
                        </div>

                        <button type="button" className="btn-icon advanced-add-rule-btn" onClick={addRule}>
                            <Plus size={14} />
                            <span>{t("advanced_add_rule")}</span>
                        </button>
                    </div>

                    <div className="advanced-rule-list">
                        {draftRules.length === 0 && (
                            <div className="advanced-empty-state">
                                <div className="advanced-empty-title">{t("advanced_empty_rules_title")}</div>
                                <div className="advanced-empty-text">
                                    {selectedTarget?.kind === "global"
                                        ? t("advanced_empty_rules_global")
                                        : t("advanced_empty_rules_app")}
                                </div>
                            </div>
                        )}

                        {draftRules.map((rule, index) => {
                            const expanded = expandedRuleIndex === index;
                            return (
                                <div key={`${selectedTarget?.id ?? "target"}-${index}`} className="advanced-rule-card">
                                    <button
                                        type="button"
                                        className="advanced-rule-header"
                                        onClick={() => setExpandedRuleIndex(expanded ? null : index)}
                                    >
                                        <span className="advanced-rule-title">
                                            {t("advanced_rule_label")} {index + 1}
                                        </span>
                                        {expanded ? <ChevronDown size={18} /> : <ChevronRight size={18} />}
                                    </button>

                                    {expanded && (
                                        <div className="advanced-rule-body">
                                            <div className="advanced-rule-field">
                                                <label>{t("advanced_match_label")}</label>
                                                <textarea
                                                    className="search-input advanced-rule-textarea"
                                                    value={rule.match}
                                                    placeholder={t("advanced_match_placeholder")}
                                                    onFocus={focusEditorWindow}
                                                    onChange={(e) => updateRule(index, { match: e.target.value })}
                                                />
                                            </div>

                                            <div className="advanced-rule-field">
                                                <label>{t("advanced_replace_label")}</label>
                                                <textarea
                                                    className="search-input advanced-rule-textarea"
                                                    value={rule.replace}
                                                    placeholder={t("advanced_replace_placeholder")}
                                                    onFocus={focusEditorWindow}
                                                    onChange={(e) => updateRule(index, { replace: e.target.value })}
                                                />
                                            </div>

                                            <div className="advanced-rule-actions">
                                                <button
                                                    type="button"
                                                    className="btn-icon advanced-delete-btn"
                                                    onClick={() => deleteRule(index)}
                                                >
                                                    <Trash2 size={14} />
                                                    <span>{t("delete")}</span>
                                                </button>
                                            </div>
                                        </div>
                                    )}
                                </div>
                            );
                        })}
                    </div>
                </div>
            </section>
        </div>
    );
};

export default AdvancedSettingsGroup;
