import { useState, useEffect, useCallback } from "react";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { isTauriRuntime } from "../lib/tauriRuntime";

export type UpdateStatus = "idle" | "checking" | "downloading" | "ready" | "error";

export const useAutoUpdate = () => {
  const [isOpen, setIsOpen] = useState(false);
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [version, setVersion] = useState("");
  const [notes, setNotes] = useState("");
  const [downloadProgress, setDownloadProgress] = useState(0);
  const [updateObj, setUpdateObj] = useState<Update | null>(null);

  const checkUpdate = useCallback(async () => {
    if (!isTauriRuntime()) return;
    
    try {
      setStatus("checking");
      
      // Use the native check() which handles RID allocation internally
      const update = await check({
        proxy: undefined, // Or configure if needed
        headers: { "Cache-Control": "no-cache" },
        timeout: 10000
      });

      if (update) {
        console.log(`[Update] New version detected: ${update.version}`);
        setUpdateObj(update);
        setVersion(update.version);
        setNotes(update.body || "");
        setIsOpen(true);
      }
      
      setStatus("idle");
    } catch (error) {
      console.error("[Update] Failed to check for updates:", error);
      setStatus("error");
    }
  }, []);

  const startUpdate = async () => {
    if (!updateObj) return;

    try {
      setStatus("downloading");
      setDownloadProgress(0);

      // 4. Use the native plugin logic to download and install
      await updateObj.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            console.log("[Update] Download started");
            break;
          case "Progress":
            // Calculate progress based on bytes if available, or incremental simulation
            setDownloadProgress((prev) => Math.min(prev + 5, 99)); 
            break;
          case "Finished":
            console.log("[Update] Download finished");
            setDownloadProgress(100);
            setStatus("ready");
            break;
        }
      });
      
      setStatus("ready");
      setDownloadProgress(100);
    } catch (error) {
      console.error("[Update] Failed to download or install update:", error);
      setStatus("error");
    }
  };

  const applyUpdate = async () => {
    try {
      await relaunch();
    } catch (error) {
      console.error("[Update] Failed to relaunch:", error);
    }
  };

  useEffect(() => {
    const timer = setTimeout(() => {
      checkUpdate();
    }, 5000);

    const setupListener = async () => {
      if (isTauriRuntime()) {
        const { listen } = await import('@tauri-apps/api/event');
        return listen("check-update-manually", () => {
          checkUpdate();
        });
      }
      return () => {};
    };

    let unlisten: (() => void) | undefined;
    setupListener().then(fn => { unlisten = fn; });
    
    return () => {
      clearTimeout(timer);
      if (unlisten) unlisten();
    };
  }, [checkUpdate]);

  return {
    isOpen,
    status,
    version,
    notes,
    downloadProgress,
    onManualUpdate: checkUpdate,
    onStartDownload: startUpdate,
    onApplyUpdate: applyUpdate,
    onClose: () => setIsOpen(false),
  };
};
