# Strata Product Requirements Document

> **Status:** Living document  
> **Role:** Product North Star  
> **Last updated:** 2026-08-28

## 1. Product summary

Strata is a fast, keyboard-first file manager for Linux that keeps the filesystem's hierarchy visible. It is designed around Miller-column navigation, temporary folder peeking, immediate search, useful previews, and deep visual customization.

Strata should feel at home on Omarchy while remaining a first-class application on other Linux distributions.

**Tagline:** Navigate every layer.

## 2. Problem

Traditional file managers often hide path context, make deep navigation repetitive, separate search from browsing, or become sluggish while loading metadata and thumbnails. Keyboard-oriented alternatives are fast but can sacrifice discoverability, visual previews, and desktop integration.

Users should not have to choose between speed, context, visual polish, and keyboard control.

## 3. Target users

### Primary

- Omarchy and other Linux users who value speed, polish, and keyboard-driven workflows
- Developers and power users navigating deep project trees
- Users who want more context than a single-folder view provides

### Secondary

- Mouse-first users who benefit from hover peeking and rich previews
- Users seeking a themeable replacement for a conventional desktop file manager

## 4. Product principles

1. **Context is persistent.** Parent and child locations remain visible while navigating.
2. **Interaction is immediate.** Input, navigation, and cancellation should never feel blocked by filesystem work.
3. **Keyboard and pointer are peers.** Every core workflow supports both.
4. **Peeking is reversible.** Exploring must not unexpectedly alter committed navigation or history.
5. **The common path is simple.** Advanced capability must not make basic file management confusing.
6. **Customization uses stable concepts.** Themes and preferences target semantic roles rather than internal widgets.
7. **Linux is the platform.** Desktop conventions, default applications, trash, mounts, and clipboard interoperability matter.
8. **Failure is contained.** Inaccessible paths, malformed previews, disconnected devices, and partial operations must not destabilize the application.

## 5. Goals

- Make deep filesystem navigation faster and easier to understand.
- Provide a distinctive, smooth Miller-column browsing experience.
- Make filename search feel immediate and content search easy to invoke.
- Preview common files without opening separate applications.
- Offer complete keyboard navigation without reducing mouse usability.
- Follow Omarchy themes automatically while supporting other systems and custom themes.
- Remain responsive in very large directories and during background work.
- Provide dependable everyday file operations.

## 6. Non-goals for the first release

- Cloud storage accounts
- Remote protocols such as SMB and SFTP
- Root or administrator browsing
- Arbitrary independent split panes
- A public in-process binary plugin ABI
- Archive editing
- Tags, ratings, or digital asset management
- A full media player or text editor
- Cross-platform support outside Linux

These are not permanently rejected; they are excluded from the first release to protect focus.

## 7. Core experience

### 7.1 Miller-column navigation

- Each committed directory level appears as a column.
- Selecting a directory reveals its children in the next column.
- Selecting a different directory replaces all deeper columns.
- The active path remains visually clear.
- Horizontal overflow scrolls naturally and keeps the active column visible.
- Back, forward, parent, home, breadcrumb, and editable-location navigation are available.
- Filesystem changes appear without requiring a manual refresh.

### 7.2 Folder peeking

- Hovering a directory temporarily reveals its children after a short delay.
- Leaving the item restores the last committed path.
- Clicking or pressing the commit key converts the peek into navigation.
- Obsolete peek requests are cancelled.
- Peeking never modifies back/forward history.
- Pointer movement across the temporary column keeps the peek alive.
- Peeking can be disabled and its delay can be configured.

### 7.3 Search

Strata exposes search scope clearly:

1. Filter the currently loaded directory.
2. Recursively search filenames from the current location.
3. Search file contents when explicitly requested.

Requirements:

- Results stream into the interface as they are found.
- Changing the query cancels obsolete work.
- Results show enough path context to distinguish duplicates.
- A result can be revealed in its containing hierarchy.
- Hidden files, ignored files, and scope are controllable.
- Search does not require a global index for the first release.

### 7.4 Preview pane

The preview pane should support:

- Common image formats
- Plain text and source code
- Markdown
- PDF first-page previews
- Audio and video metadata and thumbnails
- Directory summaries
- Generic metadata fallback

Requirements:

- Preview work is cancellable.
- Large files are sampled rather than loaded completely.
- Unsupported or failed previews degrade gracefully.
- The pane can be resized, collapsed, and toggled from the keyboard.
- Preview generation must not block navigation.

### 7.5 Sidebar

The collapsible sidebar contains:

