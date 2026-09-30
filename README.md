# DBP [Dual Browser Panels] — Ultra Core

A deliberately small Rust file manager inspired by Norton Commander and the terminal workflow of Che/Yazi.

## Core design
- Two independent browser panels.
- No Git/VCS, tags, plugin engine, indexing daemon, telemetry, database, or background watcher.
- Immediate filesystem operations.
- Built-in text preview; F4 opens the current file in Micro.
- Built-in ZIP compression/extraction.
- Favorite Locations persisted in `favorites-v3.tsv`.
- Batch Rename derived from the recovered DBP Ultra renamer, including `[N]`, `[O]`, `[F]`, `[E]`, `[C]`, `[C3]` style counters and collision-safe two-stage rename with rollback.

## Keys
- `Tab`: switch active panel
- `↑/↓`, `PgUp/PgDn`: navigate
- `Enter`: enter directory
- `Backspace`: parent directory
- `Space`: mark/unmark
- `F5`: copy selected/current item to opposite panel
- `F6`: move selected/current item to opposite panel
- `Ctrl/Cmd+C`, `X`, `V`: internal copy/cut/paste
- `F2`: rename one
- `Ctrl/Cmd+R`: Batch Rename
- `Ctrl/Cmd+B`: Favorite Locations
- `Ctrl/Cmd+L`: add current directory directly to Favorites
- `Ctrl/Cmd+A`: compress selection/current item to ZIP
- `Ctrl/Cmd+E`: extract current ZIP
- `F4`: edit current file with Micro
- `Ctrl/Cmd+Q`: quit

## Build
Install a current stable Rust toolchain and run:

```bash
cargo build --release
```

Executable:
- Windows: `target\\release\\dbp.exe`
- macOS/Linux: `target/release/dbp`

Micro must be available in PATH for F4. The rest of the core is Rust-native.

## GitHub automatic binaries
The included `.github/workflows/build.yml` produces a Windows executable and a macOS binary from the same source using GitHub Actions. Run the workflow manually from **Actions → Build DBP → Run workflow**, or push a `v*` tag.
