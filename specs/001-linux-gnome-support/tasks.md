# Tasks: Linux GNOME Support

**Input**: Design documents from `/specs/001-linux-gnome-support/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/platform-interface.md

**Tests**: Each user story phase includes a Testing Specification section with manual test instructions, test standards derived from spec.md acceptance scenarios, and result recording format.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

- **Project type**: Desktop application (Tauri)
- **Backend**: `src-tauri/src/`
- **Frontend**: `src/` (TypeScript/React - minimal changes needed)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Linux build configuration and dependency setup

- [X] T001 Add Linux-specific dependencies to src-tauri/Cargo.toml (x11-clipboard, xdg-activation, xdg-dialog)
- [X] T002 [P] Update src-tauri/tauri.conf.json to remove Windows-specific path scopes and add Linux-compatible paths
- [X] T003 [P] Create src-tauri/src/infrastructure/linux_api/mod.rs module structure

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core Linux infrastructure that MUST be complete before ANY user story can be implemented

**CRITICAL**: No user story work can begin until this phase is complete

- [X] T004 Create DisplayServer enum and detection function in src-tauri/src/infrastructure/linux_api/mod.rs
- [X] T005 [P] Update src-tauri/src/infrastructure/mod.rs to conditionally include linux_api module with `#[cfg(target_os = "linux")]`
- [X] T006 [P] Replace Windows-only fallback stubs in src-tauri/src/infrastructure/mod.rs with Linux implementations
- [X] T007 Update src-tauri/src/app/commands/ui_cmd.rs to add Linux platform detection in get_platform_info()
- [X] T008 Verify application compiles on Linux with `cargo build --manifest-path src-tauri/Cargo.toml`

**Note**: T008 blocked by system dependencies - requires `libglib2.0-dev`, `libgtk-3-dev`, etc. Run: `sudo apt install libglib2.0-dev libgtk-3-dev libwebkit2gtk-4.1-dev libappindicator3-1`

**Checkpoint**: Foundation ready - application compiles on Linux, user story implementation can begin

---

## Phase 3: User Story 1 - Basic Clipboard Functionality (Priority: P1)

**Goal**: Core clipboard capture (text, images, files) and paste functionality on Ubuntu

**Independent Test**: Copy various content types (text, images, files) on Ubuntu and verify they appear in clipboard history within 1 second and can be pasted successfully

### Testing Specification for User Story 1

> **Instructions**: After completing implementation tasks, follow these test procedures and record results.

#### Test Instructions

| Step | Action | Expected Result |
|------|--------|-----------------|
| 1    | Start TieZ on Ubuntu | Application launches without errors |
| 2    | Copy text from any application (e.g., Firefox, Terminal) | Text appears in clipboard history within 1 second |
| 3    | Copy an image from browser or screenshot tool | Image appears in clipboard history with thumbnail preview |
| 4    | Copy files from file manager (Nautilus) | Files appear in clipboard history |
| 5    | Click any clipboard entry | Content is pasted to system clipboard and active application |

#### Test Standards (from Acceptance Scenarios)

| Scenario ID | Description | Pass Criteria |
|-------------|-------------|---------------|
| US1-AC1 | Given TieZ is running on Ubuntu, When user copies text from any application, Then text appears in clipboard history within 1 second | Text visible in history ≤ 1s after copy |
| US1-AC2 | Given TieZ is running on Ubuntu, When user copies an image from a browser, Then image appears in clipboard history with thumbnail preview | Image thumbnail visible in history |
| US1-AC3 | Given TieZ is running on Ubuntu, When user copies files from file manager, Then files appear in clipboard history and can be pasted to destination | Files visible in history, paste works |
| US1-AC4 | Given Clipboard history has multiple entries, When user clicks any entry, Then content is pasted to system clipboard and active application | Clicked content available for paste |

#### Test Results Recording