- Home and standard user directories
- Mounted volumes
- User bookmarks
- Trash

Users can add, remove, and reorder bookmarks. Missing standard directories should not produce broken destinations.

### 7.6 Presentation

- List and grid modes
- Compact and airy density presets
- Configurable icon and thumbnail sizes
- Sorting by name, type, size, and modification time
- Optional hidden files
- Clear loading, empty, unavailable, and error states
- Smooth transitions that respect reduced-motion preferences

### 7.7 File operations

The first release supports:

- Open and Open With
- Create file and folder
- Rename
- Copy, cut, paste, move, and duplicate
- Move to trash and permanent delete
- Drag and drop
- Progress, cancellation, and conflict resolution
- Clear reporting of partial failures and permission errors

Destructive actions must be deliberate. Permanent deletion requires stronger confirmation than moving to trash.

### 7.8 Keyboard navigation

- Every core action is reachable without a pointer.
- Selection and keyboard focus remain visibly distinct.
- Directional navigation maps naturally to Miller columns.
- Search, location entry, preview, sidebar, view mode, hidden files, rename, copy, move, paste, trash, and cancel have defaults.
- Keybindings become user-configurable without editing source code.
- The application exposes a discoverable shortcut reference.

### 7.9 Themes and typography

- Strata automatically follows the active Omarchy theme when available.
- It follows suitable system appearance defaults elsewhere.
- Users can select or create a Strata-specific theme.
- Themes use documented semantic tokens for surfaces, text, selection, borders, status colors, spacing, radii, and typography.
- The default visual profile uses JetBrains Mono with a safe system monospace fallback.
- Users and themes can configure interface and monospace preview fonts independently.
- Missing theme values always fall back safely.
- Theme changes apply without restarting.

### 7.10 Preferences

Preferences include:

- View mode and density
- Sidebar and preview visibility
- Hidden-file behavior
- Sorting defaults
- Hover-peek enablement and delay
- Thumbnail behavior
- Search exclusions
- Interface and preview fonts
- Theme source and reduced motion
- Configurable keybindings

Settings must have stable defaults and tolerate unknown values from newer versions.

## 8. Quality requirements

### Responsiveness

- Keyboard and pointer feedback should appear within one display frame.
- Ordinary directory navigation should begin rendering without perceptible delay.
- Background search, metadata, and preview work must not freeze interaction.
- A directory containing 100,000 entries must remain navigable.
- Obsolete asynchronous work must be cancellable or safely ignored.

### Reliability

- Strata handles invalid UTF-8 filenames, broken symlinks, disappearing files, permission changes, disconnected mounts, cross-device moves, and disk-full failures.
- A failed preview or search provider cannot crash the main browsing experience.
- File operations report their final state accurately.

### Accessibility

- Core functionality is keyboard accessible.
- Interactive controls expose accessible names and roles.
- Focus is always visible.
- Themes maintain usable contrast.
- Reduced motion and system text preferences are respected where practical.

### Privacy

- Local browsing data is not transmitted externally.
- Content search is user initiated.
- Any future index clearly documents scope, exclusions, storage, and deletion.
- Extensions cannot silently receive unrestricted file access.

## 9. MVP acceptance criteria

The MVP is complete when a user can:

1. Launch Strata into a responsive local directory view.
2. Navigate deep trees through animated Miller columns using mouse or keyboard.
3. Peek into folders without changing committed history.
4. Use the sidebar, breadcrumbs, and history to move between locations.
5. Switch between list/grid and compact/airy modes.
6. Search filenames recursively and receive streaming results.
7. Preview common images, text, Markdown, PDFs, and media thumbnails.
8. Perform essential file operations with progress and error handling.
9. Follow an Omarchy or system theme and customize fonts.
10. Browse a 100,000-entry fixture without the UI becoming unresponsive.

## 10. Success indicators

- Navigation and selection remain fluid during background activity.
- Users can complete common navigation and file-operation workflows entirely by keyboard.
- Search presents useful initial results faster than opening a separate search tool.
- Preview failures are isolated and recoverable.
- Themes can change the product's appearance without requiring widget-specific patches.
- New preview, search, and theme integrations can be added without rewriting navigation.

## 11. Open product questions

- Should a single click commit navigation, or should this be configurable?
- How should a peek transition into a committed path when the pointer enters the peeked column?
- Should list/grid mode apply globally, per location, or per window?
- Which source-code languages receive syntax highlighting in the first release?
- What should the default balance be between thumbnail quality and generation cost?
- Which advanced extension capabilities are safe and valuable enough for the first public extension API?
