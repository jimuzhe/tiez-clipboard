import type { ComponentType, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronRight } from "lucide-react";
import ConfigShareQr from "../ConfigShareQr";
import { SettingSwitch } from "../SettingControls";
import {
    buildMqttConfigQrPayload,
    isMqttConfigQrReady,
} from "../../utils/configQr";
import E2eKeyPanel from "./E2eKeyPanel";

interface LabelWithHintProps {
    label: string;
    hint?: string | ReactNode;
    hintKey: string;
}

interface SyncSettingsGroupProps {
    t: (key: string) => string;
    collapsed: boolean;
    onToggle: () => void;
    LabelWithHint: ComponentType<LabelWithHintProps>;
    mqttEnabled: boolean;
    mqttStatus: "connected" | "disconnected" | "connecting";
    setMqttEnabled: (val: boolean) => void;
    saveMqtt: (key: string, val: string) => void;
    mqttProtocol: string;
    setMqttProtocol: (val: string) => void;
    mqttWsPath: string;
    setMqttWsPath: (val: string) => void;
    mqttServer: string;
    setMqttServer: (val: string) => void;
    mqttPort: string;
    setMqttPort: (val: string) => void;
    mqttUser: string;
    setMqttUser: (val: string) => void;
    mqttPass: string;
    setMqttPass: (val: string) => void;
    mqttTopic: string;
    setMqttTopic: (val: string) => void;
    mqttNotificationEnabled: boolean;
    setMqttNotificationEnabled: (val: boolean) => void;
    e2eKey: string;
    saveE2eKey: (value: string) => Promise<void>;
}