| Scenario ID | Test Date | Result (Pass/Fail) | Notes |
|-------------|-----------|-------------------|-------|
| US1-AC1 | 2026年3月24日 | Pass | None |
| US1-AC2 | 2026年3月24日 | Fail | cannot show thumbnail, cannot detect copy image action in web browser |
| US1-AC3 | 2026年3月24日 | Fail | cannot show thumbnail|
| US1-AC4 | 2026年3月24日 | Partly Pass | Next user story will be fix |


### Implementation for User Story 1

- [x] T009 [US1] Implement X11 clipboard event monitoring in src-tauri/src/infrastructure/linux_api/win_clipboard.rs
- [x] T010 [P] [US1] Implement get_clipboard_image() using arboard in src-tauri/src/infrastructure/linux_api/win_clipboard.rs
- [x] T011 [P] [US1] Implement get_clipboard_files() for file URI parsing in src-tauri/src/infrastructure/linux_api/win_clipboard.rs
- [x] T012 [P] [US1] Implement set_clipboard_files() in src-tauri/src/infrastructure/linux_api/win_clipboard.rs
- [x] T013 [P] [US1] Implement set_clipboard_text_and_html() in src-tauri/src/infrastructure/linux_api/win_clipboard.rs
- [x] T014 [P] [US1] Implement set_clipboard_image_with_formats() in src-tauri/src/infrastructure/linux_api/win_clipboard.rs
- [x] T015 [US1] Update src-tauri/src/services/clipboard_listener.rs to use X11 events on Linux X11 sessions
- [x] T016 [US1] Add Wayland polling fallback in src-tauri/src/services/clipboard_listener.rs for Wayland sessions
- [x] T017 [US1] Test clipboard capture manually per quickstart.md section 3.1-3.3

**Checkpoint**: Basic clipboard functionality works - text, images, and files can be captured and pasted on Ubuntu

---

## Phase 4: User Story 2 - Source Application Detection (Priority: P2)

**Goal**: Identify and display source application name for each clipboard entry

**Independent Test**: Copy content from Firefox, GNOME Terminal, and VS Code, verify correct app names appear in clipboard history

### Testing Specification for User Story 2

> **Instructions**: After completing implementation tasks, follow these test procedures and record results.

#### Test Instructions

| Step | Action | Expected Result |
|------|--------|-----------------|
| 1    | Start TieZ on Ubuntu with X11 session | Application launches, window tracking active |
| 2    | Copy text from Firefox | Clipboard entry shows source as "Firefox" |
| 3    | Copy text from GNOME Terminal | Clipboard entry shows source as "gnome-terminal" |
| 4    | Copy text from VS Code | Clipboard entry shows source as "code" |
| 5    | Repeat on Wayland session | Source shows as "Unknown" (graceful fallback) |

#### Test Standards (from Acceptance Scenarios)

| Scenario ID | Description | Pass Criteria |
|-------------|-------------|---------------|
| US2-AC1 | Given TieZ is running on Ubuntu, When user copies text from Firefox, Then clipboard entry shows source as "Firefox" | Source field displays "Firefox" |
| US2-AC2 | Given TieZ is running on Ubuntu, When user copies text from GNOME Terminal, Then clipboard entry shows source as "gnome-terminal" | Source field displays "gnome-terminal" |
| US2-AC3 | Given TieZ is running on Ubuntu, When user copies text from VS Code, Then clipboard entry shows source as "code" | Source field displays "code" |

#### Test Results Recording

| Scenario ID | Test Date | Result (Pass/Fail) | Notes |
|-------------|-----------|-------------------|-------|
| US2-AC1 | 2026年3月24日 | Pass | None |
| US2-AC2 | 2026年3月24日 | Pass | None |
| US2-AC3 | 2026年3月24日 | Pass | None |

### Implementation for User Story 2

