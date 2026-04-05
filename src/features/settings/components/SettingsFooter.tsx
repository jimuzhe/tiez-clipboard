import { Github, MessageSquare, RotateCcw } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

interface SettingsFooterProps {
    t: (key: string) => string;
    appVersion: string;
    updateStatus: string;
    setUpdateStatus: (val: string) => void;
    onResetSettings: () => void;
    emailCopied: boolean;
    setEmailCopied: (val: boolean) => void;
}

const SettingsFooter = ({
    t,
    appVersion,
    updateStatus,
    setUpdateStatus,
    onResetSettings,
    emailCopied,
    setEmailCopied
}: SettingsFooterProps) => (
    <>
        {/* Footer Actions */}
        <div style={{
            marginTop: '16px',
            display: 'flex',
            justifyContent: 'center',
            gap: '12px',
            flexWrap: 'wrap'
        }}>
            {/* Feedback Card */}
            <div
                className="settings-group"
                style={{
                    cursor: 'pointer',
                    transition: 'all 0.2s',
                    margin: 0,
                    width: 'auto',
                    padding: '10px 16px',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    marginBottom: '0'
                }}
                onClick={() => {
                    navigator.clipboard.writeText('tiez@name666.top');
                    setEmailCopied(true);
                    setTimeout(() => setEmailCopied(false), 2000);
                }}
            >
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <MessageSquare size={16} />
                    <span style={{ fontSize: '13px', fontWeight: 600 }}>
                        {emailCopied ? t('email_copied') : t('feedback')}
                    </span>
                </div>
            </div>

            {/* Reset Card */}
            <div
                className="settings-group"
                style={{
                    cursor: 'pointer',
                    transition: 'all 0.2s',
                    margin: 0,
                    width: 'auto',
                    padding: '10px 16px',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    marginBottom: '0'
                }}
                onClick={() => onResetSettings()}
            >
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <RotateCcw size={16} />
                    <span style={{ fontSize: '13px', fontWeight: 600 }}>{t('reset_defaults')}</span>
                </div>
            </div>
        </div>

        {/* Version Info */}
        <div style={{
            marginTop: '16px',
            marginBottom: '32px',
            textAlign: 'center',
            opacity: 1
        }}>
            <div style={{
                fontSize: '13px',
                fontWeight: 600,
                color: 'var(--text-secondary)',
                letterSpacing: '0.5px',
                marginBottom: '4px',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                gap: '8px'
            }}>
                <span>TieZ {appVersion ? `v${appVersion}` : "v0.2.0"}</span>
                <button
                    onClick={async () => {
                        if (updateStatus) return;
                        setUpdateStatus(t('checking'));
                        try {
                            const update = await check();
                            if (update) {
                                setUpdateStatus(t('downloading'));
                                await update.downloadAndInstall();
                                // Native relaunch for seamless update experience
                                await relaunch();
                            } else {
                                setUpdateStatus(t('up_to_date'));
                                setTimeout(() => setUpdateStatus(''), 3000);
                            }
                        } catch (err) {
                            console.error('Update check/install failed:', err);
                            setUpdateStatus(t('checking_failed'));
                            setTimeout(() => setUpdateStatus(''), 3000);
                        }
                    }}
                    disabled={!!updateStatus}
                    style={{
                        border: 'none',
                        background: 'transparent',
                        color: (updateStatus && (updateStatus.includes('Failed') || updateStatus.includes('失败'))) ? '#ff4d4f' : 'var(--accent-color)',
                        cursor: updateStatus ? 'default' : 'pointer',
                        fontSize: '11px',
                        padding: '2px 6px',
                        borderRadius: '4px',
                        opacity: updateStatus ? 1 : 0.8,
                        fontWeight: updateStatus ? 'bold' : 'normal',
                        transition: 'all 0.2s'
                    }}
                    onMouseEnter={(e) => !updateStatus && (e.currentTarget.style.opacity = '1')}
                    onMouseLeave={(e) => !updateStatus && (e.currentTarget.style.opacity = '0.8')}
                >
                    {updateStatus || t('check_update')}
                </button>
            </div>
            <div style={{
                fontSize: '11px',
                color: 'var(--text-secondary)',
                fontWeight: 500,
                marginBottom: '4px'
            }}>
                {t('slogan')}
            </div>
            <div style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                gap: '10px',
                flexWrap: 'wrap'
            }}>
                <button
                    onClick={() => openUrl('https://tiez.name666.top/')}
                    style={{
                        fontSize: '11px',
                        color: 'var(--accent-color)',
                        background: 'transparent',
                        border: 'none',
                        cursor: 'pointer',
                        textDecoration: 'underline',
                        opacity: 0.7,
                        fontWeight: 600,
                        padding: '2px 4px'
                    }}
                    onMouseEnter={(e) => (e.currentTarget.style.opacity = '1')}
                    onMouseLeave={(e) => (e.currentTarget.style.opacity = '0.7')}
                >
                    {t('official_website')}
                </button>
                <button
                    onClick={() => openUrl('https://github.com/jimuzhe/tiez-clipboard')}
                    style={{
                        fontSize: '11px',
                        color: 'var(--accent-color)',
                        background: 'transparent',
                        border: 'none',
                        cursor: 'pointer',
                        textDecoration: 'underline',
                        opacity: 0.7,
                        fontWeight: 600,
                        padding: '2px 4px',
                        display: 'inline-flex',
                        alignItems: 'center',
                        gap: '4px'
                    }}
                    onMouseEnter={(e) => (e.currentTarget.style.opacity = '1')}
                    onMouseLeave={(e) => (e.currentTarget.style.opacity = '0.7')}
                >
                    <Github size={12} />
                    <span>GitHub</span>
                </button>
            </div>
        </div>
    </>
);

export default SettingsFooter;
