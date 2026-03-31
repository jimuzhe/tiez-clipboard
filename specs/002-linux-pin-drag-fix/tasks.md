# Tasks: Linux Window Pinning and Drag Fixes

**Input**: Design documents from `/specs/002-linux-pin-drag-fix/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Each user story phase includes a Testing Specification section with manual test instructions, test standards, and result recording format.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

- **Frontend**: `src/` (React/TypeScript)
- **Backend**: `src-tauri/src/` (Rust)
- This is a bug fix feature - all paths are existing files

---

## Phase 1: Setup (Verification)

**Purpose**: Verify development environment and dependencies are in place

- [x] T001 Verify xdotool is installed on system (`which xdotool` returns path)
- [x] T002 [P] Verify ydotool is installed for Wayland compatibility (`which ydotool` returns path) - OPTIONAL: Not installed, Wayland fallback optional
- [x] T003 [P] Verify Rust toolchain version >= 1.75 (`rustc --version`)
- [x] T004 [P] Verify Node.js version >= 18 (`node --version`)
- [x] T005 Build and run application in dev mode (`npm run tauri:dev`)

---

## Phase 2: User Story 1 - Pin and Paste Workflow (Priority: P1) 🎯 MVP

**Goal**: Fix paste operation not working after window is pinned on Linux

**Independent Test**: Pin the window, select any clipboard item, press Enter, verify content pastes to target application

### Testing Specification for User Story 1

> **Instructions**: After completing implementation tasks, follow these test procedures and record results.

#### Test Instructions

| Step | Action | Expected Result |
|------|--------|-----------------|
| 1 | Open TieZ clipboard manager | Window appears with clipboard history |
| 2 | Click the pin icon in title bar | Window becomes pinned (icon changes, visual feedback) |
| 3 | Open a text editor (e.g., gedit, VS Code) | Editor window opens |
| 4 | Select any text item in clipboard manager | Item is highlighted |
| 5 | Press Enter key | Content is pasted into the text editor |
| 6 | Verify pasted content matches selected item | Content matches exactly |
| 7 | Select another item and double-click | Content is pasted into the text editor |
| 8 | Test with image content | Image is pasted correctly |
| 9 | Test with file/folder path | Path is pasted correctly |

#### Test Standards (from Acceptance Scenarios)

| Scenario ID | Description | Pass Criteria |
|-------------|-------------|---------------|
| US1-AC1 | Given window open and unpinned, When pin clicked, Then window pins AND paste works | Paste succeeds after pin, no errors in console |
| US1-AC2 | Given window pinned, When item selected and paste triggered, Then content pastes to target | Content appears in target app within 500ms |
| US1-AC3 | Given window pinned and item selected, When Enter pressed, Then content copied and pasted | Content in clipboard AND pasted to target app |

#### Test Results Recording

| Scenario ID | Test Date | Result (Pass/Fail) | Notes |
|-------------|-----------|-------------------|-------|
| US1-AC1 | 2026年3月25日 | Pass | None |
| US1-AC2 | 2026年3月25日 | Pass | None |
| US1-AC3 | 2026年3月25日 | partly pass | any file/directory can not be paste in file manager. Log:[DEBUG] Raw text/uri-list: "file:///home/owner/Coding/Perl%20Learning/tk\r\n"
[DEBUG] Linux found 1 valid files via text/uri-list: ["/home/owner/Coding/Perl Learning/tk"]
[DEBUG] copy_to_clipboard called: id=-1774409128992, paste=true, content_type=file, content_len=35
[DEBUG] Setting 1 files to clipboard: ["/home/owner/Coding/Perl Learning/tk"]
[DEBUG] URI list: "file:///home/owner/Coding/Perl%20Learning/tk\r\n"|

### Implementation for User Story 1

- [x] T006 [US1] Add Linux paste keystroke function using xdotool in `src-tauri/src/services/clipboard_ops.rs`
- [x] T007 [US1] Add Wayland fallback using ydotool with comment "Wayland is also implemented here" in `src-tauri/src/services/clipboard_ops.rs`
- [x] T008 [US1] Detect display protocol (X11 vs Wayland) using `XDG_SESSION_TYPE` env var in `src-tauri/src/services/clipboard_ops.rs`
- [x] T009 [US1] Add Linux focus restoration using xdotool windowactivate in `src-tauri/src/services/clipboard_ops.rs`
- [x] T010 [US1] Gate Linux-specific code with `#[cfg(target_os = "linux")]` in `src-tauri/src/services/clipboard_ops.rs`
- [x] T011 [US1] Test paste operation with pinned window on X11
- [ ] T012 [US1] Test paste operation with pinned window on Wayland (if available)

