import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface UseMqttListenerOptions {
  enabled: boolean;
  t: (key: string) => string;
}

const normalizeMessage = (message: string) => message.trim().replace(/\s+/g, " ");

const buildNotificationBody = (message: string) => {
  if (message.length <= 64) {
    return message;
  }

  return `${message.slice(0, 61)}...`;
};

const sendNativeNotification = async (title: string, body: string) => {
  await invoke("send_system_notification", { title, body });
};

export const useMqttListener = ({ enabled, t }: UseMqttListenerOptions) => {
  const lastNotificationRef = useRef<{ key: string; timestamp: number } | null>(null);

  useEffect(() => {
    const unlistenMqtt = listen<string>("mqtt-message", async (event) => {
      if (!enabled) return;

      const message = normalizeMessage(event.payload ?? "");
      if (!message) return;

      const notificationKey = message;
      const now = Date.now();
      if (
        lastNotificationRef.current &&
        lastNotificationRef.current.key === notificationKey &&
        now - lastNotificationRef.current.timestamp < 10_000
      ) {
        return;
      }

      lastNotificationRef.current = { key: notificationKey, timestamp: now };
      try {
        await sendNativeNotification(
          t("mqtt_notification_title"),
          buildNotificationBody(message)
        );
      } catch (error) {
        console.error("Failed to send native MQTT notification", error);
      }
    });

    return () => {
      unlistenMqtt.then((f) => f());
    };
  }, [enabled, t]);
};
