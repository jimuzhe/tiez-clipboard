import type { ComponentType, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronRight } from "lucide-react";

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
    setMqttNotificationEnabled
}: SyncSettingsGroupProps) => (
    <div className={`settings-group ${collapsed ? 'collapsed' : ''}`}>
        <div className="group-header" onClick={onToggle}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <h3 style={{ margin: 0 }}>{t('sync_settings')}</h3>
                {mqttEnabled && (
                    <span
                        style={{
                            width: '8px',
                            height: '8px',
                            borderRadius: '50%',
                            backgroundColor: mqttStatus === 'connected' ? '#4CAF50' : mqttStatus === 'connecting' ? '#FF9800' : '#F44336',
                            display: 'inline-block'
                        }}
                        title={mqttStatus === 'connected' ? "Connected" : mqttStatus === 'connecting' ? "Connecting..." : "Disconnected"}
                    />
                )}
            </div>
            {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
        </div>
        {!collapsed && (
            <div className="group-content">
                <div style={{
                    marginBottom: '12px',
                    padding: '8px 12px',
                    background: 'rgba(72, 123, 219, 0.1)',
                    border: '1px solid rgba(72, 123, 219, 0.2)',
                    borderRadius: '4px',
                    display: 'flex',
                    flexDirection: 'row',
                    justifyContent: 'space-between',
                    alignItems: 'center',
                    gap: '12px'
                }}>
                    <span style={{ fontSize: '12px', color: 'var(--text-primary)' }}>{t('mqtt_tutorial_hint')}</span>
                    <button
                        className="btn-icon"
                        style={{ fontSize: '11px', padding: '4px 12px', height: '24px', width: 'auto', flexShrink: 0 }}
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
                    </div>
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={mqttEnabled}
                            onChange={(e) => {
                                const val = e.target.checked;
                                setMqttEnabled(val);
                                saveMqtt('mqtt_enabled', String(val));
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>
                {mqttEnabled && (
                    <>
                        <div className="setting-item">
                            <LabelWithHint
                                label={t('mqtt_protocol') || 'Protocol'}
                                hint={t('mqtt_protocol_hint') || 'mqtt:// = standard (1883), mqtts:// = SSL/TLS (8883), ws:// = WebSocket (8083), wss:// = Secure WebSocket (8084)'}
                                hintKey="mqtt_protocol"
                            />
                            <select
                                className="search-input"
                                style={{ borderRadius: '0', padding: '6px', width: '100px', background: 'var(--bg-input)', border: '2px solid var(--border-dark)', color: 'var(--text-primary)', fontSize: '12px' }}
                                value={mqttProtocol}
                                onChange={e => {
                                    const protocol = e.target.value;
                                    setMqttProtocol(protocol);
                                    saveMqtt('mqtt_protocol', protocol);
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
                                <div className="item-label-group"><span className="item-label">{t('mqtt_ws_path') || 'WS Path'}</span></div>
                                <input
                                    className="search-input"
                                    style={{ borderRadius: '4px', padding: '8px', width: '140px' }}
                                    value={mqttWsPath}
                                    onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                                    onChange={e => { setMqttWsPath(e.target.value); saveMqtt('mqtt_ws_path', e.target.value); }}
                                    placeholder="/mqtt"
                                />
                            </div>
                        )}
                        <div className="setting-item">
                            <div className="item-label-group"><span className="item-label">{t('mqtt_host')}</span></div>
                            <input
                                className="search-input"
                                style={{ borderRadius: '4px', padding: '8px', width: '140px' }}
                                value={mqttServer}
                                onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                                onChange={e => { setMqttServer(e.target.value); saveMqtt('mqtt_server', e.target.value); }}
                                placeholder="mqtt.example.com"
                            />
                        </div>
                        <div className="setting-item">
                            <div className="item-label-group"><span className="item-label">{t('mqtt_port')}</span></div>
                            <input
                                className="search-input"
                                style={{ borderRadius: '4px', padding: '8px', width: '140px' }}
                                value={mqttPort}
                                onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                                onChange={e => { setMqttPort(e.target.value); saveMqtt('mqtt_port', e.target.value); }}
                                placeholder="1883"
                            />
                        </div>
                        <div className="setting-item">
                            <div className="item-label-group"><span className="item-label">{t('mqtt_user')}</span></div>
                            <input
                                className="search-input"
                                style={{ borderRadius: '4px', padding: '8px', width: '140px' }}
                                value={mqttUser}
                                onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                                onChange={e => { setMqttUser(e.target.value); saveMqtt('mqtt_username', e.target.value); }}
                                placeholder="Optional"
                            />
                        </div>
                        <div className="setting-item">
                            <div className="item-label-group"><span className="item-label">{t('mqtt_password')}</span></div>
                            <input
                                className="search-input"
                                style={{ borderRadius: '4px', padding: '8px', width: '140px' }}
                                type="password"
                                value={mqttPass}
                                onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                                onChange={e => { setMqttPass(e.target.value); saveMqtt('mqtt_password', e.target.value); }}
                                placeholder="Optional"
                            />
                        </div>
                        <div className="setting-item no-border">
                            <>
                                <div className="item-label-group"><span className="item-label">{t('mqtt_topic')}</span></div>
                                <input
                                    className="search-input"
                                    style={{ borderRadius: '4px', padding: '8px', width: '140px' }}
                                    value={mqttTopic}
                                    onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                                    onChange={e => { setMqttTopic(e.target.value); saveMqtt('mqtt_topic', e.target.value); }}
                                    placeholder="tiez/my_device"
                                />
                            </>
                        </div>
                        <div className="setting-item">
                            <LabelWithHint
                                label={t('mqtt_notification_enabled')}
                                hint={t('mqtt_notification_enabled_hint')}
                                hintKey="mqtt_notification_enabled"
                            />
                            <label className="switch">
                                <input
                                    className="cb"
                                    type="checkbox"
                                    checked={mqttNotificationEnabled}
                                    onChange={(e) => {
                                        const val = e.target.checked;
                                        setMqttNotificationEnabled(val);
                                        saveMqtt('mqtt_notification_enabled', String(val));
                                    }}
                                />
                                <div className="toggle"><div className="left" /><div className="right" /></div>
                            </label>
                        </div>
                        <div style={{ padding: '0 8px 8px', fontSize: '11px', color: 'var(--text-secondary)', opacity: 0.8 }}>
                            {t('mqtt_restart_hint')}
                        </div>
                    </>
                )}
            </div>
        )}
    </div>
);

export default SyncSettingsGroup;