**Checkpoint**: At this point, User Story 1 should be fully functional - paste works after pinning

---

## Phase 3: User Story 2 - Window Always on Top (Priority: P1) 🎯 MVP

**Goal**: Fix window not staying on top after being pinned on Linux

**Independent Test**: Pin the window, open/focus other applications, verify clipboard manager remains visible

### Testing Specification for User Story 2

> **Instructions**: After completing implementation tasks, follow these test procedures and record results.

#### Test Instructions

| Step | Action | Expected Result |
|------|--------|-----------------|
| 1 | Open TieZ clipboard manager | Window appears |
| 2 | Click the pin icon to pin window | Window becomes pinned (visual feedback) |
| 3 | Open Firefox or Chrome browser | Browser window opens |
| 4 | Click on browser window to focus it | Browser gains focus |
| 5 | Verify clipboard manager is still visible | Clipboard manager visible on top of browser |
| 6 | Maximize the browser window | Browser fills screen |
| 7 | Verify clipboard manager is still visible | Clipboard manager visible on top of maximized browser |
| 8 | Unpin the window (click pin icon again) | Window unpins |
| 9 | Focus another application | Clipboard manager can be covered by other windows |

#### Test Standards (from Acceptance Scenarios)

| Scenario ID | Description | Pass Criteria |
|-------------|-------------|---------------|
| US2-AC1 | Given window pinned, When another app focused, Then clipboard remains on top | Window visible above all other windows |
| US2-AC2 | Given window pinned, When another app maximized, Then clipboard remains visible and accessible | Window not covered by maximized app |
| US2-AC3 | Given window unpinned, When another window focused, Then clipboard behaves normally | Window can be covered by other windows |

#### Test Results Recording

| Scenario ID | Test Date | Result (Pass/Fail) | Notes |
|-------------|-----------|-------------------|-------|
| US2-AC1 | 2026年3月31日 | Pass | None |
| US2-AC2 | 2026年3月31日 | Pass | None |
| US2-AC3 | 2026年3月31日 | Pass | None |

### Implementation for User Story 2

- [x] T013 [US2] Verify `set_always_on_top` behavior on X11 in `src-tauri/src/app/commands/settings_cmd.rs`
- [x] T014 [US2] Add comment "Wayland is also implemented here" near always_on_top call in `src-tauri/src/app/commands/settings_cmd.rs`
- [x] T015 [US2] If needed, add explicit X11 `_NET_WM_STATE_ABOVE` hint via GTK in `src-tauri/src/app/commands/settings_cmd.rs`
- [x] T016 [US2] Test always-on-top on X11 with GNOME
- [x] T017 [US2] Test always-on-top on X11 with other WMs (KDE, i3, etc. if available)
- [ ] T018 [US2] Document any Wayland limitations in code comments

**Checkpoint**: At this point, User Story 2 should be fully functional - window stays on top when pinned

---

## Phase 4: User Story 3 - Full Title Bar Dragging (Priority: P2)

**Goal**: Extend draggable area to entire title bar including button area

**Independent Test**: Click between buttons in title bar and drag, verify window moves

### Testing Specification for User Story 3

> **Instructions**: After completing implementation tasks, follow these test procedures and record results.

#### Test Instructions

| Step | Action | Expected Result |
|------|--------|-----------------|
| 1 | Open TieZ clipboard manager | Window appears |
| 2 | Click on empty space in title bar (not on buttons) | Click registers |
| 3 | Drag the window by title bar | Window moves following cursor |
| 4 | Release mouse | Window stays in new position |
| 5 | Click between pin and settings buttons | Click registers (not on a button) |
| 6 | Drag the window | Window moves following cursor |
| 7 | Click directly on pin button | Button activates, no drag initiated |
| 8 | Click directly on settings button | Settings panel opens, no drag initiated |
| 9 | Click directly on close button | Window hides, no drag initiated |
| 10 | Verify buttons still work after drag implementation | All buttons functional |

#### Test Standards (from Acceptance Scenarios)

| Scenario ID | Description | Pass Criteria |
|-------------|-------------|---------------|
| US3-AC1 | Given window open, When drag on empty title bar, Then window moves | Window follows cursor smoothly |
| US3-AC2 | Given window open, When drag near buttons, Then window moves | Can drag from spaces between buttons |
| US3-AC3 | Given drag completes, Then window stays in position | Window position retained after release |

#### Test Results Recording

| Scenario ID | Test Date | Result (Pass/Fail) | Notes |
|-------------|-----------|-------------------|-------|
| US3-AC1 | 2026年3月31日 | Pass | None |
| US3-AC2 | 2026年3月31日 | Pass | None |
| US3-AC3 | 2026年3月31日 | Pass | None |

### Implementation for User Story 3

