import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ChevronDown, ChevronRight, Plus, Trash2 } from "lucide-react";
import type { InstalledAppOption } from "../../../app/types";
import type { AppCleanupPolicy } from "../../types";



interface AdvancedSettingsGroupProps {
    t: (key: string) => string;
    cleanupRules: string;
    setCleanupRules: (val: string) => void;
    appCleanupPolicies: AppCleanupPolicy[];
    setAppCleanupPolicies: (val: AppCleanupPolicy[]) => void;
    installedApps: InstalledAppOption[];
}

interface EditableRule {
    match: string;
    replace: string;
    label?: string;
    actionType?: "replace" | "ignore";
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

const DEFAULT_POLICY_CONTENT_TYPES = ["text", "code", "url", "rich_text", "image", "file", "video"];

const createPolicyId = () =>
    `policy_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;

const parseRules = (rawRules: string): EditableRule[] => {
    const lines = rawRules.split(/\r?\n/).map((l) => l.trim());
    const rules: EditableRule[] = [];
    let currentLabel: string | null = null;

    for (const line of lines) {
        if (line.length === 0) continue;
        if (line.startsWith("#")) {
            const labelMatch = line.match(/^#\s*label:\s*(.*)$/i);
            if (labelMatch) {
                currentLabel = labelMatch[1].trim();
            }
            continue;
        }

        const [matchPart, replacePart = ""] = line.split(/=>/, 2);
        const replaceValue = replacePart.trim();
        rules.push({
            match: matchPart.trim(),
            replace: replaceValue === "__IGNORE_CAPTURE__" ? "" : replaceValue,
            label: currentLabel ?? undefined,
            actionType: replaceValue === "__IGNORE_CAPTURE__" ? "ignore" : "replace"
        });
        currentLabel = null;
    }
    return rules;
};

const serializeRules = (rules: EditableRule[]): string =>
    rules
        .filter((rule) => rule.match.trim().length > 0)
        .map((rule) => {
            const lines = [];
            if (rule.label?.trim()) {
                lines.push(`# label: ${rule.label.trim()}`);
            }
            const actualReplace = rule.actionType === "ignore" ? "__IGNORE_CAPTURE__" : rule.replace;
            lines.push(`${rule.match.trim()} => ${actualReplace}`);
            return lines.join("\n");
        })
        .join("\n\n");

const focusEditorWindow = () => {
    getCurrentWindow()
        .setFocus()
        .catch(() => invoke("focus_clipboard_window").catch(console.error));
};

