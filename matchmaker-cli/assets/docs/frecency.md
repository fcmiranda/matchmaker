# Matchmaker Frecency Database & Subcommand Architecture

Matchmaker (`mm`) includes a built-in, high-performance **Frecency** (Frequency + Recency) tracking engine designed to track, rank, and provide instant access to your most frequently and recently accessed files and directories.

---

## 1. Storage & Database Architecture (`redb`)

- **Embedded Database**: Frecency data is stored using [**`redb`**](https://github.com/cberner/redb), a pure-Rust, zero-dependency, high-performance embedded ACID database.
- **Table Structure**: Data is maintained in the `frecency_v1` table (`TableDefinition<&str, &str>`), mapping normalized path strings to JSON-serialized `FrecencyRecord` structs.
- **Database Path**: Stored in the user's state directory:
  - Linux/macOS: `~/.local/state/matchmaker/matchmaker.redb`
  - Windows: `%LOCALAPPDATA%\matchmaker\matchmaker.redb`

---

## 2. Frecency Scoring Algorithm

Each path record maintains a access count, last access timestamp, and a sliding window of up to 50 recent access timestamps. Scores are computed using an **exponential decay weighting** model based on access age relative to the current Unix timestamp:

| Access Recency | Time Window | Bonus Score per Access |
| :--- | :--- | :--- |
| **< 1 Hour** | `< 3,600 s` | **100 points** |
| **< 1 Day** | `< 86,400 s` | **80 points** |
| **< 1 Week** | `< 604,800 s` | **40 points** |
| **< 1 Month** | `< 2,592,000 s` | **20 points** |
| **>= 1 Month** | `>= 2,592,000 s` | **10 points** |

## 3. Automatic Recording on Selection & Navigation

When `frecency = true` (or `--frecency` / `-f`) is enabled (such as in **Jump Mode** `mm -o jump` or shell integration `j` / `z`):
- **Automatic Selection Tracking**: Whenever you select/accept a directory or file in navigation mode (`nav_mode = true`) or hit `Enter`/`@accept` to pick a path, Matchmaker automatically records access via `FrecencyStore::add()`.
- **Dynamic Score Growth**: Every selection increases the path's access count, updates its `last_accessed` timestamp, and boosts its frecency score, making frequently visited directories naturally climb to the top of future searches.

---

## 4. In-Memory Search Engine (`FrecencySnapshot`)

To prevent database disk reads during active TUI fuzzy filtering, Matchmaker loads all active frecency records into an in-memory `FrecencySnapshot`:

- **Zero-Allocation Lookups**: Uses `rustc_hash::FxHashMap` for `scores` (full paths) and `basename_scores` (file/directory basenames).
- **Sub-Millisecond Scoring**: During active search or sorting (`frecency = true`), the snapshot provides score boosts in **~5 nanoseconds per item** without any disk I/O.
- **Basename Boosting**: Even if you query a file from a different directory (e.g. `config.toml`), Matchmaker matches the basename score to rank frequently opened files high in search results.

---

## 4. Frecency CLI Commands & Subcommands

Matchmaker provides a comprehensive set of CLI subcommands for managing and inspecting the frecency database:

### Record & Query
- **`mm add <path>`**: Records an access event for a file or directory path (increases score and timestamp).
- **`mm rank <path>`**: Displays the current frecency score, total access count, last access time, and raw records for a path.
- **`mm list [-d / --dirs / --dirs-only] [keywords...]`** / **`mm query`**: Lists tracked paths sorted by frecency score descending, filtered to items matching ALL specified keywords.
  - Option `-d` / `--dirs`: Filters and outputs **directories only** (used in `jump` shell integrations like `j` / `z`).

### Maintenance & Management
- **`mm rm <path>`** / **`mm remove <path>`**: Manually deletes a path entry from the frecency database.
- **`mm clean`** / **`mm prune`**: Scans the database and purges stale entries (paths that no longer exist on the local filesystem).
- **`mm cache [path]`**: Warm-indexes directory structures into local cache for ultra-fast startup.

### Integrations & Migrations
- **`mm import zoxide`**: Imports historical directory access records and scores directly from `zoxide`.
- **`mm init <shell> [--cmd <alias>]`**: Generates shell integration code for `zsh`, `bash`, `fish`, `nushell`, or `powershell` with optional command alias (e.g. `mm init zsh --cmd j`).