- [x] T019 [US3] Add `data-tauri-drag-region` attribute to button container div in `src/features/app/components/AppHeader.tsx`
- [x] T020 [US3] Add `onMouseDown={(e) => e.stopPropagation()}` to each button to prevent drag initiation in `src/features/app/components/AppHeader.tsx`
- [x] T021 [US3] Test dragging from various title bar positions
- [x] T022 [US3] Verify all buttons still trigger their actions (not drag)

**Checkpoint**: At this point, User Story 3 should be fully functional - entire title bar is draggable

---

## Phase 5: User Story 4 - Settings Page Platform Compatibility (Priority: P2)

**Goal**: Ensure all visible Settings options work on Linux, hide Windows-specific settings

**Independent Test**: Navigate through all Settings sections, verify each option works or is hidden

### Testing Specification for User Story 4

> **Instructions**: After completing implementation tasks, follow these test procedures and record results.

#### Test Instructions

| Step | Action | Expected Result |
|------|--------|-----------------|
| 1 | Open TieZ clipboard manager | Window appears |
| 2 | Click settings icon | Settings panel opens |
| 3 | Navigate to General settings | Section expands |
| 4 | Verify "Registry Win+V" option is hidden | Option not visible (Windows-only) |
| 5 | Toggle "Auto Start" option | Setting changes, persists after restart |
| 6 | Navigate to Clipboard settings | Section expands |
| 7 | Change paste method dropdown | Setting changes, works correctly |
| 8 | Navigate to Appearance settings | Section expands |
| 9 | Change theme | Theme changes immediately |
| 10 | Navigate through Sync, File Transfer, AI settings | All visible options work |
| 11 | Restart application | All settings persist |

#### Test Standards (from Acceptance Scenarios)

| Scenario ID | Description | Pass Criteria |
|-------------|-------------|---------------|
| US4-AC1 | Given Linux, When Settings opened, Then all visible options functional | No broken settings, no errors |
| US4-AC2 | Given setting displayed, When changed, Then effect persists | Change takes effect, persists after restart |
| US4-AC3 | Given platform-specific setting, When on Linux, Then hidden or functional | Windows-only settings hidden |

#### Test Results Recording

| Scenario ID | Test Date | Result (Pass/Fail) | Notes |
|-------------|-----------|-------------------|-------|
| US4-AC1 |  | | |
| US4-AC2 |  | | |
| US4-AC3 |  | | |

### Implementation for User Story 4

- [ ] T023 [US4] Add platform check to hide `registryWinVEnabled` setting on Linux in `src/features/settings/components/groups/ClipboardSettingsGroup.tsx`
- [ ] T024 [P] [US4] Verify auto-start works with xdg-desktop-entry on Linux
- [ ] T025 [P] [US4] Verify hotkey recording works on Linux in `src/features/settings/components/groups/ClipboardSettingsGroup.tsx`
- [ ] T026 [P] [US4] Test Default Apps settings with xdg-open on Linux
- [ ] T027 [US4] Audit and document any other platform-specific settings in `src/features/settings/components/groups/*.tsx`
- [ ] T028 [US4] Test all visible settings end-to-end on Linux

**Checkpoint**: At this point, User Story 4 should be fully functional - all visible settings work on Linux

---

## Phase 6: Pin Toggle Debounce (Cross-Cutting)

**Purpose**: Implement 200ms debounce for rapid pin toggle clicks

### Testing Specification for Debounce

> **Instructions**: After completing implementation, test rapid pin toggle behavior.

#### Test Instructions

| Step | Action | Expected Result |
|------|--------|-----------------|
| 1 | Open TieZ clipboard manager | Window appears |
| 2 | Rapidly click pin button 5 times within 500ms | Only final state is processed |
| 3 | Wait 300ms after clicks | Window in correct state (pinned if odd clicks) |
| 4 | Rapidly toggle pin on/off 3 times | Only final state applied |
| 5 | Verify no visual flickering | Smooth state transition |

#### Test Standards

| Scenario ID | Description | Pass Criteria |
|-------------|-------------|---------------|
| DEBOUNCE-AC1 | Rapid pin toggles within 200ms | Only final state processed |
| DEBOUNCE-AC2 | No visual flickering during debounce | Single state transition |

#### Test Results Recording

| Scenario ID | Test Date | Result (Pass/Fail) | Notes |
|-------------|-----------|-------------------|-------|
| DEBOUNCE-AC1 | | | |
| DEBOUNCE-AC2 | | | |

### Implementation for Debounce

- [ ] T029 Add debounce timeout state in `src/features/app/components/AppHeader.tsx`
- [ ] T030 Wrap pin toggle handler with 200ms debounce in `src/features/app/components/AppHeader.tsx`
- [ ] T031 Clear debounce timeout on component unmount
- [ ] T032 Test rapid pin toggle behavior

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Final validation, cleanup, and edge case handling

