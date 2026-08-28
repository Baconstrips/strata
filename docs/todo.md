# Strata Work Breakdown

This is the execution checklist derived from the [PRD](prd.md) and [roadmap](roadmap.md). Work top-to-bottom within a milestone unless dependencies indicate otherwise.

Legend: **P0** blocks the milestone, **P1** is required for its exit criteria, **P2** is polish or follow-up.

## Current proof of concept

- [x] Create public repository, license, and CI
- [x] Launch a native application window
- [x] Enumerate the home directory asynchronously
- [x] Render a virtualized file list
- [x] Open files with their default application
- [x] Add clickable sidebar locations
- [x] Add animated prototype Miller columns

## M0 — Foundation

### Product and engineering baseline

- [x] Add PRD, roadmap, work breakdown, and architecture principles
- [ ] **P0** Create deterministic fixture generator for 1k, 10k, and 100k entries
- [ ] **P0** Record startup, first-render, navigation, and large-directory baselines
- [ ] **P0** Add structured logging with request IDs and timings
- [ ] **P1** Add contributor development commands and pre-commit guidance
- [ ] **P1** Add issue and pull-request templates

### Models and state

- [ ] **P0** Introduce native-path-safe `Location` and `FileEntry` models
- [ ] **P0** Model committed `NavigationPath` separately from temporary `PeekState`
- [ ] **P0** Add explicit active-column, focus, and selection state
- [ ] **P0** Add navigation commands and reducer/controller tests
- [ ] **P1** Add typed loading, empty, unavailable, and error states
- [ ] **P1** Define metadata states so unknown is not confused with zero/empty

### Boundaries

- [ ] **P0** Move direct enumeration out of UI widgets into a file-source service
- [ ] **P0** Define cancellable, generation-aware request handling
- [ ] **P0** Define bounded batch delivery and stale-result rejection
- [ ] **P1** Establish operation, search, preview, theme, and settings capability types
- [ ] **P1** Introduce dependency composition at application startup
- [ ] **P2** Add initial ADRs only for decisions that cannot remain reversible

### Design system

- [x] Capture the prototype's layout, motion, typography, and interaction baseline
- [x] Audit and record licenses for the bundled JetBrains Mono font and Lucide icon subset
- [ ] **P0** Replace widget-specific colors with semantic theme tokens
- [ ] **P1** Define typography, spacing, radius, density, and animation tokens
- [x] Bundle JetBrains Mono as the default visual profile with a system fallback
- [ ] **P1** Define separate interface and monospace preview font settings
- [x] Establish semantic icon names backed by a curated, namespaced Lucide subset
- [ ] **P1** Add reduced-motion token and safe fallback values

## M1 — Navigation core

### Miller columns

- [ ] **P0** Render columns from `NavigationPath` rather than constructing them ad hoc
- [ ] **P0** Replace deeper columns when a sibling path is committed
- [ ] **P0** Keep the active column visible during horizontal overflow
- [ ] **P0** Preserve selection per committed column
- [ ] **P1** Make entry/exit animations interruptible
- [ ] **P1** Add loading skeleton and directory error state
- [ ] **P1** Cancel enumeration when a column is removed

### Hover peeking

- [ ] **P0** Add configurable hover debounce
- [ ] **P0** Model peek without modifying committed history
- [ ] **P0** Cancel obsolete peeks and ignore stale results
- [ ] **P0** Keep peek alive while moving into its temporary column
- [ ] **P0** Commit a peek by click or keyboard action
- [ ] **P1** Restore committed columns when a peek closes
- [ ] **P1** Add setting to disable hover peeking
- [ ] **P1** Test rapid pointer movement and slow directories

### Navigation controls

- [ ] **P0** Back, forward, parent, and home commands
- [ ] **P0** Editable location entry with validation and error feedback
- [ ] **P1** Breadcrumb/path display
- [ ] **P1** Reveal and focus the active location after navigation
- [ ] **P1** Handle symlinks and inaccessible destinations deliberately

### Keyboard and selection

- [ ] **P0** Arrow and `h/j/k/l` navigation
- [ ] **P0** Enter/open, Escape/cancel, and Space/preview actions
- [ ] **P0** Define focus transfer between sidebar, columns, location, search, and preview
- [ ] **P1** Visible distinction between focus and selection
- [ ] **P1** Multi-selection model and modifier behavior
- [ ] **P1** Shortcut reference overlay

### Directory behavior

- [ ] **P0** Hidden-file toggle
- [ ] **P0** Stable sorting by name, type, size, and modified time
- [ ] **P0** Directory monitoring with incremental updates
- [ ] **P0** Preserve UI responsiveness in 100k-entry fixture
- [ ] **P1** Handle invalid UTF-8 display names without losing native paths
- [ ] **P1** Handle broken symlinks and files disappearing during navigation
- [ ] **P1** Add configurable folders-first sorting

