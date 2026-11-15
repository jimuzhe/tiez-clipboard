import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, ask, message } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import {
    AlertTriangle,
    ArchiveRestore,
    ArrowLeft,
    Check,
    CheckCircle2,
    ChevronDown,
    ChevronRight,
    Database,
    FolderOpen,
    ShieldCheck,
    X
} from "lucide-react";
import AppModal from "../../../../shared/components/AppModal";
import { withNativeDialog } from "../../../../shared/lib/focus";
import { isMacPlatform, isWindowsPlatform } from "../../../../shared/lib/platform";
import { formatError } from "../../../../shared/lib/errorMessage";

interface DataSettingsGroupProps {
    t: (key: string) => string;
    theme: string;
    collapsed: boolean;
    onToggle: () => void;
    dataPath: string;
    /** Portable build: the data stays in the folder next to the app. */
    portable: boolean;
}

interface ImportReport {
    source: string;
    scanned: number;
    imported: number;
    duplicates: number;
    unsupported: number;
    failed: number;
    warnings: string[];
}

interface ImportProgress {
    operationId: string;
    source: string;
    scanned: number;
    imported: number;
    duplicates: number;
    unsupported: number;
    failed: number;
}

interface DiscoveredImportSource {
    source: string;
    path: string;
    modifiedAt: number;
}

interface SupportedImportApp {
    id: "maccy" | "ditto";
    name: string;
    platform: string;
    summaryKey: string;
    chooseKey: string;
    fileTypeKey: string;
    extensions: string[];
}

type ImportStep = "apps" | "confirm" | "progress" | "result" | "error";

const allImportApps: SupportedImportApp[] = [
    {
        id: "maccy",
        name: "Maccy",
        platform: "macOS",
        summaryKey: "third_party_import_summary_maccy",
        chooseKey: "third_party_import_choose_maccy",
        fileTypeKey: "third_party_import_file_type_maccy",
        extensions: ["sqlite", "sqlite3"]
    },
    {
        id: "ditto",
        name: "Ditto",
        platform: "Windows",
        summaryKey: "third_party_import_summary_ditto",
        chooseKey: "third_party_import_choose_ditto",
        fileTypeKey: "third_party_import_file_type_ditto",
        extensions: ["db", "dto"]
    }
];

const createOperationId = () => {
    if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
        return crypto.randomUUID();
    }
    return `clipboard-import-${Date.now()}-${Math.random().toString(16).slice(2)}`;
};

