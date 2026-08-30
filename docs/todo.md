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
- [x] **P0** Create deterministic fixture generator for 1k, 10k, and 100k entries
- [x] **P0** Record startup, first-render, navigation, and large-directory baselines
- [x] **P0** Add structured logging with request IDs and timings
- [x] **P1** Add contributor development commands and pre-commit guidance
- [ ] **P1** Add issue and pull-request templates

### Models and state

- [x] **P0** Introduce native-path-safe `Location` and `FileEntry` models
- [x] **P0** Model committed `NavigationPath` separately from temporary `PeekState`
- [x] **P0** Add explicit active-column, focus, and selection state
- [x] **P0** Add navigation commands and reducer/controller tests
- [x] **P1** Add typed loading, empty, unavailable, and error states
- [x] **P1** Define metadata states so unknown is not confused with zero/empty

### Boundaries

- [x] **P0** Move direct enumeration out of UI widgets into a file-source service
- [x] **P0** Define cancellable, generation-aware request handling
- [x] **P0** Define bounded batch delivery and stale-result rejection
- [ ] **P1** Establish operation, search, preview, theme, and settings capability types
- [x] **P1** Introduce dependency composition at application startup
- [ ] **P2** Add initial ADRs only for decisions that cannot remain reversible

### Design system

- [x] Capture the prototype's layout, motion, typography, and interaction baseline
- [x] Audit and record licenses for the bundled JetBrains Mono font and Lucide icon subset
- [x] **P0** Replace widget-specific colors with semantic theme tokens
- [ ] **P1** Define typography, spacing, radius, density, and animation tokens
- [x] Bundle JetBrains Mono as the default visual profile with a system fallback
- [ ] **P1** Define separate interface and monospace preview font settings
- [x] Establish semantic icon names backed by a curated, namespaced Lucide subset
- [x] **P1** Add reduced-motion token and safe fallback values

## M1 — Navigation core

### Miller columns

- [x] **P0** Render columns from `NavigationPath` rather than constructing them ad hoc
- [x] **P0** Replace deeper columns when a sibling path is committed
- [x] **P0** Allow committed columns to stack without a fixed depth limit
- [x] **P0** Keep the newest active column visible by scrolling to the end during horizontal overflow
- [x] **P0** Preserve selection per committed column
- [x] **P1** Make column entry animations interruptible and remove closed columns immediately
- [x] **P1** Add loading skeleton and directory error state
- [x] **P1** Cancel enumeration when a column is removed

### Hover peeking

- [x] **P0** Add configurable hover debounce
- [x] **P0** Model peek without modifying committed history
- [x] **P0** Cancel obsolete peeks and ignore stale results
- [x] **P0** Keep peek alive while moving into its anchored popover
- [x] **P0** Commit a peek by click or keyboard action
- [x] **P1** Dismiss the popover without changing committed columns when a peek closes
- [x] **P1** Add setting to disable all automatic hover peeking
- [ ] **P1** Test rapid pointer movement and slow directories

### Navigation controls

- [x] **P0** Back, forward, parent, and home commands
- [x] **P0** Editable location entry with validation and error feedback
- [x] **P1** Breadcrumb/path display
- [x] **P1** Copy the current path from the active breadcrumb
- [x] **P1** Activate location editing from `Ctrl+L` or empty breadcrumb-bar space
- [x] **P1** Reveal and focus the active location after navigation
- [x] **P1** Handle symlinks and inaccessible destinations deliberately

### Keyboard and selection

- [x] **P0** Arrow and `h/j/k/l` navigation
- [x] **P0** Enter/open and Escape/close-peek-or-column actions
- [x] **P0** Space/preview action
- [x] **P0** Define focus transfer between columns and location entry
- [ ] **P0** Define focus transfer for sidebar, search, and preview
- [x] **P1** Visible distinction between focus and selection
- [x] **P1** Multi-selection model with an anchor, focused item, and stable selection across updates
- [x] **P1** Mouse multi-selection with marquee/rubber-band selection, `Ctrl+Click` toggle, and `Shift+Click` ranges
- [x] **P1** Keyboard multi-selection with `Shift+Up/Down` range extension and `Ctrl+A`
- [ ] **P1** Shortcut reference overlay

### Directory behavior

- [x] **P0** Hidden-file toggle
- [x] **P0** Stable sorting by name, type, size, and modified time
- [x] **P0** Monitor directory changes and reload affected columns
- [x] **P0** Apply directory-monitor changes incrementally without a full reload
- [x] **P0** Preserve UI responsiveness in 100k-entry fixture
- [x] **P1** Handle invalid UTF-8 display names without losing native paths
- [x] **P1** Handle broken symlinks and files disappearing during navigation
- [x] **P1** Add configurable folders-first sorting
- [x] **P1** Move sorting, folders-first, and filtering controls into each pane header

### Sidebar

- [x] **P0** Resolve standard user directories instead of assuming English folder names
- [x] **P0** Add mounted volumes and Trash
- [ ] **P1** Add, remove, activate, and reorder bookmarks
- [x] **P1** Collapse sidebar with mouse and keyboard
- [ ] **P1** Persist sidebar state and bookmarks

## M2 — Everyday file manager

### Opening and creation

- [x] **P0** Open files with the default application
- [ ] **P0** Add an Open With chooser
- [x] **P0** Create folders from `Ctrl+Shift+N` and the folder background menu
- [ ] **P0** Create an empty file
- [x] **P0** Replace the selected row label with an inline rename input on `F2`, preserving the extension selection and showing validation feedback in place
- [ ] **P1** Executable-file policy and confirmation

### Context menu (revisit after core actions exist)