- [X] T018 [US2] Implement X11 active window detection via _NET_ACTIVE_WINDOW in src-tauri/src/infrastructure/linux_api/window_tracker.rs
- [X] T019 [P] [US2] Implement get_active_app_info() extracting WM_CLASS property in src-tauri/src/infrastructure/linux_api/window_tracker.rs
- [X] T020 [P] [US2] Implement get_clipboard_source_app_info() in src-tauri/src/infrastructure/linux_api/window_tracker.rs
- [X] T021 [US2] Implement start_window_tracking() with X11 event loop in src-tauri/src/infrastructure/linux_api/window_tracker.rs
- [X] T022 [US2] Add Wayland fallback returning "Unknown" app info in src-tauri/src/infrastructure/linux_api/window_tracker.rs
- [x] T023 [US2] Test source app detection manually per quickstart.md section 3.4

**Checkpoint**: Source application names display correctly for clipboard entries on X11, gracefully falls back to "Unknown" on Wayland

---

## Phase 5: User Story 3 - File Opening and System Integration (Priority: P2)

**Goal**: Open files with system default applications, support XDG autostart

**Independent Test**: Copy file path, click to open, verify system default application launches; enable autostart and verify TieZ launches on login

### Testing Specification for User Story 3

> **Instructions**: After completing implementation tasks, follow these test procedures and record results.

#### Test Instructions

| Step | Action | Expected Result |
|------|--------|-----------------|
| 1    | Copy a file path to clipboard | File path appears in clipboard history |
| 2    | Click to open the file | File opens in system default application |
| 3    | Copy a folder path to clipboard | Folder path appears in clipboard history |
| 4    | Click to open the folder | GNOME Files (Nautilus) opens to that location |
| 5    | Enable autostart in settings | ~/.config/autostart/tiez.desktop file created |
| 6    | Log out and log back in | TieZ launches automatically on login |

#### Test Standards (from Acceptance Scenarios)

| Scenario ID | Description | Pass Criteria |
|-------------|-------------|---------------|
| US3-AC1 | Given Clipboard entry contains a file path, When user clicks to open the file, Then file opens in the system default application for that type | Default app launches with correct file |
| US3-AC2 | Given Clipboard entry contains a folder path, When user clicks to open the folder, Then GNOME Files (Nautilus) opens to that location | Nautilus opens at correct path |
| US3-AC3 | Given User enables autostart in settings, When user logs into GNOME session, Then TieZ launches automatically | TieZ running after login without manual start |

#### Test Results Recording

| Scenario ID | Test Date | Result (Pass/Fail) | Notes |
|-------------|-----------|-------------------|-------|
| US3-AC1 | 2026年3月24日 | Pass | none |
| US3-AC2 | 2026年3月24日 | Pass | none |
| US3-AC3 | 2026年3月24日 | Pass | works fine but if packaged this app will point correct path? |

### Implementation for User Story 3

- [x] T024 [P] [US3] Implement open_file_or_url() using xdg-open in src-tauri/src/infrastructure/linux_api/desktop_integration.rs
- [x] T025 [P] [US3] Implement toggle_autostart() using XDG autostart spec in src-tauri/src/infrastructure/linux_api/desktop_integration.rs
- [x] T026 [US3] Update src-tauri/src/app/commands/system_cmd.rs open_folder command to use xdg-open on Linux
- [x] T027 [US3] Update src-tauri/src/services/file_transfer/mod.rs register_received_file to use xdg-open for auto-open on Linux
- [x] T028 [US3] Test file opening manually per quickstart.md section 3.5
- [x] T029 [US3] Test autostart manually per quickstart.md section 3.6

**Checkpoint**: Files open with system default applications, autostart works via XDG specification


---

## Phase 6: User Story 4 - System Tray Integration (Priority: P3)

**Goal**: Tray icon visible in GNOME top bar with context menu

**Independent Test**: Verify tray icon appears in GNOME top bar (with AppIndicator extension), right-click shows menu options

### Testing Specification for User Story 4

> **Instructions**: After completing implementation tasks, follow these test procedures and record results. Note: Requires AppIndicator GNOME Shell extension installed.

#### Test Instructions

| Step | Action | Expected Result |
|------|--------|-----------------|
| 1    | Start TieZ on Ubuntu with AppIndicator extension | Tray icon appears in GNOME top bar |
| 2    | Right-click the tray icon | Context menu appears |
| 3    | Verify menu options | Show/Hide, Settings, and Quit options visible |
| 4    | Click "Show" option | Main window appears |
| 5    | Click "Quit" option | Application exits cleanly |