const DataSettingsGroup = ({
    t,
    theme,
    collapsed,
    onToggle,
    dataPath,
    portable
}: DataSettingsGroupProps) => {
    const [modalOpen, setModalOpen] = useState(false);
    const [step, setStep] = useState<ImportStep>("apps");
    const [discovering, setDiscovering] = useState(false);
    const [discovered, setDiscovered] = useState<DiscoveredImportSource[]>([]);
    const [selectedApp, setSelectedApp] = useState<SupportedImportApp | null>(null);
    const [selectedPath, setSelectedPath] = useState("");
    const [progress, setProgress] = useState<ImportProgress | null>(null);
    const [report, setReport] = useState<ImportReport | null>(null);
    const [importError, setImportError] = useState("");
    const importing = step === "progress";

    const supportedApps = isMacPlatform()
        ? allImportApps.filter((app) => app.id === "maccy")
        : isWindowsPlatform()
            ? allImportApps.filter((app) => app.id === "ditto")
            : allImportApps;

    const format = (key: string, values: Record<string, string | number>) =>
        Object.entries(values).reduce(
            (text, [name, value]) => text.split(`{${name}}`).join(String(value)),
            t(key)
        );

    const closeImportModal = () => {
        if (importing) return;
        setModalOpen(false);
    };

    const openImportModal = async () => {
        setModalOpen(true);
        setStep("apps");
        setSelectedApp(null);
        setSelectedPath("");
        setProgress(null);
        setReport(null);
        setImportError("");
        setDiscovering(true);
        try {
            setDiscovered(await invoke<DiscoveredImportSource[]>(
                "discover_clipboard_import_sources"
            ));
        } catch (error) {
            console.error(error);
            setDiscovered([]);
        } finally {
            setDiscovering(false);
        }
    };

    const chooseImportApp = (app: SupportedImportApp) => {
        const detected = discovered.find(
            (source) => source.source.toLowerCase() === app.name.toLowerCase()
        );
        setSelectedApp(app);
        setSelectedPath(detected?.path ?? "");
        setStep("confirm");
    };

    const chooseImportFile = async () => {
        if (!selectedApp || importing) return;
        const selected = await withNativeDialog(() => open({
            directory: false,
            multiple: false,
            title: t(selectedApp.chooseKey),
            filters: [
                {
                    name: t(selectedApp.fileTypeKey),
                    extensions: selectedApp.extensions
                }
            ]
        }), "settings:choose-import-source");
        if (selected && !Array.isArray(selected)) {
            setSelectedPath(selected);
        }
    };

    const startImport = async () => {
        if (!selectedApp || importing) return;
        if (!selectedPath) {
            await chooseImportFile();
            return;
        }

        const operationId = createOperationId();
        setStep("progress");
        setProgress({
            operationId,
            source: selectedApp.name,
            scanned: 0,
            imported: 0,
            duplicates: 0,
            unsupported: 0,
            failed: 0
        });
        setReport(null);
        setImportError("");

        let unlisten: (() => void) | null = null;
        try {
            unlisten = await listen<ImportProgress>(
                "clipboard-import-progress",
                ({ payload }) => {
                    if (payload.operationId === operationId) {
                        setProgress(payload);
                    }
                }
            );
            const result = await invoke<ImportReport>("import_clipboard_data", {
                path: selectedPath,
                operationId
            });
            setProgress({
                operationId,
                source: result.source,
                scanned: result.scanned,
                imported: result.imported,
                duplicates: result.duplicates,
                unsupported: result.unsupported,
                failed: result.failed
            });
            setReport(result);
            setStep("result");
        } catch (error) {
            console.error(error);
            setImportError(formatError(error, t));
            setStep("error");
        } finally {
            unlisten?.();
        }
    };

    const renderImportBody = () => {
        if (step === "apps") {
            return (
                <>
                    <div className="clipboard-import-modal-intro">
                        {t("third_party_import_supported_apps")}
                    </div>
                    <div className="clipboard-import-app-list">
                        {supportedApps.map((app) => {
                            const detected = discovered.find(
                                (source) => source.source.toLowerCase() === app.name.toLowerCase()
                            );
                            return (
                                <button
                                    key={app.id}
                                    type="button"
                                    className="clipboard-import-app-option"
                                    onClick={() => chooseImportApp(app)}
                                >
                                    <span className="clipboard-import-app-copy">
                                        <span className="clipboard-import-app-heading">
                                            <strong>{app.name}</strong>
                                            <span>{app.platform}</span>
                                        </span>
                                        <span className="clipboard-import-app-summary">
                                            {t(app.summaryKey)}
                                        </span>
                                        <span className={`clipboard-import-detection ${detected ? "is-detected" : ""}`}>
                                            {discovering
                                                ? t("third_party_import_detecting")
                                                : detected
                                                    ? t("third_party_import_detected")
                                                    : t("third_party_import_not_detected")}
                                        </span>
                                    </span>
                                    <ChevronRight size={17} aria-hidden="true" />
                                </button>
                            );
                        })}
                    </div>
                </>
            );
        }

        if (step === "confirm" && selectedApp) {
            return (
                <>
                    <div className="clipboard-import-selected-app">
                        <span>
                            <strong>{selectedApp.name}</strong>
                            <small>{selectedApp.platform}</small>
                        </span>
                    </div>
                    <div className="clipboard-import-reminder">
                        <div className="clipboard-import-reminder-title">
                            <ShieldCheck size={17} aria-hidden="true" />
                            {t("third_party_import_reminder_title")}
                        </div>
                        <ul>
                            <li>{t("third_party_import_reminder_readonly")}</li>
                            <li>{t("third_party_import_reminder_append")}</li>
                            <li>{t("third_party_import_reminder_duplicates")}</li>
                        </ul>
                    </div>
                    <div className="clipboard-import-source">
                        <div className="clipboard-import-source-label">
                            {t("third_party_import_source_path")}
                        </div>
                        <div className={`clipboard-import-source-path ${selectedPath ? "" : "is-empty"}`}>
                            <Database size={15} aria-hidden="true" />
                            <span>
                                {selectedPath || t("third_party_import_source_missing")}
                            </span>
                        </div>
                        {selectedPath && (
                            <button
                                type="button"
                                className="clipboard-import-text-button"
                                onClick={chooseImportFile}
                            >
                                <FolderOpen size={14} aria-hidden="true" />
                                {t("third_party_import_change_file")}
                            </button>
                        )}
                    </div>
                </>
            );
        }

        if (step === "progress") {
            const current = progress ?? {
                operationId: "",
                source: selectedApp?.name ?? "",
                scanned: 0,
                imported: 0,
                duplicates: 0,
                unsupported: 0,
                failed: 0
            };
            return (
                <div className="clipboard-import-progress">
                    <div className="clipboard-import-progress-icon" aria-hidden="true">
                        <ArchiveRestore size={22} />
                    </div>
                    <strong>
                        {format("third_party_import_progress_title", {
                            source: current.source
                        })}
                    </strong>
                    <p>{t("third_party_import_progress_desc")}</p>
                    <div
                        className="clipboard-import-progress-track"
                        role="progressbar"
                        aria-valuetext={format("third_party_import_processed", {
                            current: current.scanned
                        })}
                    >
                        <span />
                    </div>
                    <div className="clipboard-import-progress-count">
                        {format("third_party_import_processed", {
                            current: current.scanned
                        })}
                    </div>
                    <div className="clipboard-import-live-stats">
                        <span>{t("third_party_import_result_imported")} <strong>{current.imported}</strong></span>
                        <span>{t("third_party_import_result_duplicates")} <strong>{current.duplicates}</strong></span>
                        <span>{t("third_party_import_result_failed")} <strong>{current.failed}</strong></span>
                    </div>
                </div>
            );
        }

        if (step === "result" && report) {
            return (
                <div className="clipboard-import-result">
                    <div className="clipboard-import-result-icon" aria-hidden="true">
                        <CheckCircle2 size={24} />
                    </div>
                    <strong>{t("third_party_import_complete")}</strong>
                    <p>
                        {format("third_party_import_complete_desc", {
                            source: report.source,
                            scanned: report.scanned
                        })}
                    </p>
                    <div className="clipboard-import-result-grid">
                        <div><strong>{report.imported}</strong><span>{t("third_party_import_result_imported")}</span></div>
                        <div><strong>{report.duplicates}</strong><span>{t("third_party_import_result_duplicates")}</span></div>
                        <div><strong>{report.unsupported}</strong><span>{t("third_party_import_result_unsupported")}</span></div>
                        <div className={report.failed > 0 ? "has-error" : ""}>
                            <strong>{report.failed}</strong><span>{t("third_party_import_result_failed")}</span>
                        </div>
                    </div>
                    {report.warnings.length > 0 && (
                        <details className="clipboard-import-warnings">
                            <summary>{t("third_party_import_warnings")}</summary>
                            <ul>
                                {report.warnings.map((warning) => <li key={warning}>{warning}</li>)}
                            </ul>
                        </details>
                    )}
                </div>
            );
        }

        return (
            <div className="clipboard-import-result is-error">
                <div className="clipboard-import-result-icon" aria-hidden="true">
                    <AlertTriangle size={24} />
                </div>
                <strong>{t("third_party_import_error_title")}</strong>
                <p>{format("third_party_import_failed", { e: importError })}</p>
            </div>
        );
    };

    return (
        <>
            <div className={`settings-group ${collapsed ? "collapsed" : ""}`}>
                <button type="button" className="group-header" aria-expanded={!collapsed} onClick={onToggle}>
                    <h3>{t("data_management")}</h3>
                    {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
                </button>
                {!collapsed && (
                    <div className="group-content">
                        <div className="setting-item column">
                            <div className="setting-item-heading">
                                <div className="item-label-group">
                                    <span className="item-label">{t("data_path")}</span>
                                </div>
                                <div className="settings-action-group">
                                    {!portable && (
                                        <button
                                            type="button"
                                            className="settings-action-button settings-action-button--small"
                                            onClick={async () => {
                                                await withNativeDialog(async () => {
                                                    const selected = await open({
                                                        directory: true,
                                                        multiple: false,
                                                        title: t("change_data_path")
                                                    });
                                                    if (!selected) return;

                                                    const newPath = selected as string;
                                                    const confirmed = await ask(
                                                        t("data_move_confirm").replace("{path}", newPath),
                                                        { title: t("change_data_path"), kind: "warning", okLabel: t("data_move_action"), cancelLabel: t("cancel") }
                                                    );
                                                    if (!confirmed) return;

                                                    try {
                                                        await invoke("set_data_path", { newPath });
                                                        // The dialog before this already said the app restarts.
                                                        await invoke("relaunch");
                                                    } catch (error: unknown) {
                                                        console.error(error);
                                                        await message(
                                                            t("data_move_failed").replace("{e}", formatError(error, t)),
                                                            { title: t("change_data_path"), kind: "error" }
                                                        );
                                                    }
                                                }, "settings:change-data-path");
                                            }}
                                        >
                                            {t("change_app")}
                                        </button>
                                    )}
                                    <button
                                        type="button"
                                        className="settings-action-button settings-action-button--small"
                                        onClick={async () => {
                                            try {
                                                if (!dataPath) throw new Error(t("not_set"));
                                                await invoke("open_folder", { path: dataPath });
                                            } catch (error: unknown) {
                                                console.error(error);
                                                await withNativeDialog(() => message(
                                                    `${t("open_failed")}${formatError(error, t)}`,
                                                    { title: t("error"), kind: "error" }
                                                ), "settings:data-path-error");
                                            }
                                        }}
                                        title={t("open_folder")}
                                    >
                                        {t("open_folder")}
                                    </button>
                                </div>
                            </div>
                            <div className="data-panel settings-path-panel is-static">
                                {dataPath}
                            </div>
                            {portable && (
                                <span className="item-desc">{t("data_path_portable_hint")}</span>
                            )}
                        </div>
                        <div className="setting-item clipboard-import-entry">
                            <div className="item-label-group">
                                <span className="item-label">{t("third_party_import_entry_title")}</span>
                                <span className="item-desc">
                                    {t("third_party_import_entry_desc")}
                                </span>
                            </div>
                            <button
                                type="button"
                                className="settings-action-button clipboard-import-launch"
                                onClick={openImportModal}
                            >
                                <ArchiveRestore size={14} aria-hidden="true" />
                                {t("third_party_import_choose_app")}
                            </button>
                        </div>
                    </div>
                )}
            </div>

            <AppModal
                open={modalOpen}
                onClose={closeImportModal}
                theme={theme}
                panelClassName="modal-panel clipboard-import-modal"
                closeOnOverlayClick={!importing}
                ariaLabel={t("third_party_import_modal_title")}
            >
                <div className="clipboard-import-modal-header">
                    <div>
                        <h2>{t("third_party_import_modal_title")}</h2>
                        <p>{t("third_party_import_modal_desc")}</p>
                    </div>
                    <button
                        type="button"
                        className="modal-button icon-only"
                        onClick={closeImportModal}
                        disabled={importing}
                        aria-label={t("close")}
                    >
                        <X size={17} />
                    </button>
                </div>

                <div className="clipboard-import-modal-body">
                    {renderImportBody()}
                </div>

                {(step === "confirm" || step === "result" || step === "error") && (
                    <div className="clipboard-import-modal-actions">
                        {step === "confirm" && (
                            <button
                                type="button"
                                className="modal-button"
                                onClick={() => setStep("apps")}
                            >
                                <ArrowLeft size={14} />
                                {t("third_party_import_back")}
                            </button>
                        )}
                        {step === "confirm" && (
                            <button
                                type="button"
                                className="modal-button primary"
                                onClick={startImport}
                            >
                                {selectedPath ? <Check size={14} /> : <FolderOpen size={14} />}
                                {selectedPath
                                    ? t("third_party_import_start")
                                    : t("third_party_import_choose_manually")}
                            </button>
                        )}
                        {(step === "result" || step === "error") && (
                            <button
                                type="button"
                                className="modal-button primary"
                                onClick={closeImportModal}
                            >
                                {t("third_party_import_close")}
                            </button>
                        )}
                    </div>
                )}
            </AppModal>
        </>
    );
};

export default DataSettingsGroup;
