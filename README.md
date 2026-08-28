# Strata

**Navigate every layer.**

Strata is an experimental, keyboard-first file manager for Linux. It is designed primarily for Omarchy while remaining portable to other modern Linux environments.

## Vision

- Miller-column navigation
- Folder peeking on hover
- Ultra-fast search
- Rich file previews
- Collapsible sidebar
- Compact and airy density modes
- List and grid views
- Omarchy and system theming
- Complete keyboard navigation

Read the [MVP and technical direction](docs/mvp.md) for the proposed scope and architecture.

## Technology

- Rust
- GTK4
- GIO
- Native Wayland support

## Development

### Requirements

- Rust 1.85 or newer
- GTK 4.12 or newer
- A C toolchain and `pkg-config`

On Arch Linux:

```bash
sudo pacman -S --needed base-devel rust gtk4
```

Run Strata:

```bash
cargo run
```

Run the checks:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

## Status

Strata is at the technical-spike stage. The first objective is to validate responsive Miller columns, cancellable hover peeking, incremental directory enumeration, and previews in very large directories.

## License

Strata is licensed under the [GNU General Public License v3.0 or later](LICENSE).
