import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { ComponentType, ReactNode } from "react";

export interface CloudSyncStatusPayload {
    state: string;
    running: boolean;
    last_sync_at?: number | null;
    last_error?: string | null;
    uploaded_items?: number;
    received_items?: number;
}

interface LabelWithHintProps {
    label: string;
    hint?: string | ReactNode;
    hintKey: string;
}

interface CloudSyncSettingsGroupProps {
    t: (key: string) => string;
    collapsed: boolean;
    onToggle: () => void;
    LabelWithHint: ComponentType<LabelWithHintProps>;
    cloudSyncEnabled: boolean;
    setCloudSyncEnabled: (val: boolean) => void;
    cloudSyncAuto: boolean;
    setCloudSyncAuto: (val: boolean) => void;
    cloudSyncIntervalSec: string;
    setCloudSyncIntervalSec: (val: string) => void;
    cloudSyncSnapshotIntervalMin: string;
    setCloudSyncSnapshotIntervalMin: (val: string) => void;
    cloudSyncWebdavUrl: string;
    setCloudSyncWebdavUrl: (val: string) => void;
    cloudSyncWebdavUsername: string;
    setCloudSyncWebdavUsername: (val: string) => void;
    cloudSyncWebdavPassword: string;
    setCloudSyncWebdavPassword: (val: string) => void;
    cloudSyncWebdavBasePath: string;
    setCloudSyncWebdavBasePath: (val: string) => void;
    saveCloudSync: (key: string, val: string) => void;
    status: CloudSyncStatusPayload;
    syncingNow: boolean;
    onSyncNow: () => void;
}

const statusColor = (state: string) => {
    if (state === "syncing") return "#FF9800";
    if (state === "idle") return "#4CAF50";
    if (state === "error") return "#F44336";
    return "#9E9E9E";
};

const statusLabel = (t: (key: string) => string, state: string) => {
    if (state === "syncing") return t("cloud_sync_status_syncing");
    if (state === "idle") return t("cloud_sync_status_idle");
    if (state === "error") return t("cloud_sync_status_error");
    return t("cloud_sync_status_disabled");
};