#### Test Standards (from Acceptance Scenarios)

| Scenario ID | Description | Pass Criteria |
|-------------|-------------|---------------|
| US4-AC1 | Given TieZ is running on Ubuntu, When user looks at the top bar, Then TieZ tray icon is visible (with AppIndicator extension) | Icon visible in tray area |
| US4-AC2 | Given Tray icon is visible, When user right-clicks the tray icon, Then context menu shows Show/Hide, Settings, and Quit options | All three options present in menu |

#### Test Results Recording

| Scenario ID | Test Date | Result (Pass/Fail) | Notes |
|-------------|-----------|-------------------|-------|
| US4-AC1 | 2026年3月25日 | Pass | None |
| US4-AC2 | 2026年3月25日 | Pass | None |

### Implementation for User Story 4

- [x] T030 [US4] Verify Tauri tray-icon feature is enabled in src-tauri/Cargo.toml (already present via tray-icon feature)
- [x] T031 [US4] Test tray icon visibility on Ubuntu with AppIndicator extension
- [x] T032 [US4] Verify tray context menu shows Show/Hide, Settings, and Quit options

**Checkpoint**: System tray icon works on Ubuntu with AppIndicator extension

---
### Known Issues (Linux) - Discovered During Testing

- [ ] T017A [US1] Fix paste not working when window is pinned (pin icon clicked)
- [ ] T017B [US1] Fix window not staying on top when pinned (other windows can cover it)
- [x] T017C [US1] Fix window disappearing when moved while not pinned
- [ ] T017D [US1] Extend draggable area to entire title bar (including button area)

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Final validation and cleanup across all user stories

### Testing Specification for Polish Phase

> **Instructions**: After completing all user story phases, perform these cross-cutting tests.

#### Cross-Cutting Test Instructions

| Step | Action | Expected Result |
|------|--------|-----------------|
| 1    | Verify all user story tests pass | All scenarios in US1-US4 marked Pass |
| 2    | Test on X11 session | All features work correctly |
| 3    | Test on Wayland session | Graceful fallbacks, no crashes |
| 4    | Run quickstart.md validation | All steps complete successfully |
| 5    | Verify constitution compliance | No violations found |
| 6    | Test edge cases | Graceful handling, no crashes |
| 7    | Final build test | npm run tauri:build succeeds |

#### Cross-Cutting Test Standards (from Success Criteria)

| Standard ID | Description | Pass Criteria |
|-------------|-------------|---------------|
| CC-001 | Application compiles on Ubuntu 24.04 LTS | Build succeeds without errors |
| CC-002 | Clipboard capture latency | Content captured within 1 second |
| CC-003 | Source app accuracy | 90% accuracy for common apps |
| CC-004 | File opening success | Standard file types open correctly |
| CC-005 | Feature parity with macOS | Core clipboard functionality equivalent |
| CC-006 | Constitution compliance | No emojis in comments, correct platform gates |

#### Edge Case Testing

| Edge Case | Description | Expected Behavior |
|-----------|-------------|-------------------|
| EC-001 | KDE Plasma session | Application runs, may have reduced integration |
| EC-002 | Wayland-only application clipboard | Content captured, source shows "Unknown" |
| EC-003 | Large clipboard content | Graceful handling, no memory crash |
| EC-004 | Clipboard changes while TieZ not running | Changes detected on restart |
| EC-005 | No active window detectable | Source shows "Unknown", no crash |

#### Test Results Recording

| Standard/Edge ID | Test Date | Result (Pass/Fail) | Notes |
|------------------|-----------|-------------------|-------|
| CC-001 | | | |
| CC-002 | | | |
| CC-003 | | | |
| CC-004 | | | |
| CC-005 | | | |
| CC-006 | | | |
| EC-001 | | | |
| EC-002 | | | |
| EC-003 | | | |
| EC-004 | | | |
| EC-005 | | | |

