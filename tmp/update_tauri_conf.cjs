const fs = require('fs');
const config = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8'));

// 1. Add updater and process permissions
const permissions = config.app.security.capabilities[0].permissions;
if (!permissions.includes('updater:default')) {
  permissions.push('updater:default');
}
if (!permissions.includes('process:allow-relaunch')) {
  permissions.push('process:allow-relaunch');
}

// 2. Add updater plugin if not already there
config.plugins = config.plugins || {};
config.plugins.updater = {
  pubkey: "SET_YOUR_PUBKEY_HERE",
  endpoints: [
    "https://tiez.name666.top/api/checkUpdate?version={{current_version}}"
  ]
};

// 3. Write back with 2 spaces
fs.writeFileSync('src-tauri/tauri.conf.json', JSON.stringify(config, null, 2));