const SyncSettingsGroup = ({
    t,
    collapsed,
    onToggle,
    LabelWithHint,
    mqttEnabled,
    mqttStatus,
    setMqttEnabled,
    saveMqtt,
    mqttProtocol,
    setMqttProtocol,
    mqttWsPath,
    setMqttWsPath,
    mqttServer,
    setMqttServer,
    mqttPort,
    setMqttPort,
    mqttUser,
    setMqttUser,
    mqttPass,
    setMqttPass,
    mqttTopic,
    setMqttTopic,
    mqttNotificationEnabled,
    setMqttNotificationEnabled,
    e2eKey,
    saveE2eKey
}: SyncSettingsGroupProps) => (
    <div className={`settings-group ${collapsed ? 'collapsed' : ''}`}>
        <button type="button" className="group-header" aria-expanded={!collapsed} onClick={onToggle}>
            <div className="group-header-title">
                <h3>{t('sync_settings')}</h3>
                {mqttEnabled && (
                    <span
                        className={`settings-status-dot is-${mqttStatus}`}
                        title={t(`mqtt_status_${mqttStatus}`)}
                    />
                )}
            </div>
            {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
        </button>
        {!collapsed && (
            <div className="group-content">
                <div className="settings-notice settings-notice--action">
                    <span>{t('mqtt_tutorial_hint')}</span>
                    <button
                        type="button"
                        className="settings-action-button settings-action-button--small"
                        onClick={() => {
                            invoke("open_content", {
                                id: 0,
                                content: 'https://my.feishu.cn/docx/JFjVdSqaGou5ZDxFGUZczCz2nth?from=from_copylink',
                                contentType: 'url'
                            });
                        }}
                    >
                        {t('view_tutorial')}
                    </button>
                </div>
                <div className="setting-item">
                    <div className="item-label-group">
                        <span className="item-label">{t('enable_sync')}</span>
                        {/* The header dot alone doesn't say what its colour means. */}
                        {mqttEnabled && <span className="item-desc">{t(`mqtt_status_${mqttStatus}`)}</span>}
                    </div>
                    <SettingSwitch
                        checked={mqttEnabled}
                        onChange={(e) => {
                            const val = e.target.checked;
                            setMqttEnabled(val);
                            saveMqtt('mqtt_enabled', String(val));
                        }}
                    />
                </div>
                {mqttEnabled && (
                    <>
                        <div className="setting-item is-child">
                            <LabelWithHint
                                label={t('mqtt_notification_enabled')}
                                hint={t('mqtt_notification_enabled_hint')}
                                hintKey="mqtt_notification_enabled"
                            />
                            <SettingSwitch
                                checked={mqttNotificationEnabled}
                                onChange={(e) => {
                                    const val = e.target.checked;
                                    setMqttNotificationEnabled(val);
                                    saveMqtt('mqtt_notification_enabled', String(val));
                                }}
                            />
                        </div>

                        <h4 className="settings-section-title">{t('settings_section_connection')}</h4>
                        <div className="setting-item">
                            <LabelWithHint
                                label={t('mqtt_protocol')}
                                hint={t('mqtt_protocol_hint')}
                                hintKey="mqtt_protocol"
                            />
                            <select
                                className="search-input settings-field"
                                value={mqttProtocol}
                                onChange={e => {
                                    const protocol = e.target.value;
                                    setMqttProtocol(protocol);
                                    saveMqtt('mqtt_protocol', protocol);
                                    invoke('save_setting', {
                                        key: 'mqtt_ssl',
                                        value: String(protocol === 'mqtts://' || protocol === 'wss://')
                                    }).catch(console.error);
                                    // Auto-update port based on protocol
                                    let defaultPort = '1883';
                                    if (protocol === 'mqtts://') defaultPort = '8883';
                                    else if (protocol === 'ws://') defaultPort = '8083';
                                    else if (protocol === 'wss://') defaultPort = '8084';
                                    setMqttPort(defaultPort);
                                    saveMqtt('mqtt_port', defaultPort);
                                }}
                            >
                                <option value="mqtt://">mqtt://</option>
                                <option value="mqtts://">mqtts://</option>
                                <option value="ws://">ws://</option>
                                <option value="wss://">wss://</option>
                            </select>
                        </div>
                        {(mqttProtocol === 'ws://' || mqttProtocol === 'wss://') && (
                            <div className="setting-item">
                                <div className="item-label-group"><span className="item-label">{t('mqtt_ws_path')}</span></div>
                                <input
                                    className="search-input settings-field"
                                    value={mqttWsPath}
                                    onChange={e => { setMqttWsPath(e.target.value); saveMqtt('mqtt_ws_path', e.target.value); }}
                                    placeholder="/mqtt"
                                />
                            </div>
                        )}
                        <div className="setting-item">
                            <div className="item-label-group">
                                <span className="item-label">{t('mqtt_host')}</span>
                                {/* Messages are not end-to-end encrypted; the broker's access rules are the only protection. */}
                                <span className="item-desc">{t('mqtt_host_privacy_hint')}</span>
                            </div>
                            <input
                                className="search-input settings-field"
                                value={mqttServer}
                                onChange={e => { setMqttServer(e.target.value); saveMqtt('mqtt_server', e.target.value); }}
                                placeholder="mqtt.example.com"
                            />
                        </div>
                        <div className="setting-item">
                            <div className="item-label-group"><span className="item-label">{t('mqtt_port')}</span></div>
                            <input
                                className="search-input settings-field"
                                value={mqttPort}
                                onChange={e => { setMqttPort(e.target.value); saveMqtt('mqtt_port', e.target.value); }}
                                placeholder="1883"
                            />
                        </div>
                        <div className="setting-item">
                            <div className="item-label-group"><span className="item-label">{t('mqtt_user')}</span></div>
                            <input
                                className="search-input settings-field"
                                value={mqttUser}
                                onChange={e => { setMqttUser(e.target.value); saveMqtt('mqtt_username', e.target.value); }}
                                placeholder={t('optional_placeholder')}
                            />
                        </div>
                        <div className="setting-item">
                            <div className="item-label-group"><span className="item-label">{t('mqtt_password')}</span></div>
                            <input
                                className="search-input settings-field"
                                type="password"
                                value={mqttPass}
                                onChange={e => { setMqttPass(e.target.value); saveMqtt('mqtt_password', e.target.value); }}
                                placeholder={t('optional_placeholder')}
                            />
                        </div>
                        <div className="setting-item">
                            <div className="item-label-group"><span className="item-label">{t('mqtt_topic')}</span></div>
                            <input
                                className="search-input settings-field"
                                value={mqttTopic}
                                onChange={e => { setMqttTopic(e.target.value); saveMqtt('mqtt_topic', e.target.value); }}
                                placeholder="tiez/my_device"
                            />
                        </div>
                        {/* Encryption is not offered to new setups; a device that already
                            has a key still sees it, so it can be turned off. Above the
                            QR code, which carries the key while one is set. */}
                        {e2eKey && <E2eKeyPanel t={t} e2eKey={e2eKey} saveE2eKey={saveE2eKey} hintPrefix="mqtt" />}
                        {(() => {
                            const mqttQr = buildMqttConfigQrPayload({
                                server: mqttServer,
                                port: mqttPort,
                                protocol: mqttProtocol,
                                wsPath: mqttWsPath,
                                username: mqttUser,
                                password: mqttPass,
                                topic: mqttTopic,
                                e2eKey,
                            });
                            const mqttQrReady = isMqttConfigQrReady(mqttQr);
                            return (
                                <ConfigShareQr
                                    value={JSON.stringify(mqttQr)}
                                    ready={mqttQrReady}
                                    title={t('mqtt_share_qr_title')}
                                    hint={t('mqtt_share_qr_hint')}
                                    emptyHint={t('mqtt_share_qr_empty')}
                                revealLabel={t('config_qr_reveal')}
                                revealHint={t('config_qr_reveal_hint')}
                                />
                            );
                        })()}
                    </>
                )}
            </div>
        )}
    </div>
);

export default SyncSettingsGroup;
