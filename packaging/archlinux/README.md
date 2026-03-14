# Arch Packaging

This folder contains an Arch Linux `PKGBUILD` for TieZ.

## Build

```bash
cd packaging/archlinux
makepkg -si
```

## Notes

- The package name is `tiez-clipboard-git`.
- It installs the binary as `tiez`.
- It installs a desktop entry and a 128x128 application icon.
- The `PKGBUILD` currently builds from the Git repository URL, which is convenient for repeatable packaging and AUR-style maintenance.
- During `prepare()`, the package normalizes a couple of upstream files for Linux builds: it removes the stale `focusable` window field from `src-tauri/tauri.conf.json`, strips the unused `tauri-plugin-sql` registration that would otherwise pull in `sqlx`, and removes the matching stale `sql:default` capability entry.
- If you want to package uncommitted local changes from your current checkout, adjust the `source`/build flow before running `makepkg`.