### Implementation Tasks

- [ ] T033 Test both X11 and Wayland sessions per quickstart.md section 3.7
- [ ] T034 [P] Run full quickstart.md manual testing checklist
- [ ] T035 [P] Verify constitution compliance (no emojis in comments, platform gates correct)
- [ ] T036 Final compilation test: npm run tauri:build on Ubuntu 24.04 LTS

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-6)**: All depend on Foundational phase completion
  - US1 (P1): Can start after Phase 2
  - US2 (P2): Can start after Phase 2 (independent of US1)
  - US3 (P2): Can start after Phase 2 (independent of US1, US2)
  - US4 (P3): Can start after Phase 2 (independent of all others)
- **Polish (Phase 7)**: Depends on desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: No dependencies on other stories - MVP candidate
- **User Story 2 (P2)**: Independent - can test source app detection standalone
- **User Story 3 (P2)**: Independent - can test file opening/autostart standalone
- **User Story 4 (P3)**: Independent - tray functionality separate from clipboard

### Within Each User Story

- X11 implementations before Wayland fallbacks
- Core functions before utility functions
- Implementation before manual testing

### Parallel Opportunities

- T002, T003 can run in parallel (different files)
- T005, T006 can run in parallel (different sections of mod.rs)
- T010, T011, T012, T013, T014 can run in parallel (independent clipboard functions)
- T019, T020 can run in parallel (independent window tracker functions)
- T024, T025 can run in parallel (independent desktop integration functions)
- T034, T035 can run in parallel (independent validation tasks)

---

## Parallel Example: User Story 1

```bash
# Launch all independent clipboard functions together:
Task: "Implement get_clipboard_image() using arboard in src-tauri/src/infrastructure/linux_api/win_clipboard.rs"
Task: "Implement get_clipboard_files() for file URI parsing in src-tauri/src/infrastructure/linux_api/win_clipboard.rs"
Task: "Implement set_clipboard_files() in src-tauri/src/infrastructure/linux_api/win_clipboard.rs"
Task: "Implement set_clipboard_text_and_html() in src-tauri/src/infrastructure/linux_api/win_clipboard.rs"
Task: "Implement set_clipboard_image_with_formats() in src-tauri/src/infrastructure/linux_api/win_clipboard.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: Test clipboard functionality independently per quickstart.md
5. Deploy/demo basic Linux clipboard manager

### Incremental Delivery

1. Complete Setup + Foundational → Application compiles on Linux
2. Add User Story 1 → Test independently → Deploy/Demo (MVP!)
3. Add User Story 2 → Test independently → Enhanced UX with source apps
4. Add User Story 3 → Test independently → Full desktop integration
5. Add User Story 4 → Test independently → Complete GNOME experience
6. Each story adds value without breaking previous stories

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together
2. Once Foundational is done:
   - Developer A: User Story 1 (clipboard - highest priority)
   - Developer B: User Story 2 (window tracking)
   - Developer C: User Story 3 (desktop integration)
3. Stories complete and integrate independently

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Manual testing via quickstart.md validates each user story

## Constitution Compliance

Per `.specify/memory/constitution.md`, implementations must:
- Preserve backward compatibility (no breaking changes to Windows/mac code)
- Use simplest viable approach (direct platform modules, no over-abstraction)
- Gate platform-specific code with `#[cfg(target_os = "...")]`
- Add comments only before complex logic (no emojis, no line-by-line commenting)
- Not generate markdown documentation after task completion (brief summary only)

## Summary

| Metric | Value |
|--------|-------|
| Total Tasks | 36 |
| Setup (Phase 1) | 3 |
| Foundational (Phase 2) | 5 |
| User Story 1 (P1) | 9 |
| User Story 2 (P2) | 6 |
| User Story 3 (P2) | 6 |
| User Story 4 (P3) | 3 |
| Polish (Phase 7) | 4 |
| Parallel Opportunities | 15 tasks marked [P] |
| MVP Scope | User Story 1 (9 tasks) |
