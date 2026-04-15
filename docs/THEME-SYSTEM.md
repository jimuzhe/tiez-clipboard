# Theme System

## Goal

The app theme system now follows one rule:

- shared CSS defines structure and neutral defaults
- each theme file owns its visual language

This avoids the previous problem where theme switching only changed colors while the UI still kept the same retro 3D component shapes.

## Files That Matter

### Theme registry

- `src/shared/config/themes.ts`

This is the single source of truth for:

- theme id
- display labels
- feature flags such as custom background and surface opacity

### Shared style layer

- `src/styles/base.css`
- `src/styles/components/*.css`

This layer should contain:

- layout
- spacing
- component structure
- neutral interaction behavior
- semantic theme tokens

This layer should not contain:

- one theme's identity hardcoded as the global default
- theme-specific `!important` patches unless there is no better option

### Theme files

- `src/styles/themes/retro.css`
- `src/styles/themes/mica.css`
- `src/styles/themes/acrylic.css`
- `src/styles/themes/sticky-note.css`

Each theme file is now independent and is responsible for:

- theme tokens
- component-level visual overrides that cannot be expressed by tokens alone
- light and dark variants if needed

### Theme loader

- `src/styles/themes/load.ts`

`load.ts` imports:

- `index.css`
- `dark.css`
- every other `src/styles/themes/*.css` file automatically

That means a normal pure-CSS theme does not need loader edits anymore.

## Runtime Contract

The runtime always normalizes theme ids through `src/shared/config/themes.ts`.

Rules:

- only registered theme ids are applied
- unknown ids fall back to `DEFAULT_THEME`
- the same theme id is applied to both the main window and compact preview window
- active classes are always `theme-<id>` on both `html` and `body`

## Layer Model

### 1. Base layer

Base layer responsibilities:

- define shell structure
- define default control layout
- define semantic tokens used by components

Examples now living in shared CSS:

- window shell
- header layout
- search input structure
- history card structure
- settings panel layout
- modal layout
- tag dropdown layout

### 2. Theme layer

Theme layer responsibilities:

- colors
- border language
- radius language
- shadow language
- typography style
- special material effects

Examples:

- `retro.css` restores hard borders, pressed buttons, segmented mechanical switch, uppercase labels
- `mica.css` keeps the UI soft and translucent, with noise texture but no blur shell
- `acrylic.css` adds glass blur and highlight overlays

### 3. Native layer

Files:

- `src-tauri/src/app/commands/ui_cmd.rs`

Use this layer only when the theme needs platform-native window behavior.

Examples:

- mica
- acrylic

Pure CSS themes do not need backend changes.

## Semantic Token Contract

Shared components now read semantic tokens instead of assuming retro visuals.

### Core shell tokens

- `--shell-background`
- `--shell-border`
- `--shell-shadow`
- `--shell-radius`
- `--shell-backdrop-filter`

### Header and title tokens

- `--toolbar-border`
- `--toolbar-shadow`
- `--title-background`
- `--title-color`
- `--title-border`
- `--title-radius`
- `--title-shadow`
- `--title-padding`
- `--title-transform`
- `--title-letter-spacing`
- `--title-font-size`
- `--title-font-weight`
- `--title-font-family`

### Input tokens

- `--input-border`
- `--input-radius`
- `--input-shadow`
- `--input-focus-border-color`
- `--input-focus-shadow`
- `--input-font-weight`

### Button tokens

- `--button-border`
- `--button-radius`
- `--button-shadow`
- `--button-hover-border-color`
- `--button-hover-shadow`
- `--button-hover-transform`
- `--button-active-shadow`
- `--button-active-transform`
- `--button-active-filled-background`
- `--button-active-filled-color`
- `--button-active-filled-border-color`
- `--button-active-filled-shadow`

### Card tokens

- `--card-border`
- `--card-radius`
- `--card-shadow`
- `--card-hover-border-color`
- `--card-hover-shadow`
- `--card-hover-transform`
- `--card-active-shadow`
- `--card-active-transform`
- `--card-selected-background`
- `--card-selected-border-color`
- `--card-selected-shadow`
- `--card-selected-outline`
- `--card-selected-outline-offset`

### Panel and settings tokens

- `--panel-border`
- `--panel-radius`
- `--panel-shadow`
- `--panel-header-background`
- `--panel-header-border-color`
- `--panel-divider-color`
- `--data-panel-border`
- `--data-panel-radius`
- `--data-panel-shadow`
- `--data-panel-hover-background`

### Slider, switch, and segmented control tokens

- `--range-track-border`
- `--range-track-radius`
- `--range-track-shadow`
- `--range-thumb-border`
- `--range-thumb-radius`
- `--range-thumb-shadow`
- `--range-thumb-active-transform`
- `--range-thumb-active-shadow`
- `--switch-track-background`
- `--switch-track-border`
- `--switch-track-radius`
- `--switch-track-shadow`
- `--switch-track-active-background`
- `--switch-track-active-border-color`
- `--switch-thumb-background`
- `--switch-thumb-border`
- `--switch-thumb-shadow`
- `--switch-thumb-size`
- `--switch-thumb-offset`
- `--switch-thumb-translate`
- `--segmented-border`
- `--segmented-radius`
- `--segmented-shadow`
- `--segment-radius`
- `--segment-font-weight`
- `--segment-text-transform`
- `--segment-active-background`
- `--segment-active-color`
- `--segment-active-shadow`

### Modal, tags, and menu tokens

