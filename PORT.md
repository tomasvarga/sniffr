# Rust port (branch `rust`)

The bash `bin/sniffr` on `main` stays the reference until this reaches parity.
This branch ports it to a single binary (distributed via `cargo-dist`).

## Crates
clap (CLI) · tokio (async + parallel agents) · serde/serde_json (findings) ·
toml (config) · regex (diff parse) · anyhow (errors) · dirs · which · owo-colors.
Still shells out to `gh`, the agent CLIs, and tuicr/hunk (those stay external).

## Module map (bash → rust)
| bash | rust | status |
|------|------|--------|
| arg parsing / dispatch | `cli.rs` + `main.rs` | ✅ skeleton |
| `~/.config/sniffr/config.toml` (cfg_json) | `config.rs` | ✅ |
| finding JSON shapes + mkbadge | `finding.rs` | ✅ |
| `doctor` | `doctor.rs` | ✅ (deps drop jq/python) |
| `version` | inline | ✅ |
| run_agent / run_model (parallel) | `agent.rs` | ⬜ (tokio::process) |
| the python resolver (diff parse + content-anchor) | `resolve.rs` | ⬜ |
| tuicr/hunk/custom inject + `open-cmd` | `backend.rs` | ⬜ |
| consensus merge (cheap model) + filters | `consensus.rs` | ⬜ |
| review orchestration (pool → merge → inject, --bg) | `review.rs` | ⬜ |
| `queue` | `queue.rs` | ⬜ |
| `setup` / `update` | `setup.rs` / `update.rs` | ⬜ |
| default prompts (prompts/*.md) | `prompt.rs` | ⬜ |

## Build / release
`cargo build --release` → `target/release/sniffr`. Distribution via **cargo-dist**
(`cargo dist init`) → cross-compiled binaries + Homebrew tap + curl installer + CI.

> Not yet compiled here (no rustc in the authoring env). Run `cargo build` after
> `rustup` install; expect a few first-pass fixes.
