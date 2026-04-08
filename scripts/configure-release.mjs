import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const isCi = process.env.GITHUB_ACTIONS === "true" || process.env.CI === "true";

const requiredInCi = [
  "TIEZ_UPDATE_ENDPOINT",
  "TIEZ_ANNOUNCEMENT_PING_URL",
  "VITE_AI_DEFAULT_API_KEY"
];

for (const key of requiredInCi) {
  if (isCi && !process.env[key]?.trim()) {
    console.error(`[configure-release] Missing required env: ${key}`);
    process.exit(1);
  }
}

const configPath = resolve("src-tauri/tauri.conf.json");
const config = JSON.parse(readFileSync(configPath, "utf8"));

const updateEndpoint =
  process.env.TIEZ_UPDATE_ENDPOINT?.trim() ||
  process.env.VITE_TIEZ_UPDATE_ENDPOINT?.trim();
const updaterPublicKey =
  process.env.TIEZ_UPDATER_PUBLIC_KEY?.trim() ||
  config.plugins?.updater?.pubkey?.trim();

if (updateEndpoint) {
  config.plugins ??= {};
  config.plugins.updater ??= {};
  config.plugins.updater.endpoints = [updateEndpoint];
}

if (updaterPublicKey) {
  config.plugins ??= {};
  config.plugins.updater ??= {};
  config.plugins.updater.pubkey = updaterPublicKey;
}

writeFileSync(configPath, `${JSON.stringify(config, null, 2)}\n`);

console.log("[configure-release] Updated Tauri release config");
if (updateEndpoint) {
  console.log(`[configure-release] updater endpoint: ${updateEndpoint}`);
}
if (updaterPublicKey) {
  console.log("[configure-release] updater public key: available");
}