const AdvancedSettingsGroup = ({
    t,
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
    const [sidebarWidth, setSidebarWidth] = useState(120);
    const [sidebarHeight, setSidebarHeight] = useState(180);
    const [isResizing, setIsResizing] = useState(false);
    const [isStacked, setIsStacked] = useState(false);
    const workbenchRef = useRef<HTMLElement | null>(null);

    const configuredAppPolicies = appCleanupPolicies;

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
                ruleCount: policy.action === "ignore" ? 0 : parseRules(rawRules).length,
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

    // App icons fetching removed for minimalist style

    useEffect(() => {
        const mediaQuery = window.matchMedia("(max-width: 340px)");
        const updateLayoutMode = () => {
            setIsStacked(mediaQuery.matches);
        };

        updateLayoutMode();
        mediaQuery.addEventListener("change", updateLayoutMode);

        return () => mediaQuery.removeEventListener("change", updateLayoutMode);
    }, []);

    useEffect(() => {
        if (!isResizing) return;

        const handleMouseMove = (event: MouseEvent) => {
            const bounds = workbenchRef.current?.getBoundingClientRect();
            if (!bounds) return;
            if (isStacked) {
                const maxHeight = Math.max(140, bounds.height - 220);
                const nextHeight = Math.min(Math.max(event.clientY - bounds.top, 120), maxHeight);
                setSidebarHeight(nextHeight);
                return;
            }

            const nextWidth = Math.min(Math.max(event.clientX - bounds.left, 80), 280);
            setSidebarWidth(nextWidth);
        };

        const handleMouseUp = () => {
            setIsResizing(false);
            document.body.style.cursor = "";
            document.body.style.userSelect = "";
        };

        document.body.style.cursor = isStacked ? "row-resize" : "col-resize";
        document.body.style.userSelect = "none";
        window.addEventListener("mousemove", handleMouseMove);
        window.addEventListener("mouseup", handleMouseUp);

        return () => {
            window.removeEventListener("mousemove", handleMouseMove);
            window.removeEventListener("mouseup", handleMouseUp);
            document.body.style.cursor = "";
            document.body.style.userSelect = "";
        };
    }, [isResizing, isStacked]);

    const handleDeleteTarget = (event: React.MouseEvent, target: SourceTarget) => {
        event.stopPropagation();
        if (target.kind === "global") return;

        // Filter out the app policy. Since rules are inside the policy, they are deleted automatically.
        const nextPolicies = appCleanupPolicies.filter(p => (
            p.id !== target.policyId && (p.appPath !== target.appPath || !target.appPath)
        ));
        
        persistAppPolicies(nextPolicies);
        
        // If the deleted target was selected, switch back to global
        if (selectedSourceId === target.id) {
            setSelectedSourceId("global");
        }
    };

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

        const nextContentTypes = existingIndex >= 0
            ? Array.from(new Set([...(nextPolicies[existingIndex].contentTypes ?? []), ...DEFAULT_POLICY_CONTENT_TYPES]))
            : [...DEFAULT_POLICY_CONTENT_TYPES];

        const nextPolicy: AppCleanupPolicy = {
            id: existingIndex >= 0 ? nextPolicies[existingIndex].id : (target.policyId ?? createPolicyId()),
            enabled: existingIndex >= 0 ? nextPolicies[existingIndex].enabled : true,
            appName: target.label,
            appPath,
            action: existingIndex >= 0 ? nextPolicies[existingIndex].action : "clean",
            contentTypes: nextContentTypes,
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
        const nextRules = [...draftRules, { match: "", replace: "", actionType: "replace" as const }];
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

    const toggleTargetAction = () => {
        if (!selectedTarget || selectedTarget.kind === "global") return;

        const existingIndex = appCleanupPolicies.findIndex((policy) => (
            selectedTarget.policyId ? policy.id === selectedTarget.policyId : policy.appPath === (selectedTarget.appPath ?? "")
        ));
        
        if (existingIndex < 0) return;

        const nextPolicies = [...appCleanupPolicies];
        const nextAction = nextPolicies[existingIndex].action === "ignore" ? "clean" : "ignore";
        nextPolicies[existingIndex] = {
            ...nextPolicies[existingIndex],
            action: nextAction,
            contentTypes: Array.from(new Set([...(nextPolicies[existingIndex].contentTypes ?? []), ...DEFAULT_POLICY_CONTENT_TYPES]))
        };

        persistAppPolicies(nextPolicies);
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
            {/* Header is handled by AppHeader */}

            <section
                ref={workbenchRef}
                className={`advanced-workbench ${isStacked ? "stacked-layout" : ""}`}
                style={{
                    ["--advanced-sidebar-width" as string]: `${sidebarWidth}px`,
                    ["--advanced-sidebar-height" as string]: `${sidebarHeight}px`
                }}
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
                                <span className="advanced-target-meta">
                                    <span className="advanced-target-name">{target.label}</span>
                                    <span className="advanced-target-sub">
                                        {target.kind === "app" && appCleanupPolicies.find(p => p.id === target.policyId || p.appPath === target.appPath)?.action === "ignore"
                                            ? t("app_cleanup_policy_ignore")
                                            : target.ruleCount > 0
                                                ? `${target.ruleCount} ${t("advanced_rule_count_suffix")}`
                                                : t("advanced_no_rules")}
                                    </span>
                                </span>

                                {target.kind !== "global" && (
                                    <button
                                        type="button"
                                        className="advanced-target-delete"
                                        onClick={(e) => handleDeleteTarget(e, target)}
                                        title={t("delete")}
                                    >
                                        <Trash2 size={14} />
                                    </button>
                                )}
                            </button>
                        ))}
                    </div>
                </aside>

                <div
                    className={`advanced-divider ${isResizing ? "active" : ""} ${isStacked ? "stacked" : ""}`}
                    onMouseDown={() => setIsResizing(true)}
                >
                    <span className="advanced-divider-handle" />
                </div>

                <div className="advanced-editor">
                    <div className="advanced-editor-toolbar">
                        {selectedTarget?.kind === "global" || (appCleanupPolicies.find(p => p.id === selectedTarget.policyId || p.appPath === selectedTarget.appPath)?.action !== "ignore") ? (
                            <button type="button" className="btn-icon advanced-add-rule-btn" onClick={addRule}>
                                <Plus size={14} />
                                <span>{t("advanced_add_rule")}</span>
                            </button>
                        ) : null}
                    </div>

                    {selectedTarget?.kind === "app" && (
                        <div className="advanced-action-toggle">
                            <div className="advanced-action-info">
                                <span className="advanced-action-label">记录此应用的内容？</span>
                                <span className="advanced-action-hint">
                                    {appCleanupPolicies.find(p => p.id === selectedTarget.policyId || p.appPath === selectedTarget.appPath)?.action === "ignore"
                                        ? "当前已关闭记录，来自该应用的剪贴板内容将被忽略"
                                        : "当前已开启记录，您可以点击上方按钮添加清洗或拦截规则"
                                    }
                                </span>
                            </div>
                            <button 
                                type="button" 
                                className={`advanced-action-switch ${appCleanupPolicies.find(p => p.id === selectedTarget.policyId || p.appPath === selectedTarget.appPath)?.action !== "ignore" ? "active" : ""}`}
                                onClick={toggleTargetAction}
                            >
                                <span className="advanced-action-switch-track">
                                    <span className="advanced-action-switch-thumb" />
                                </span>
                                <span className="advanced-action-switch-label">
                                    {appCleanupPolicies.find(p => p.id === selectedTarget.policyId || p.appPath === selectedTarget.appPath)?.action !== "ignore"
                                        ? "开启"
                                        : "关闭"
                                    }
                                </span>
                            </button>
                        </div>
                    )}

                    <div className="advanced-rule-list">
                        {selectedTarget?.kind === "app" && appCleanupPolicies.find(p => p.id === selectedTarget.policyId || p.appPath === selectedTarget.appPath)?.action === "ignore" ? (
                            <div className="advanced-empty-state ignore-mode">
                                <div className="advanced-empty-title">已停止记录</div>
                                <div className="advanced-empty-text">
                                    当前应用已被设置为忽略，记录功能已关闭。
                                </div>
                            </div>
                        ) : (
                            <>
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
                                                    {rule.label?.trim() || `${t("advanced_rule_label")} ${index + 1}`}
                                                </span>
                                                {expanded ? <ChevronDown size={18} /> : <ChevronRight size={18} />}
                                            </button>

                                            {expanded && (
                                                <div className="advanced-rule-body">
                                                    <div className="advanced-rule-field">
                                                        <label>{t("advanced_rule_label_name")}</label>
                                                        <input
                                                            type="text"
                                                            className="search-input advanced-rule-input"
                                                            value={rule.label ?? ""}
                                                            placeholder={`${t("advanced_rule_label")} ${index + 1}`}
                                                            onFocus={focusEditorWindow}
                                                            onChange={(e) => updateRule(index, { label: e.target.value })}
                                                        />
                                                    </div>
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
                                                        <label>命中后动作</label>
                                                        <div className="advanced-rule-action-tabs">
                                                            <button 
                                                                type="button" 
                                                                className={`advanced-rule-action-tab ${rule.actionType === "replace" ? "active" : ""}`}
                                                                onClick={() => updateRule(index, { actionType: "replace" })}
                                                            >
                                                                {t("advanced_replace_label")}
                                                            </button>
                                                            <button 
                                                                type="button" 
                                                                className={`advanced-rule-action-tab ${rule.actionType === "ignore" ? "active" : ""}`}
                                                                onClick={() => updateRule(index, { actionType: "ignore" })}
                                                            >
                                                                {t("app_cleanup_policy_ignore")}
                                                            </button>
                                                        </div>
                                                    </div>

                                                    {rule.actionType === "replace" && (
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
                                                    )}

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
                            </>
                        )}
                    </div>
                </div>
            </section>
        </div>
    );
};

export default AdvancedSettingsGroup;