### Testing Specification for Polish Phase

> **Instructions**: After completing all user story phases, perform these cross-cutting tests.

#### Cross-Cutting Test Instructions

| Step | Action | Expected Result |
|------|--------|-----------------|
| 1 | Verify all user story tests pass | All scenarios marked Pass |
| 2 | Test edge case: rapid pin toggle | Debounced correctly |
| 3 | Test edge case: multi-monitor drag | Window moves correctly |
| 4 | Test edge case: pin + display config change | Window stays on top |
| 5 | Verify constitution compliance | No violations found |
| 6 | Check for Wayland compatibility comments | Comments present |

#### Cross-Cutting Test Standards

| Standard ID | Description | Pass Criteria |
|-------------|-------------|---------------|
| CC-001 | All acceptance scenarios pass | 100% Pass rate |
| CC-002 | No compilation errors | Build succeeds |
| CC-003 | Constitution compliance | No violations |
| CC-004 | Wayland compatibility comments | "Wayland is also implemented here" present |
| CC-005 | Edge case handling | No crashes, graceful fallbacks |

#### Test Results Recording

| Standard ID | Test Date | Result (Pass/Fail) | Notes |
|-------------|-----------|-------------------|-------|
| CC-001 | | | |
| CC-002 | | | |
| CC-003 | | | |
| CC-004 | | | |
| CC-005 | | | |

### Implementation Tasks

- [ ] T033 [P] Add code comment "Wayland is also implemented here" at all Wayland compatibility points
- [ ] T034 Run `cargo clippy` and fix any warnings in modified files
- [ ] T035 Run `npm run build` and verify no TypeScript errors
- [ ] T036 Final integration test of all user stories together
- [ ] T037 Commit changes with conventional commit format

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - verify environment first
- **User Story 1 (Phase 2)**: Depends on Setup - can start after T001-T005
- **User Story 2 (Phase 3)**: Depends on Setup - can run in parallel with US1
- **User Story 3 (Phase 4)**: Depends on Setup - can run in parallel with US1/US2
- **User Story 4 (Phase 5)**: Depends on Setup - can run in parallel with US1/US2/US3
- **Debounce (Phase 6)**: Can run in parallel with US3/US4
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: No dependencies on other stories - MVP critical
- **User Story 2 (P1)**: No dependencies on other stories - MVP critical
- **User Story 3 (P2)**: No dependencies on other stories
- **User Story 4 (P2)**: No dependencies on other stories

### Parallel Opportunities

- T001-T004 can run in parallel (verification tasks)
- T006-T008 can run in parallel (different aspects of paste fix)
- T013-T015 can run in parallel (different aspects of always-on-top)
- T023-T026 can run in parallel (different settings groups)
- User Stories 1-4 can be worked on in parallel by different developers

---

## Parallel Example: User Stories 1 & 2

```bash
# Developer A: User Story 1 (Paste fix)
Task: "T006 [US1] Add Linux paste keystroke function using xdotool"
Task: "T007 [US1] Add Wayland fallback using ydotool"
Task: "T008 [US1] Detect display protocol"

# Developer B: User Story 2 (Always on top) - in parallel
Task: "T013 [US2] Verify set_always_on_top behavior"
Task: "T014 [US2] Add Wayland compatibility comment"
Task: "T015 [US2] Add X11 hint if needed"
```

---

## Implementation Strategy

### MVP First (User Stories 1 & 2 Only)

1. Complete Phase 1: Setup verification
2. Complete Phase 2: User Story 1 (Paste fix)
3. Complete Phase 3: User Story 2 (Always on top)
4. **STOP and VALIDATE**: Test both P1 stories
5. Deploy/demo if ready - core functionality restored

### Incremental Delivery

1. Setup verification → Environment confirmed
2. User Story 1 → Paste works after pin (MVP!)
3. User Story 2 → Window stays on top (MVP!)
4. User Story 3 → Better drag UX
5. User Story 4 → Settings fully compatible
6. Debounce → Polish edge case
7. Polish → Production ready

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- **Testing Specification**: Complete manual testing per Test Instructions, record results
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- X11 is primary; Wayland compatibility must be preserved with comments

## Constitution Compliance

Per `.specify/memory/constitution.md`, implementations must:
- Preserve backward compatibility (no breaking changes)
- Use simplest viable approach (over-engineering prohibited)
- Gate platform-specific code with `#[cfg(target_os = "linux")]`
- Add comments only before complex logic (no emojis, no line-by-line commenting)
- Not generate markdown documentation after task completion (brief summary only)