const CloudSyncSettingsGroup = ({
    t,
    collapsed,
    onToggle,
    LabelWithHint,
    cloudSyncEnabled,
    setCloudSyncEnabled,
    cloudSyncAuto,
    setCloudSyncAuto,
    cloudSyncIntervalSec,
    setCloudSyncIntervalSec,
    cloudSyncSnapshotIntervalMin,
    setCloudSyncSnapshotIntervalMin,
    cloudSyncWebdavUrl,
    setCloudSyncWebdavUrl,
    cloudSyncWebdavUsername,
    setCloudSyncWebdavUsername,
    cloudSyncWebdavPassword,
    setCloudSyncWebdavPassword,
    cloudSyncWebdavBasePath,
    setCloudSyncWebdavBasePath,
    saveCloudSync,
    status,
    syncingNow,
    onSyncNow
}: CloudSyncSettingsGroupProps) => {
    const normalizeInterval = (raw: string) => {
        const parsed = Number.parseInt(raw, 10);
        if (!Number.isFinite(parsed)) return "120";
        return String(Math.min(3600, Math.max(5, parsed)));
    };
    const normalizeSnapshotIntervalMin = (raw: string) => {
        const parsed = Number.parseInt(raw, 10);
        if (!Number.isFinite(parsed)) return "720";
        return String(Math.min(1440, Math.max(5, parsed)));
    };

    return (
        <div className={`settings-group ${collapsed ? "collapsed" : ""}`}>
            <div className="group-header" onClick={onToggle}>
                <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                    <h3 style={{ margin: 0 }}>{t("cloud_sync_settings")}</h3>
                    <span className="settings-inline-note">Beta</span>
                    {cloudSyncEnabled && (
                        <span
                            style={{
                                width: "8px",
                                height: "8px",
                                borderRadius: "50%",
                                backgroundColor: statusColor(status.state),
                                display: "inline-block"
                            }}
                            title={statusLabel(t, status.state)}
                        />
                    )}
                </div>
                {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
            </div>
            {!collapsed && (
                <div className="group-content">
                    <div
                        style={{
                            marginBottom: "12px",
                            padding: "8px 12px",
                            background: "rgba(72, 123, 219, 0.1)",
                            border: "1px solid rgba(72, 123, 219, 0.2)",
                            borderRadius: "4px",
                            display: "flex",
                            flexDirection: "row",
                            justifyContent: "space-between",
                            alignItems: "center",
                            gap: "12px"
                        }}
                    >
                        <span style={{ fontSize: "12px", color: "var(--text-primary)" }}>{t("mqtt_tutorial_hint")}</span>
                        <button
                            className="btn-icon"
                            style={{ fontSize: "11px", padding: "4px 12px", height: "24px", width: "auto", flexShrink: 0 }}
                            onClick={() => {
                                invoke("open_content", {
                                    id: 0,
                                    content: "https://my.feishu.cn/docx/J8LEdTamioQ4aOxBnVYcgnGlnmd?from=from_copylink",
                                    contentType: "url"
                                });
                            }}
                        >
                            {t("view_tutorial")}
                        </button>
                    </div>

                    <div className="setting-item">
                        <LabelWithHint
                            label={t("cloud_sync_enable")}
                            hint={t("cloud_sync_enable_hint")}
                            hintKey="cloud_sync_enable"
                        />
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={cloudSyncEnabled}
                                onChange={(e) => {
                                    const next = e.target.checked;
                                    setCloudSyncEnabled(next);
                                    saveCloudSync("cloud_sync_enabled", String(next));
                                }}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>

                    <div className="setting-item">
                        <LabelWithHint
                            label={t("cloud_sync_auto")}
                            hint={t("cloud_sync_auto_hint")}
                            hintKey="cloud_sync_auto"
                        />
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={cloudSyncAuto}
                                onChange={(e) => {
                                    const next = e.target.checked;
                                    setCloudSyncAuto(next);
                                    saveCloudSync("cloud_sync_auto", String(next));
                                }}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>

                    {cloudSyncAuto && (
                        <div className="setting-item">
                            <LabelWithHint
                                label={t("cloud_sync_interval")}
                                hint={t("cloud_sync_interval_hint")}
                                hintKey="cloud_sync_interval"
                            />
                            <input
                                className="search-input"
                                style={{ borderRadius: "4px", padding: "8px", width: "180px" }}
                                value={cloudSyncIntervalSec}
                                onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                                onChange={(e) => setCloudSyncIntervalSec(e.target.value)}
                                onBlur={() => {
                                    const next = normalizeInterval(cloudSyncIntervalSec);
                                    setCloudSyncIntervalSec(next);
                                    saveCloudSync("cloud_sync_interval_sec", next);
                                }}
                                placeholder="120"
                            />
                        </div>
                    )}

                    {cloudSyncAuto && (
                        <div className="setting-item">
                            <LabelWithHint
                                label={t("cloud_sync_snapshot_interval")}
                                hint={t("cloud_sync_snapshot_interval_hint")}
                                hintKey="cloud_sync_snapshot_interval"
                            />
                            <input
                                className="search-input"
                                style={{ borderRadius: "4px", padding: "8px", width: "180px" }}
                                value={cloudSyncSnapshotIntervalMin}
                                onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                                onChange={(e) => setCloudSyncSnapshotIntervalMin(e.target.value)}
                                onBlur={() => {
                                    const next = normalizeSnapshotIntervalMin(cloudSyncSnapshotIntervalMin);
                                    setCloudSyncSnapshotIntervalMin(next);
                                    saveCloudSync("cloud_sync_snapshot_interval_min", next);
                                }}
                                placeholder="720"
                            />
                        </div>
                    )}

                    {/* TODO: HTTP provider will be restored after a real server API implementation is available. */}
                    <div className="setting-item">
                        <div className="item-label-group">
                            <span className="item-label">{t("cloud_sync_webdav_url")}</span>
                        </div>
                        <input
                            className="search-input"
                            style={{ borderRadius: "4px", padding: "8px", width: "180px" }}
                            value={cloudSyncWebdavUrl}
                            onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                            onChange={(e) => setCloudSyncWebdavUrl(e.target.value)}
                            onBlur={() => saveCloudSync("cloud_sync_webdav_url", cloudSyncWebdavUrl.trim())}
                            placeholder="https://dav.example.com/remote.php/dav/files/user"
                        />
                    </div>

                    <div className="setting-item">
                        <div className="item-label-group">
                            <span className="item-label">{t("cloud_sync_webdav_username")}</span>
                        </div>
                        <input
                            className="search-input"
                            style={{ borderRadius: "4px", padding: "8px", width: "180px" }}
                            value={cloudSyncWebdavUsername}
                            onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                            onChange={(e) => setCloudSyncWebdavUsername(e.target.value)}
                            onBlur={() => saveCloudSync("cloud_sync_webdav_username", cloudSyncWebdavUsername.trim())}
                            placeholder="username"
                        />
                    </div>

                    <div className="setting-item">
                        <LabelWithHint
                            label={t("cloud_sync_webdav_password")}
                            hint={t("cloud_sync_webdav_password_hint")}
                            hintKey="cloud_sync_webdav_password"
                        />
                        <input
                            className="search-input"
                            type="password"
                            style={{ borderRadius: "4px", padding: "8px", width: "180px" }}
                            value={cloudSyncWebdavPassword}
                            onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                            onChange={(e) => setCloudSyncWebdavPassword(e.target.value)}
                            onBlur={() => saveCloudSync("cloud_sync_webdav_password", cloudSyncWebdavPassword)}
                            placeholder={t("cloud_sync_api_key_placeholder")}
                        />
                    </div>

                    <div className="setting-item">
                        <LabelWithHint
                            label={t("cloud_sync_webdav_base_path")}
                            hint={t("cloud_sync_webdav_base_path_hint")}
                            hintKey="cloud_sync_webdav_base_path"
                        />
                        <input
                            className="search-input"
                            style={{ borderRadius: "4px", padding: "8px", width: "180px" }}
                            value={cloudSyncWebdavBasePath}
                            onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                            onChange={(e) => setCloudSyncWebdavBasePath(e.target.value)}
                            onBlur={() => saveCloudSync("cloud_sync_webdav_base_path", cloudSyncWebdavBasePath.trim() || "tiez-sync")}
                            placeholder="tiez-sync"
                        />
                    </div>

                    <div
                        style={{
                            marginTop: "10px",
                            padding: "8px",
                            border: "1px solid var(--border-dark)",
                            borderRadius: "4px",
                            background: "var(--bg-element)"
                        }}
                    >
                        <div style={{ fontSize: "11px", marginBottom: "6px", color: "var(--text-secondary)" }}>
                            {t("cloud_sync_status_label")}
                        </div>
                        <div style={{ fontSize: "12px", display: "flex", gap: "10px", flexWrap: "wrap" }}>
                            <span>{statusLabel(t, status.state)}</span>
                            <span>{t("cloud_sync_uploaded")}: {status.uploaded_items ?? 0}</span>
                            <span>{t("cloud_sync_received")}: {status.received_items ?? 0}</span>
                        </div>
                        <div style={{ marginTop: "4px", fontSize: "11px", color: "var(--text-secondary)" }}>
                            {t("cloud_sync_last_sync")}: {status.last_sync_at ? new Date(status.last_sync_at).toLocaleString() : "-"}
                        </div>
                        {status.last_error && (
                            <div style={{ marginTop: "4px", fontSize: "11px", color: "#F44336" }}>
                                {t("cloud_sync_last_error")}: {status.last_error}
                            </div>
                        )}
                    </div>

                    <div className="setting-item no-border">
                        <div className="item-label-group">
                            <span className="item-label">{t("cloud_sync_actions")}</span>
                        </div>
                        <button
                            className="btn-icon"
                            style={{ width: "auto", padding: "0 10px", height: "28px" }}
                            onClick={onSyncNow}
                            disabled={syncingNow || status.state === "syncing"}
                            title={t("cloud_sync_now")}
                        >
                            {syncingNow || status.state === "syncing" ? t("checking") : t("cloud_sync_now")}
                        </button>
                    </div>
                </div>
            )}
        </div>
    );
};

export default CloudSyncSettingsGroup;
