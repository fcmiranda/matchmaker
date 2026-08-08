# Matchmaker Agent Guide

This workspace is a Rust monorepo with four main crates:

- `matchmaker-cli`: command-line entrypoint, clap parsing, config loading, preset registration, and CLI-specific actions.
- `matchmaker-lib`: core picker library, renderer, event loop, previewer, and dynamic handler system.
- `matchmaker-partial`: partial-struct merge traits and tests that power config layering.
- `matchmaker-partial-macros`: proc macros that generate partial structs and derive merge helpers.

Prefer linking to existing docs instead of restating them:

- Architecture and event flow: [matchmaker-lib/ARCHITECTURE.md](matchmaker-lib/ARCHITECTURE.md)
- CLI override syntax: [matchmaker-cli/assets/docs/options.md](matchmaker-cli/assets/docs/options.md)
- Bind syntax: [matchmaker-cli/assets/docs/binds.md](matchmaker-cli/assets/docs/binds.md)
- Template placeholders: [matchmaker-cli/assets/docs/template.md](matchmaker-cli/assets/docs/template.md)
- Partial-config behavior: [matchmaker-partial/README.md](matchmaker-partial/README.md)

## Working Rules

- After every change, run `cargo build --workspace` (or `just build`) before finalizing; targeted tests complement this build but do not replace it.
- Prefer narrow validation first: `cargo test -p <crate>` before `cargo test --workspace` when a change is crate-local.
- Use `just preview -- --help` or `cargo run -p matchmaker-cli -F experimental -- <args>` when validating CLI behavior.
- Use `dprint fmt` or `dprint check` for Markdown and TOML edits.
- Keep config-related changes aligned with the partial-merge model instead of patching around deserialization or override behavior.
- When changing picker behavior, trace the path through event -> action -> renderer/handler rather than only editing a UI surface.
- Treat `matchmaker-partial-macros` as sensitive code: prefer small changes and validate with targeted tests because the generated behavior is easy to regress.

## Commit Policy

- After implementing a feature, create a git commit for that feature.
- Do not amend commits unless explicitly requested.
- If unrelated worktree changes make a clean feature commit ambiguous, stop and ask before committing.

## Navigation Hints

- Start in `matchmaker-cli/src/config.rs` and `matchmaker-lib/src/config.rs` for config shape questions.
- Start in `matchmaker-lib/src/action.rs`, `matchmaker-lib/src/matchmaker.rs`, and [matchmaker-lib/ARCHITECTURE.md](matchmaker-lib/ARCHITECTURE.md) for action flow or TUI behavior.
- Start in `matchmaker-cli/assets/config.toml` and `matchmaker-cli/assets/presets/` for default UX and preset behavior.
- Start in `matchmaker-partial/tests/` for expected merge semantics and macro edge cases.

## Custom Agents

- Use the `Matchmaker Architecture Navigator` agent for event flow, previewer, renderer, and feature-routing questions.
- Use the `Matchmaker Config Surgeon` agent for TOML config, CLI overrides, presets, and template substitution work.
- Use the `Matchmaker Bind Auditor` agent for semantic triggers, mode-scoped binds, and conflict analysis.
- Use the `Matchmaker Partial Merge Specialist` agent for `matchmaker-partial` and macro-generated partial-struct behavior.