### Sidebar

- [ ] **P0** Resolve standard user directories instead of assuming English folder names
- [ ] **P0** Add mounted volumes and Trash
- [ ] **P1** Add, remove, activate, and reorder bookmarks
- [ ] **P1** Collapse sidebar with mouse and keyboard
- [ ] **P1** Persist sidebar state and bookmarks

## M2 — Everyday file manager

### Opening and creation

- [ ] **P0** Open and Open With
- [ ] **P0** Create folder and empty file
- [ ] **P0** Rename with validation and inline feedback
- [ ] **P1** Executable-file policy and confirmation

### Operation engine

- [ ] **P0** Model queued operation lifecycle and final outcomes
- [ ] **P0** Copy, move, duplicate, trash, and permanent delete
- [ ] **P0** Progress reporting and cancellation
- [ ] **P0** Conflict handling: skip, replace, rename, and apply-to-all
- [ ] **P0** Partial-failure reporting and safe cancellation cleanup
- [ ] **P1** Cross-device move behavior
- [ ] **P1** Disk-full, permissions, disappearing source, and read-only tests
- [ ] **P2** Limited undo where behavior can be guaranteed

### Desktop interoperability

- [ ] **P0** Copy/cut/paste with interoperable file-manager clipboard formats
- [ ] **P0** Internal and external drag and drop
- [ ] **P1** Removable media mount, unmount, and disconnect states
- [ ] **P1** Notifications for completed long-running operations

## M3 — Search and previews

### Search

- [ ] **P0** Instant current-directory filtering
- [ ] **P0** Streaming recursive filename search
- [ ] **P0** Query cancellation and stale-result rejection
- [ ] **P0** Search result path context and reveal-in-columns action
- [ ] **P1** Content search
- [ ] **P1** Hidden/ignored file and scope controls
- [ ] **P1** Search error and unavailable-provider states
- [ ] **P2** Evaluate indexed search only after measuring real need

### Preview framework

- [ ] **P0** Add MIME-aware preview registry with provider priorities
- [ ] **P0** Enforce byte, time, pixel, and concurrency budgets
- [ ] **P0** Cancel previews when selection changes
- [ ] **P0** Generic metadata and unsupported fallback
- [ ] **P1** Freedesktop-compatible thumbnail cache
- [ ] **P1** Isolate provider failures from navigation

### Built-in previews

- [ ] **P0** Common image formats
- [ ] **P0** Bounded plain text and source preview
- [ ] **P1** Markdown rendering
- [ ] **P1** PDF first page
- [ ] **P1** Audio metadata and artwork
- [ ] **P1** Video metadata and thumbnail
- [ ] **P1** Directory summary
- [ ] **P2** Syntax highlighting with configurable monospace font

## M4 — Presentation and customization

### Views

- [ ] **P0** Production list mode
- [ ] **P0** Virtualized grid mode
- [ ] **P0** Compact and airy density presets
- [ ] **P1** Configurable icon/thumbnail size
- [ ] **P1** Resizable and collapsible preview pane
- [ ] **P1** Persist view preferences at the agreed scope

### Theme system

- [ ] **P0** Validate semantic theme schema and fallback cascade
- [ ] **P0** Load current Omarchy theme and watch for live changes
- [ ] **P0** Add generic system light/dark source
- [ ] **P1** Load user themes from XDG configuration directories
- [ ] **P1** Apply interface and monospace font overrides live
- [ ] **P1** Document theme format with a complete example
- [ ] **P1** Test missing, malformed, light, and low-contrast themes

### Settings and keybindings

- [ ] **P0** Add versioned settings schema and XDG persistence
- [ ] **P0** Centralize defaults and tolerate unknown settings
- [ ] **P1** Preferences UI
- [ ] **P1** Configurable keybindings and conflict detection
- [ ] **P1** Reduced-motion preference
- [ ] **P1** Import/export settings

## M5 — Hardening and release

- [ ] **P0** Keyboard and assistive-technology accessibility audit
- [ ] **P0** Performance profiling against defined budgets
- [ ] **P0** Preview parser threat review and process-isolation decision
- [ ] **P0** Crash and operation recovery review
- [ ] **P0** Arch package and AUR release workflow
- [ ] **P1** Test on Omarchy and representative non-Omarchy environments
- [ ] **P1** User guide, troubleshooting, and contribution guide
- [ ] **P1** Release notes and automated tagged builds
- [ ] **P1** Flatpak filesystem/portal feasibility review

## Later

- [ ] Design a versioned, capability-limited extension protocol
- [ ] Evaluate remote location adapters
- [ ] Evaluate archive browsing
- [ ] Evaluate independent panes, tabs, and saved workspaces
- [ ] Evaluate batch rename and optional developer integrations