- `--modal-border`
- `--modal-radius`
- `--modal-shadow`
- `--modal-title-background`
- `--modal-title-color`
- `--modal-title-border`
- `--modal-title-shadow`
- `--modal-title-padding`
- `--modal-title-transform`
- `--modal-title-letter-spacing`
- `--modal-title-font-weight`
- `--dialog-button-border`
- `--dialog-button-radius`
- `--dialog-button-shadow`
- `--dialog-button-hover-shadow`
- `--dialog-button-hover-transform`
- `--dialog-button-active-shadow`
- `--dialog-button-active-transform`
- `--tags-border`
- `--tags-radius`
- `--tags-shadow`
- `--tag-chip-border`
- `--tag-chip-radius`
- `--tag-chip-shadow`
- `--tag-chip-hover-shadow`
- `--tag-chip-hover-transform`
- `--tag-chip-active-shadow`
- `--tag-chip-active-transform`
- `--menu-border`
- `--menu-radius`
- `--menu-shadow`
- `--menu-item-hover-background`
- `--menu-item-hover-color`

### Typography and auxiliary tokens

- `--content-font-family`
- `--content-line-height`
- `--meta-text-transform`
- `--meta-letter-spacing`
- `--empty-state-font-family`
- `--empty-state-text-transform`
- `--queue-border`
- `--queue-radius`
- `--queue-shadow`
- `--queue-count-border`
- `--queue-count-shadow`
- `--queue-count-rotate`
- `--queue-action-border`
- `--queue-action-shadow`
- `--queue-button-text-transform`
- `--reset-button-border`
- `--reset-button-radius`
- `--reset-button-shadow`
- `--reset-button-text-transform`

## Authoring Rules

### Rule 1: shared CSS must stay theme-neutral

If the value describes one theme's personality, it does not belong in the shared layer.

Examples that should stay out of shared CSS:

- hard 3D push-down transforms
- retro uppercase title chip
- acrylic blur overlay
- sticky note tape decoration

### Rule 2: use tokens first

If the difference is just shape, shadow, color, spacing, or typography, use tokens.

Good:

```css
body.theme-example {
  --card-radius: 20px;
  --button-shadow: 0 10px 24px rgba(0, 0, 0, 0.12);
}
```

Only use selectors when structure really changes.

Valid selector override example:

```css
.theme-retro .toggle::before {
  display: none;
}

.theme-retro .toggle > .left,
.theme-retro .toggle > .right {
  display: flex;
}
```

### Rule 3: theme files own their special effects

Examples:

- paper texture belongs in `paper.css`
- mica noise layer belongs in `mica.css`
- acrylic blur highlight belongs in `acrylic.css`
- retro mechanical switch belongs in `retro.css`

### Rule 4: labels live in `themes.ts`

Do not add theme names to `src/locales.ts`.

Theme labels belong in:

- `src/shared/config/themes.ts`

## Adding A New Pure CSS Theme

### Step 1. Register it

Edit:

- `src/shared/config/themes.ts`

Example:

```ts
{
  id: "paper",
  labels: {
    zh: "纸页",
    en: "Paper",
    tw: "紙頁"
  }
}
```

### Step 2. Create the theme file

Create:

- `src/styles/themes/paper.css`

Start with token overrides:

```css
:root.theme-paper,
body.theme-paper {
  --bg-window: #f7f1e6;
  --bg-content: linear-gradient(180deg, #fbf5ea 0%, #efe5d6 100%);
  --text-primary: #43352a;
  --text-secondary: #7a6556;
  --accent-color: #a55337;
  --accent-color-rgb: 165, 83, 55;
  --shell-border: 1px solid rgba(111, 90, 73, 0.24);
  --card-radius: 6px;
}
```

### Step 3. Add selector overrides only when necessary

Example:

```css
body.theme-paper #root {
  background-image:
    linear-gradient(180deg, rgba(255, 255, 255, 0.38), rgba(244, 232, 214, 0.18)),
    repeating-linear-gradient(180deg, transparent, transparent 27px, rgba(153, 122, 101, 0.06) 28px);
}
```

### Step 4. Build

Run:

```bash
npm run build
```

## Adding A Native Theme

If the theme needs OS-native material behavior:

1. register the theme in `src/shared/config/themes.ts`
2. create `src/styles/themes/<id>.css`
3. extend `set_theme` in `src-tauri/src/app/commands/ui_cmd.rs`

Only do this for themes that truly need native window materials.

## Review Checklist

Before merging a theme change, verify:

- the theme is registered in `src/shared/config/themes.ts`
- the stylesheet exists in `src/styles/themes/`
- the theme appears in settings without locale file edits
- the main window switches correctly
- the compact preview window switches correctly
- header, buttons, inputs, list cards, settings panels, modals, and tags all follow the theme
- hover, active, selected, and focus states remain readable
- light and dark variants remain readable
- `npm run build` passes

## Current Architecture Outcome

This repository now uses:

- neutral shared component CSS
- `retro.css` for the old 3D/mechanical style
- `mica.css` for soft translucent material
- `acrylic.css` for blur glass material
- `sticky-note.css` for the note-style theme

That is the foundation for adding future themes without rewriting multiple unrelated files every time.

## Remaining Debt

The main shared UI layer has been normalized, but a few feature-local styles still carry direct visual assumptions and should be migrated into the same token contract in a follow-up pass:

- `src/styles/components/Announcement.css`
- `src/styles/components/compact-mode.css`
- `src/styles/components/emoji.css`
- `src/styles/components/file-transfer.css`

These files are now the exception, not the base theme system.
