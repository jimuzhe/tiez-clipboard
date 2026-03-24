import { useEffect } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { DefaultAppsMap, InstalledAppOption } from "../../features/app/types";
import { invoke } from "@tauri-apps/api/core";

interface UseAppBootstrapOptions {
  setDataPath: Dispatch<SetStateAction<string>>;
  setInstalledApps: Dispatch<SetStateAction<InstalledAppOption[]>>;
  setAutoStart: Dispatch<SetStateAction<boolean>>;
  setWinClipboardDisabled: Dispatch<SetStateAction<boolean>>;
  setDefaultApps: Dispatch<SetStateAction<DefaultAppsMap>>;
}

export const useAppBootstrap = ({
  setDataPath,
  setInstalledApps,
  setAutoStart,
  setWinClipboardDisabled,
  setDefaultApps
}: UseAppBootstrapOptions) => {
  useEffect(() => {
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

    const types = ["text", "image", "video", "code", "url"];
    types.forEach(async (type) => {
      try {
        const name = await invoke<string>("get_system_default_app", { contentType: type });
        setDefaultApps((prev) => ({ ...prev, [type]: name }));
      } catch (err) {
        console.error(`Failed to get default for ${type}`, err);
      }
    });

    return;
  }, [
    setAutoStart,
    setDataPath,
    setDefaultApps,
    setInstalledApps,
    setWinClipboardDisabled
  ]);
};
