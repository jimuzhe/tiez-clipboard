import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { ComponentType, ReactNode } from "react";
import type { CloudSyncContentPrefs } from "../../../app/types";
import ConfigShareQr from "../ConfigShareQr";
import { SettingSwitch } from "../SettingControls";
import {
    buildWebdavConfigQrPayload,
    isWebdavConfigQrReady,
} from "../../utils/configQr";
import { formatError } from "../../../../shared/lib/errorMessage";
import E2eKeyPanel from "./E2eKeyPanel";

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
    cloudSyncContentPrefs: CloudSyncContentPrefs;
    setCloudSyncContentPrefs: (val: CloudSyncContentPrefs) => void;
    saveCloudSync: (key: string, val: string) => void;
    status: CloudSyncStatusPayload;
    syncingNow: boolean;
    onSyncNow: () => void;
    e2eKey: string;
    saveE2eKey: (value: string) => Promise<void>;
}

const statusTone = (state: string) => {
    if (state === "syncing") return "is-connecting";
    if (state === "idle") return "is-connected";
    if (state === "error") return "is-disconnected";
    return "";
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
    cloudSyncContentPrefs,
    setCloudSyncContentPrefs,
    saveCloudSync,
    status,
    syncingNow,
    onSyncNow,
    e2eKey,
    saveE2eKey
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

    const patchContentPrefs = (key: keyof CloudSyncContentPrefs, nextVal: boolean) => {
        const next = { ...cloudSyncContentPrefs, [key]: nextVal };
        setCloudSyncContentPrefs(next);
        saveCloudSync("cloud_sync_content_prefs", JSON.stringify(next));
    };

    return (
        <div className={`settings-group ${collapsed ? "collapsed" : ""}`}>
            <button type="button" className="group-header" aria-expanded={!collapsed} onClick={onToggle}>
                <div className="group-header-title">
                    <h3>{t("cloud_sync_settings")}</h3>
                    <span className="settings-badge">Beta</span>
                    {cloudSyncEnabled && (
                        <span
                            className={`settings-status-dot ${statusTone(status.state)}`}
                            title={statusLabel(t, status.state)}
                        />
                    )}
                </div>
                {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
            </button>
            {!collapsed && (
                <div className="group-content">
                    <div className="settings-notice settings-notice--action">
                        <span>{t("mqtt_tutorial_hint")}</span>
                        <button
                            type="button"
                            className="settings-action-button settings-action-button--small"
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
                        <SettingSwitch
                            checked={cloudSyncEnabled}
                            onChange={(e) => {
                                const next = e.target.checked;
                                setCloudSyncEnabled(next);
                                saveCloudSync("cloud_sync_enabled", String(next));
                            }}
                        />
                    </div>

                    <div className="setting-item">
                        <LabelWithHint
                            label={t("cloud_sync_auto")}
                            hint={t("cloud_sync_auto_hint")}
                            hintKey="cloud_sync_auto"
                        />
                        <SettingSwitch
                            checked={cloudSyncAuto}
                            onChange={(e) => {
                                const next = e.target.checked;
                                setCloudSyncAuto(next);
                                saveCloudSync("cloud_sync_auto", String(next));
                            }}
                        />
                    </div>

                    {cloudSyncAuto && (
                        <div className="setting-item is-child">
                            <LabelWithHint
                                label={t("cloud_sync_interval")}
                                hint={t("cloud_sync_interval_hint")}
                                hintKey="cloud_sync_interval"
                            />
                            <input
                                className="search-input settings-field settings-field--sm settings-field--numeric"
                                value={cloudSyncIntervalSec}
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
                        <div className="setting-item is-child">
                            <LabelWithHint
                                label={t("cloud_sync_snapshot_interval")}
                                hint={t("cloud_sync_snapshot_interval_hint")}
                                hintKey="cloud_sync_snapshot_interval"
                            />
                            <input
                                className="search-input settings-field settings-field--sm settings-field--numeric"
                                value={cloudSyncSnapshotIntervalMin}
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

                    <h4 className="settings-section-title">{t("settings_section_connection")}</h4>
                    {/* TODO: HTTP provider will be restored after a real server API implementation is available. */}
                    <div className="setting-item">
                        <div className="item-label-group">
                            <span className="item-label">{t("cloud_sync_webdav_url")}</span>
                        </div>
                        <input
                            className="search-input settings-field"
                            value={cloudSyncWebdavUrl}
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
                            className="search-input settings-field"
                            value={cloudSyncWebdavUsername}
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
                            className="search-input settings-field"
                            type="password"
                            value={cloudSyncWebdavPassword}
                            onChange={(e) => setCloudSyncWebdavPassword(e.target.value)}
                            onBlur={() => saveCloudSync("cloud_sync_webdav_password", cloudSyncWebdavPassword)}
                            placeholder={t("cloud_sync_webdav_password_placeholder")}
                        />
                    </div>

                    <div className="setting-item">
                        <LabelWithHint
                            label={t("cloud_sync_webdav_base_path")}
                            hint={t("cloud_sync_webdav_base_path_hint")}
                            hintKey="cloud_sync_webdav_base_path"
                        />
                        <input
                            className="search-input settings-field"
                            value={cloudSyncWebdavBasePath}
                            onChange={(e) => setCloudSyncWebdavBasePath(e.target.value)}
                            onBlur={() => saveCloudSync("cloud_sync_webdav_base_path", cloudSyncWebdavBasePath.trim() || "tiez-sync")}
                            placeholder="tiez-sync"
                        />
                    </div>

                    {/* Encryption is not offered to new setups; a device that already
                        has a key still sees it, so it can be turned off. Above the
                        QR code, which carries the key while one is set. */}
                    {e2eKey && <E2eKeyPanel t={t} e2eKey={e2eKey} saveE2eKey={saveE2eKey} hintPrefix="webdav" />}
                    {(() => {
                        const webdavQr = buildWebdavConfigQrPayload({
                            url: cloudSyncWebdavUrl,
                            username: cloudSyncWebdavUsername,
                            password: cloudSyncWebdavPassword,
                            basePath: cloudSyncWebdavBasePath,
                                e2eKey,
                        });
                        const webdavQrReady = isWebdavConfigQrReady(webdavQr);
                        return (
                            <ConfigShareQr
                                value={JSON.stringify(webdavQr)}
                                ready={webdavQrReady}
                                title={t("webdav_share_qr_title")}
                                hint={t("webdav_share_qr_hint")}
                                emptyHint={t("webdav_share_qr_empty")}
                                revealLabel={t("config_qr_reveal")}
                                revealHint={t("config_qr_reveal_hint")}
                            />
                        );
                    })()}

                    <h4 className="settings-section-title">{t("cloud_sync_content_scope")}</h4>
                    <p className="settings-section-desc">{t("cloud_sync_content_scope_hint")}</p>
                    <div className="setting-item">
                        <LabelWithHint
                            label={t("cloud_sync_sync_text")}
                            hint={t("cloud_sync_sync_text_hint")}
                            hintKey="cloud_sync_sync_text"
                        />
                        <SettingSwitch
                            checked={cloudSyncContentPrefs.text}
                            onChange={(e) => patchContentPrefs("text", e.target.checked)}
                        />
                    </div>
                    <div className="setting-item">
                        <LabelWithHint
                            label={t("cloud_sync_sync_image")}
                            hint={t("cloud_sync_sync_image_hint")}
                            hintKey="cloud_sync_sync_image"
                        />
                        <SettingSwitch
                            checked={cloudSyncContentPrefs.image}
                            onChange={(e) => patchContentPrefs("image", e.target.checked)}
                        />
                    </div>
                    <div className="setting-item">
                        <LabelWithHint
                            label={t("cloud_sync_sync_file_path")}
                            hint={t("cloud_sync_sync_file_path_hint")}
                            hintKey="cloud_sync_sync_file_path"
                        />
                        <SettingSwitch
                            checked={cloudSyncContentPrefs.file_path}
                            onChange={(e) => patchContentPrefs("file_path", e.target.checked)}
                        />
                    </div>
                    <div className="setting-item">
                        <LabelWithHint
                            label={t("cloud_sync_sync_emoji")}
                            hint={t("cloud_sync_sync_emoji_hint")}
                            hintKey="cloud_sync_sync_emoji"
                        />
                        <SettingSwitch
                            checked={cloudSyncContentPrefs.emoji}
                            onChange={(e) => patchContentPrefs("emoji", e.target.checked)}
                        />
                    </div>

                    <h4 className="settings-section-title">{t("settings_section_status")}</h4>
                    <div className="settings-panel">
                        <div className="settings-panel-caption">{t("cloud_sync_status_label")}</div>
                        <div className="settings-panel-row">
                            <span>{statusLabel(t, status.state)}</span>
                            <span>{t("cloud_sync_uploaded")}: {status.uploaded_items ?? 0}</span>
                            <span>{t("cloud_sync_received")}: {status.received_items ?? 0}</span>
                        </div>
                        <div className="settings-panel-meta">
                            {t("cloud_sync_last_sync")}: {status.last_sync_at ? new Date(status.last_sync_at).toLocaleString() : "-"}
                        </div>
                        {status.last_error && (
                            <div className="settings-panel-meta is-error">
                                {t("cloud_sync_last_error")}: {status.last_error === "e2e_mismatch" ? t("e2e_mismatch") : formatError(status.last_error, t)}
                            </div>
                        )}
                    </div>

                    <div className="setting-item">
                        <div className="item-label-group">
                            <span className="item-label">{t("cloud_sync_actions")}</span>
                        </div>
                        <button
                            type="button"
                            className="settings-action-button"
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
