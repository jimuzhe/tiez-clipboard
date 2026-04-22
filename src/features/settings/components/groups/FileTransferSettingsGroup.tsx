import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { ChevronDown, ChevronRight, HelpCircle } from "lucide-react";
import { motion } from "framer-motion";
import { QRCodeCanvas } from "qrcode.react";

interface FileTransferSettingsGroupProps {
    t: (key: string) => string;
    collapsed: boolean;
    onToggle: () => void;
    fileServerEnabled: boolean;
    setFileServerEnabled: (val: boolean) => void;
    fileServerPort: string;
    setFileServerPort: (val: string) => void;
    applyFileServerPort: (portStr: string) => void;
    localIp: string;
    availableIps?: string[];
    setLocalIp?: (val: string) => void;
    actualPort: string;
    fileTransferAutoOpen: boolean;
    setFileTransferAutoOpen: (val: boolean) => void;
    showAutoCloseHint: boolean;
    setShowAutoCloseHint: (val: boolean) => void;
    fileServerAutoClose: boolean;
    setFileServerAutoClose: (val: boolean) => void;
    fileTransferAutoCopy: boolean;
    setFileTransferAutoCopy: (val: boolean) => void;
    onOpenChat?: () => void;
    fileTransferPath: string;
    saveSetting: (key: string, val: string) => void;
    fetchEffectiveTransferPath: () => void;
}

