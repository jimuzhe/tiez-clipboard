export type AppEdition = "local" | "cloud";

const rawEdition = (import.meta.env.VITE_EDITION ?? "cloud").toLowerCase();

export const APP_EDITION: AppEdition = rawEdition === "local" ? "local" : "cloud";
export const CLOUD_SYNC_ENABLED = APP_EDITION === "cloud";
