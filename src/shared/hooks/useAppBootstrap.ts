import { useEffect } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { DefaultAppsMap, InstalledAppOption } from "../../features/app/types";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface UseAppBootstrapOptions {
  fetchEffectiveTransferPath: () => void;
  setDataPath: Dispatch<SetStateAction<string>>;
  setInstalledApps: Dispatch<SetStateAction<InstalledAppOption[]>>;
  setAutoStart: Dispatch<SetStateAction<boolean>>;
  setWinClipboardDisabled: Dispatch<SetStateAction<boolean>>;
  setDefaultApps: Dispatch<SetStateAction<DefaultAppsMap>>;
  setFileServerEnabled: Dispatch<SetStateAction<boolean>>;
  setActualPort: Dispatch<SetStateAction<string>>;
  setLocalIp: Dispatch<SetStateAction<string>>;
  setAvailableIps: Dispatch<SetStateAction<string[]>>;
}

interface FileServerStatusPayload {
  enabled: boolean;
  port: number;
  ip: string;
}

export const useAppBootstrap = ({
  fetchEffectiveTransferPath,
  setDataPath,
  setInstalledApps,
  setAutoStart,
  setWinClipboardDisabled,
  setDefaultApps,
  setFileServerEnabled,
  setActualPort,
  setLocalIp,
  setAvailableIps
}: UseAppBootstrapOptions) => {
  useEffect(() => {
    fetchEffectiveTransferPath();

    invoke<string>("get_data_path").then(setDataPath).catch(console.error);

    invoke<{ name: string; path: string }[]>("scan_installed_apps")
      .then((apps) => {
        if (apps && apps.length > 0) {
          setInstalledApps(
            apps
              .map((a) => ({ label: a.name, value: a.path }))
              .sort((a, b) => a.label.localeCompare(b.label))
          );
        } else {
          console.warn("No apps found by scan_installed_apps");
        }
      })
      .catch((err) => {
        console.error("Failed to scan apps:", err);
      });

    invoke<boolean>("is_autostart_enabled").then(setAutoStart).catch(console.error);

    invoke<boolean>("get_windows_clipboard_history")
      .then((enabled) => {
        setWinClipboardDisabled(!enabled);
      })
      .catch(console.error);

    const types = ["text", "rich_text", "image", "video", "code", "url"];
    types.forEach(async (type) => {
      try {
        const name = await invoke<string>("get_system_default_app", { contentType: type });
        setDefaultApps((prev) => ({ ...prev, [type]: name }));
      } catch (err) {
        console.error(`Failed to get default for ${type}`, err);
      }
    });

    const setupServerListener = async () => {
      const unlisten = await listen<FileServerStatusPayload>("file-server-status-changed", (event) => {
        const payload = event.payload;
        setFileServerEnabled(payload.enabled);
        setActualPort(payload.port === 0 ? "" : payload.port.toString());
        setLocalIp(payload.ip);
      });
      return unlisten;
    };

    let unlistenServer: (() => void) | undefined;
    setupServerListener().then((u) => {
      unlistenServer = u;
    });

    invoke<FileServerStatusPayload>("get_file_server_status")
      .then((status) => {
        setFileServerEnabled(status.enabled);
        setActualPort(status.port === 0 ? "" : status.port.toString());
        setLocalIp(status.ip);
      })
      .catch(console.error);

    invoke<string[]>("get_available_ips")
      .then((ips) => {
        if (ips && ips.length > 0) setAvailableIps(ips);
      })
      .catch(console.error);

    return () => {
      if (unlistenServer) unlistenServer();
    };
  }, [
    fetchEffectiveTransferPath,
    setActualPort,
    setAutoStart,
    setAvailableIps,
    setDataPath,
    setDefaultApps,
    setFileServerEnabled,
    setInstalledApps,
    setLocalIp,
    setWinClipboardDisabled
  ]);
};
