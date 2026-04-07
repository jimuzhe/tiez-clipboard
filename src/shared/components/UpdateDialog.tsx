import React, { useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Download, RefreshCw, X, ExternalLink } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import "./UpdateDialog.css"; // Import the custom styles

interface UpdateDialogProps {
  isOpen: boolean;
  version: string;
  notes: string;
  downloadProgress: number;
  status: "idle" | "checking" | "downloading" | "ready" | "error";
  onUpdate: () => void;
  onClose: () => void;
}

const UpdateDialog: React.FC<UpdateDialogProps> = ({
  isOpen,
  version,
  notes,
  downloadProgress,
  status,
  onUpdate,
  onClose,
}) => {
  useEffect(() => {
    if (isOpen) {
      invoke("set_ignore_blur", { ignore: true }).catch(console.error);
    } else {
      invoke("set_ignore_blur", { ignore: false }).catch(console.error);
    }
  }, [isOpen]);

  const handleOpenWebsite = () => {
    openUrl("https://tiez.name666.top/");
  };

  return (
    <AnimatePresence>
      {isOpen && (
        <div className="update-overlay">
          <motion.div
            initial={{ opacity: 0, scale: 0.9, y: 20 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.9, y: 20 }}
            className="update-modal"
          >
            {/* Header */}
            <div className="update-header">
              <div className="update-header-info">
                <h3 className="update-title">发现新版本</h3>
                <span className="update-version">v{version}</span>
              </div>
              <button 
                onClick={onClose}
                className="update-close-btn"
              >
                <X size={16} />
              </button>
            </div>

            {/* Content */}
            <div className="update-content">
              <div className="update-notes-container custom-scrollbar">
                <p className="update-notes-text">
                  {status === "error" 
                    ? "更新过程中遇到了错误。这可能是由于网络原因或系统权限导致，请尝试前往官网手动下载最新版本。"
                    : (notes || "在这个版本中，我们带来了一些性能优化和体验改进。")}
                </p>
              </div>

              {/* Progress Bar */}
              {(status === "downloading" || status === "ready") && (
                <div className="update-progress-container">
                  <div className="update-progress-labels">
                    <span>{status === "ready" ? "下载完成" : "正在下载更新..."}</span>
                    <span>{Math.round(downloadProgress)}%</span>
                  </div>
                  <div className="update-progress-track">
                    <motion.div 
                      className="update-progress-bar"
                      initial={{ width: 0 }}
                      animate={{ width: `${downloadProgress}%` }}
                      transition={{ type: "spring", damping: 20, stiffness: 100 }}
                    />
                  </div>
                </div>
              )}

              {/* Footer Actions */}
              <div className="update-actions">
                <button
                  onClick={onClose}
                  disabled={status === "downloading"}
                  className="update-btn-secondary"
                >
                  {status === "error" ? "关闭" : "稍后"}
                </button>

                {status === "error" ? (
                  <button
                    onClick={handleOpenWebsite}
                    className="update-btn"
                  >
                    <ExternalLink size={16} />
                    前往官网
                  </button>
                ) : status === "ready" ? (
                  <button
                    onClick={onUpdate}
                    className="update-btn update-btn-success"
                  >
                    <RefreshCw size={18} className="animate-spin-slow" />
                    立即重启
                  </button>
                ) : (
                  <button
                    onClick={onUpdate}
                    disabled={status === "downloading"}
                    className="update-btn"
                  >
                    {status === "downloading" ? (
                      <>
                        <RefreshCw size={18} className="animate-spin" />
                        下载中...
                      </>
                    ) : (
                      <>
                        <Download size={18} />
                        立即更新
                      </>
                    )}
                  </button>
                )}
              </div>
            </div>
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  );
};

export default UpdateDialog;