const FileTransferSettingsGroup = ({
    t,
    collapsed,
    onToggle,
    fileServerEnabled,
    setFileServerEnabled,
    fileServerPort,
    setFileServerPort,
    applyFileServerPort,
    localIp,
    availableIps,
    setLocalIp,
    actualPort,
    fileTransferAutoOpen,
    setFileTransferAutoOpen,
    showAutoCloseHint,
    setShowAutoCloseHint,
    fileServerAutoClose,
    setFileServerAutoClose,
    fileTransferAutoCopy,
    setFileTransferAutoCopy,
    onOpenChat,
    fileTransferPath,
    saveSetting,
    fetchEffectiveTransferPath
}: FileTransferSettingsGroupProps) => (
    <div className={`settings-group ${collapsed ? 'collapsed' : ''}`}>
        <div className="group-header" onClick={onToggle}>
            <h3 style={{ margin: 0 }}>{t('file_transfer')}</h3>
            {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
        </div>
        {!collapsed && (
            <div className="group-content">
                <div className="setting-item">
                    <div className="item-label-group">
                        <span className="item-label">{t('enable_file_server')}</span>
                    </div>
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={fileServerEnabled}
                            onChange={(e) => {
                                const val = e.target.checked;
                                setFileServerEnabled(val);
                                const port = Number(fileServerPort);
                                invoke("toggle_file_server", { enabled: val, port: Number.isInteger(port) ? port : undefined });
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>

                {fileServerEnabled && (
                    <>
                        <div className="setting-item">
                            <div className="item-label-group"><span className="item-label">{t('file_server_port')}</span></div>
                            <input
                                className="search-input"
                                style={{ borderRadius: '4px', padding: '8px', width: '80px' }}
                                value={fileServerPort}
                                onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                                onChange={e => { setFileServerPort(e.target.value); }}
                                onBlur={() => applyFileServerPort(fileServerPort)}
                                onKeyDown={(e) => {
                                    if (e.key === 'Enter') {
                                        e.preventDefault();
                                        applyFileServerPort(fileServerPort);
                                    }
                                }}
                                placeholder="18888"
                            />
                        </div>
                        <div className="setting-item no-border">
                            <div className="item-label-group"><span className="item-label">Local IP</span></div>
                            <div className="data-panel" style={{ minWidth: '160px', justifyContent: 'flex-start', padding: '6px 10px', height: 'auto' }}>
                                {availableIps && availableIps.length > 1 && setLocalIp ? (
                                    <select
                                        className="search-input"
                                        style={{
                                            border: 'none',
                                            background: 'transparent',
                                            padding: 0,
                                            margin: 0,
                                            fontSize: '11px',
                                            height: 'auto',
                                            width: 'auto',
                                            minWidth: '0',
                                            boxShadow: 'none'
                                        }}
                                        value={localIp}
                                        onChange={(e) => {
                                            const newIp = e.target.value;
                                            setLocalIp(newIp);
                                            invoke("set_display_ip", { ip: newIp }).catch(console.error);
                                        }}
                                    >
                                        {availableIps.map(ip => (
                                            <option key={ip} value={ip}>{ip}</option>
                                        ))}
                                    </select>
                                ) : (
                                    <span style={{ fontSize: '11px' }}>{localIp}</span>
                                )}
                                <span style={{ fontSize: '11px', opacity: 0.7 }}>:{Number(actualPort) > 0 ? actualPort : fileServerPort}</span>
                            </div>
                        </div>
                        <div className="setting-item">
                            <div className="item-label-group"><span className="item-label">{t('file_transfer_auto_open')}</span></div>
                            <label className="switch">
                                <input
                                    className="cb"
                                    type="checkbox"
                                    checked={fileTransferAutoOpen}
                                    onChange={(e) => {
                                        const val = e.target.checked;
                                        setFileTransferAutoOpen(val);
                                    }}
                                />
                                <div className="toggle"><div className="left" /><div className="right" /></div>
                            </label>
                        </div>
                        <div className="setting-item">
                            <div className="item-label-group">
                                <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                                    <span className="item-label">{t('file_server_auto_close')}</span>
                                    <button
                                        onClick={() => setShowAutoCloseHint(!showAutoCloseHint)}
                                        className="hint-icon-btn"
                                        style={{ background: 'transparent', border: 'none', padding: 0, cursor: 'pointer', display: 'flex', opacity: showAutoCloseHint ? 1 : 0.6 }}
                                    >
                                        <HelpCircle size={12} />
                                    </button>
                                </div>
                            </div>
                            <label className="switch">
                                <input
                                    className="cb"
                                    type="checkbox"
                                    checked={fileServerAutoClose}
                                    onChange={(e) => {
                                        const val = e.target.checked;
                                        setFileServerAutoClose(val);
                                    }}
                                />
                                <div className="toggle"><div className="left" /><div className="right" /></div>
                            </label>
                        </div>

                        {showAutoCloseHint && (
                            <div style={{ margin: '4px 8px 12px', padding: '10px 14px', background: 'rgba(72, 123, 219, 0.08)', border: '1px solid rgba(72, 123, 219, 0.2)', borderRadius: '8px', fontSize: '11px', color: 'var(--text-secondary)', lineHeight: '1.5' }}>
                                {t('auto_close_hint')}
                            </div>
                        )}
                        <div className="setting-item">
                            <div className="item-label-group"><span className="item-label">{t('file_transfer_auto_copy')}</span></div>
                            <label className="switch">
                                <input
                                    className="cb"
                                    type="checkbox"
                                    checked={fileTransferAutoCopy}
                                    onChange={(e) => {
                                        const val = e.target.checked;
                                        setFileTransferAutoCopy(val);
                                    }}
                                />
                                <div className="toggle"><div className="left" /><div className="right" /></div>
                            </label>
                        </div>

                        {localIp && actualPort && (
                            <motion.div
                                initial={{ height: 0, opacity: 0 }}
                                animate={{ height: 'auto', opacity: 1 }}
                                style={{ overflow: 'hidden', paddingBottom: '10px' }}
                            >
                                <div className="file-transfer-panel">
                                    <div className="qr-container">
                                        <QRCodeCanvas value={`http://${localIp}:${actualPort}`} size={90} />
                                        <div className="qr-label">SCAN ME</div>
                                    </div>
                                    <div className="transfer-info">
                                        <div className="scan-title">{t('scan_to_send')}</div>
                                        <div className="info-row">
                                            <span className="info-label">STATUS</span>
                                            <span className="status-online">ONLINE</span>
                                        </div>
                                        <div className="info-row">
                                            <span className="info-label">HOST</span>
                                            <span className="info-value">{localIp}</span>
                                        </div>
                                        <div className="info-row">
                                            <span className="info-label">PORT</span>
                                            <span className="info-value">{actualPort}</span>
                                        </div>
                                        <div className="open-browser-btn" onClick={() => {
                                            if (onOpenChat) onOpenChat();
                                        }}>
                                            {t('open_page')}
                                        </div>
                                    </div>
                                </div>
                            </motion.div>
                        )}

                        <div className="setting-item column no-border">
                            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '8px' }}>
                                <span className="item-label">{t('custom_save_path')}</span>
                                <button
                                    className="btn-icon"
                                    style={{ width: 'auto', fontSize: '10px', height: '24px', padding: '0 8px' }}
                                    onClick={async () => {
                                        try {
                                            const selected = await open({ directory: true, multiple: false });
                                            if (selected) {
                                                saveSetting('file_transfer_path', selected as string);
                                                setTimeout(fetchEffectiveTransferPath, 100);
                                            }
                                        } catch (e) { console.error(e); }
                                    }}
                                >
                                    {t('choose_path')}
                                </button>
                            </div>
                            <div
                                className="data-panel"
                                style={{ fontSize: '11px', color: 'var(--text-secondary)', cursor: 'pointer', display: 'flex', alignItems: 'center', gap: '4px' }}
                                onClick={async () => {
                                    let path = fileTransferPath;
                                    if (!path) {
                                        try {
                                            path = await invoke<string>("get_active_file_transfer_path");
                                        } catch (e) { console.error(e); }
                                    }
                                    if (path) {
                                        invoke("open_folder", { path }).catch(console.error);
                                    }
                                }}
                                title={fileTransferPath || t('not_set')}
                            >
                                {fileTransferPath ? fileTransferPath : t('not_set')}
                            </div>
                        </div>
                    </>
                )}
            </div>
        )}
    </div>
);

export default FileTransferSettingsGroup;
