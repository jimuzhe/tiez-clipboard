# Tasks: Linux GNOME Support

**Input**: Design documents from `/specs/001-linux-gnome-support/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/platform-interface.md

**Tests**: No tests were explicitly requested in the feature specification.

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

- [ ] T004 Create DisplayServer enum and detection function in src-tauri/src/infrastructure/linux_api/mod.rs
- [ ] T005 [P] Update src-tauri/src/infrastructure/mod.rs to conditionally include linux_api module with `#[cfg(target_os = "linux")]`
- [ ] T006 [P] Replace Windows-only fallback stubs in src-tauri/src/infrastructure/mod.rs with Linux implementations
- [ ] T007 Update src-tauri/src/app/commands/ui_cmd.rs to add Linux platform detection in get_platform_info()
- [ ] T008 Verify application compiles on Linux with `cargo build --manifest-path src-tauri/Cargo.toml`

**Checkpoint**: Foundation ready - application compiles on Linux, user story implementation can begin

---

## Phase 3: User Story 1 - Basic Clipboard Functionality (Priority: P1)

**Goal**: Core clipboard capture (text, images, files) and paste functionality on Ubuntu

**Independent Test**: Copy various content types (text, images, files) on Ubuntu and verify they appear in clipboard history within 1 second and can be pasted successfully

### Implementation for User Story 1

- [ ] T009 [US1] Implement X11 clipboard event monitoring in src-tauri/src/infrastructure/linux_api/win_clipboard.rs
- [ ] T010 [P] [US1] Implement get_clipboard_image() using arboard in src-tauri/src/infrastructure/linux_api/win_clipboard.rs
- [ ] T011 [P] [US1] Implement get_clipboard_files() for file URI parsing in src-tauri/src/infrastructure/linux_api/win_clipboard.rs
- [ ] T012 [P] [US1] Implement set_clipboard_files() in src-tauri/src/infrastructure/linux_api/win_clipboard.rs
- [ ] T013 [P] [US1] Implement set_clipboard_text_and_html() in src-tauri/src/infrastructure/linux_api/win_clipboard.rs
- [ ] T014 [P] [US1] Implement set_clipboard_image_with_formats() in src-tauri/src/infrastructure/linux_api/win_clipboard.rs
- [ ] T015 [US1] Update src-tauri/src/services/clipboard_listener.rs to use X11 events on Linux X11 sessions
- [ ] T016 [US1] Add Wayland polling fallback in src-tauri/src/services/clipboard_listener.rs for Wayland sessions
- [ ] T017 [US1] Test clipboard capture manually per quickstart.md section 3.1-3.3

**Checkpoint**: Basic clipboard functionality works - text, images, and files can be captured and pasted on Ubuntu

---

## Phase 4: User Story 2 - Source Application Detection (Priority: P2)

**Goal**: Identify and display source application name for each clipboard entry

**Independent Test**: Copy content from Firefox, GNOME Terminal, and VS Code, verify correct app names appear in clipboard history

### Implementation for User Story 2

- [ ] T018 [US2] Implement X11 active window detection via _NET_ACTIVE_WINDOW in src-tauri/src/infrastructure/linux_api/window_tracker.rs
- [ ] T019 [P] [US2] Implement get_active_app_info() extracting WM_CLASS property in src-tauri/src/infrastructure/linux_api/window_tracker.rs
- [ ] T020 [P] [US2] Implement get_clipboard_source_app_info() in src-tauri/src/infrastructure/linux_api/window_tracker.rs
- [ ] T021 [US2] Implement start_window_tracking() with X11 event loop in src-tauri/src/infrastructure/linux_api/window_tracker.rs
- [ ] T022 [US2] Add Wayland fallback returning "Unknown" app info in src-tauri/src/infrastructure/linux_api/window_tracker.rs
- [ ] T023 [US2] Test source app detection manually per quickstart.md section 3.4

**Checkpoint**: Source application names display correctly for clipboard entries on X11, gracefully falls back to "Unknown" on Wayland

---

## Phase 5: User Story 3 - File Opening and System Integration (Priority: P2)

**Goal**: Open files with system default applications, support XDG autostart

**Independent Test**: Copy file path, click to open, verify system default application launches; enable autostart and verify TieZ launches on login

### Implementation for User Story 3

- [ ] T024 [P] [US3] Implement open_file_or_url() using xdg-open in src-tauri/src/infrastructure/linux_api/desktop_integration.rs
- [ ] T025 [P] [US3] Implement toggle_autostart() using XDG autostart spec in src-tauri/src/infrastructure/linux_api/desktop_integration.rs
- [ ] T026 [US3] Update src-tauri/src/app/commands/system_cmd.rs open_folder command to use xdg-open on Linux
- [ ] T027 [US3] Update src-tauri/src/services/file_transfer/mod.rs register_received_file to use xdg-open for auto-open on Linux
- [ ] T028 [US3] Test file opening manually per quickstart.md section 3.5
- [ ] T029 [US3] Test autostart manually per quickstart.md section 3.6

**Checkpoint**: Files open with system default applications, autostart works via XDG specification

---

## Phase 6: User Story 4 - System Tray Integration (Priority: P3)

**Goal**: Tray icon visible in GNOME top bar with context menu

**Independent Test**: Verify tray icon appears in GNOME top bar (with AppIndicator extension), right-click shows menu options

### Implementation for User Story 4

- [ ] T030 [US4] Verify Tauri tray-icon feature is enabled in src-tauri/Cargo.toml (already present via tray-icon feature)
- [ ] T031 [US4] Test tray icon visibility on Ubuntu with AppIndicator extension
- [ ] T032 [US4] Verify tray context menu shows Show/Hide, Settings, and Quit options

**Checkpoint**: System tray icon works on Ubuntu with AppIndicator extension

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Final validation and cleanup

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
