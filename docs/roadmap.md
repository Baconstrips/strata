# Strata Roadmap

The roadmap is ordered by risk and dependency rather than by visual prominence. Each milestone should leave Strata usable and measurable.

## Milestone 0 — Foundation

**Goal:** Replace the proof of concept with boundaries that can support the product without premature abstraction.

- Define navigation, location, file-entry, selection, and view-state models.
- Separate UI composition from filesystem, search, preview, operations, settings, and theme services.
- Establish cancellation, error, logging, and background-work conventions.
- Add fixture generation, unit tests, UI smoke tests, and performance baselines.
- Define semantic design tokens and versioned configuration.

**Exit:** The current behavior runs through the new application model, and a provider can be replaced in a test without constructing the full UI.

## Milestone 1 — Navigation core

**Goal:** Deliver the defining Strata browsing experience.

- Production Miller columns and active-path behavior
- Debounced, cancellable hover peeking
- Back/forward/parent/home and editable location
- Keyboard navigation and focus model
- Filesystem monitoring, hidden files, sorting, and large-directory handling
- Functional sidebar and bookmarks
- Horizontal overflow and smooth interruptible transitions

**Exit:** Deep local navigation is reliable by keyboard and pointer, including the 100,000-entry fixture.

## Milestone 2 — Everyday file manager

**Goal:** Make Strata safe and useful for daily local file management.

- Open/Open With, create, and rename
- Copy, cut, paste, move, duplicate, trash, and delete
- Operation queue with progress, cancellation, conflicts, and partial failures
- Clipboard interoperability and drag and drop
- Mounts, removable media, Trash, and permission/error states

**Exit:** Core local workflows can be completed without another file manager.

## Milestone 3 — Search and previews

**Goal:** Make finding and understanding files immediate.

- Current-view filtering and streaming recursive filename search
- Optional content search
- Search scope, exclusions, and reveal-in-context
- Preview registry and resource budgets
- Image, text, source, Markdown, PDF, audio/video, and metadata providers
- Thumbnail caching and preview failure isolation

**Exit:** Search and preview remain responsive during rapid navigation and malformed/large-file tests.

## Milestone 4 — Presentation and customization

**Goal:** Make Strata adapt cleanly to users and desktops.

- List/grid modes and compact/airy density
- Collapsible/resizable sidebar and preview
- Semantic theme engine and live reload
- Omarchy and generic system theme sources
- Configurable interface and monospace fonts
- Configurable keybindings, reduced motion, and preferences UI
- Documented theme format

**Exit:** Appearance and interaction preferences can change without modifying application code.

## Milestone 5 — Hardening and first release

**Goal:** Ship a dependable public release.

- Accessibility audit
- Performance profiling and regression budgets
- Preview isolation and security review
- Crash recovery and operation edge-case testing
- Arch/AUR packaging and release automation
- Flatpak feasibility and permissions review
- User documentation, contribution guide, and issue templates

**Exit:** Release checklist passes on Omarchy and representative non-Omarchy Linux environments.

## Later exploration

- Remote locations
- Archive browsing
- Independent panes and tabs
- Saved workspaces
- Batch rename
- Global indexed search
- Out-of-process or sandboxed extensions
- Optional developer integrations such as Git status