- [x] **P1** Open a selection-aware file context menu from right-click
- [ ] **P1** Open the selection-aware file context menu from the keyboard menu key
- [x] **P1** Preserve an existing multi-selection when right-clicking one of its selected rows; otherwise select the clicked row
- [x] **P1** Group the available Open, Preview, Cut, Rename, Trash, and Properties actions with visible shortcuts and separators
- [ ] **P1** Add Open With, Copy, Move To, Copy To, and permanent Delete as their underlying actions become available
- [x] **P1** Adapt context-menu actions to single and multiple selections and disable inapplicable preview and paste actions
- [ ] **P1** Disable or hide actions according to destination writability, clipboard operation, and provider capability
- [x] **P1** Add a folder background menu for New Folder, Paste, Select All, and Properties
- [x] **P1** Add file and folder properties dialogs
- [ ] **P2** Evaluate optional Compress, Email, LocalSend, and desktop share integrations without making them core dependencies

### Operation engine

- [ ] **P0** Model queued operation lifecycle and final outcomes
- [ ] **P0** Copy and move selected files between folders through the operation engine
- [ ] **P0** Duplicate selected files
- [x] **P0** Trash and permanently delete selected files
- [x] **P0** Bind `Delete` and `Shift+Delete` to confirmed trash and permanent-delete actions
- [ ] **P0** Progress reporting and cancellation
- [ ] **P0** Conflict handling: skip, replace, rename, and apply-to-all
- [ ] **P0** Partial-failure reporting and safe cancellation cleanup
- [ ] **P1** Cross-device move behavior
- [ ] **P1** Disk-full, permissions, disappearing source, and read-only tests
- [ ] **P2** Limited undo where behavior can be guaranteed
- [ ] **P2** Future Undo/Redo operation history with toolbar buttons and configurable keyboard shortcuts
- [ ] **P2** Bind Undo to `Ctrl+Z` and Redo to both `Ctrl+Shift+Z` and `Ctrl+Y`

### Desktop interoperability

- [ ] **P0** Copy/cut selected files with `Ctrl+C` / `Ctrl+X`
- [x] **P0** Paste a GDK file-list clipboard into the active folder with `Ctrl+V` or the folder background menu
- [ ] **P0** Use interoperable file-manager clipboard formats for external copy/cut/paste
- [ ] **P0** Enable Cut/Copy based on selection and Paste based on clipboard contents and destination writability
- [ ] **P0** Drag and drop between locations within Strata
- [ ] **P0** Export selected files as GTK/GDK file-list and `text/uri-list` drag data for browsers, editors, desktop targets, and other external applications
- [ ] **P1** Test outbound file dragging across native Wayland applications, XWayland applications, and browser upload targets
- [ ] **P1** Removable media mount, unmount, and disconnect states
- [ ] **P1** Show an Unmount action only for volumes that report they can be unmounted
- [ ] **P1** Confirm unmounting, report busy/error states, and refresh the sidebar after completion
- [ ] **P1** Notifications for completed long-running operations

## M3 — Search and previews

### Search

- [x] **P0** Instant current-directory filtering
- [ ] **P0** Streaming recursive filename search
- [ ] **P0** Query cancellation and stale-result rejection
- [ ] **P0** Search result path context and reveal-in-columns action
- [ ] **P1** Content search
- [ ] **P1** Hidden/ignored file and scope controls
- [ ] **P1** Search error and unavailable-provider states
- [ ] **P2** Evaluate indexed search only after measuring real need

### Preview framework

- [ ] **P0** Add MIME-aware preview registry with provider priorities
- [x] **P0** Enforce bounded text reads and PDF pixel budgets
- [ ] **P0** Enforce preview time and concurrency budgets
- [x] **P0** Cancel previews when selection changes
- [x] **P0** Generic metadata and unsupported fallback
- [ ] **P1** Freedesktop-compatible thumbnail cache
- [ ] **P1** Isolate provider failures from navigation

### Built-in previews

- [x] **P0** Common image formats
- [x] **P0** Bounded plain text and source preview
- [ ] **P1** Markdown rendering
- [x] **P1** Native, virtualized, continuous multi-page PDF viewer
- [x] **P1** PDF zoom, fit-width reset, and pointer panning
- [ ] **P1** Audio metadata and artwork
- [ ] **P1** Video metadata and thumbnail
- [ ] **P1** Directory summary
- [x] **P2** Source syntax highlighting and line numbers
- [ ] **P2** Configurable monospace preview font

## M4 — Presentation and customization

### Views

- [x] **P0** Production virtualized list mode
- [ ] **P0** Virtualized grid mode
- [ ] **P0** Compact and airy density presets
- [ ] **P1** Configurable icon/thumbnail size
- [x] **P1** Resizable and collapsible preview pane
- [ ] **P1** Persist view preferences at the agreed scope

### Theme system

- [x] **P0** Validate semantic theme schema and fallback cascade
- [x] **P0** Load current Omarchy theme and watch for live changes
- [ ] **P0** Add generic system light/dark source
- [x] **P1** Load user themes from XDG configuration directories
- [ ] **P1** Apply interface and monospace font overrides live
- [x] **P1** Document theme format with a complete example
- [ ] **P1** Test missing, malformed, light, and low-contrast themes

### Settings and keybindings

- [ ] **P0** Add versioned settings schema and XDG persistence
- [ ] **P0** Centralize defaults and tolerate unknown settings
- [x] **P1** Preferences UI
- [ ] **P1** Persist all preferences through the versioned settings schema
- [ ] **P1** Configurable keybindings and conflict detection
- [x] **P1** Reduced-motion preference that disables nonessential animations
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
