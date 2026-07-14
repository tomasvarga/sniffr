# Rust port (branch `rust`)

The bash `bin/sniffr` on `main` is the reference; this branch ports it to a
single binary. **Functional parity reached** — builds clean, review (json +
inject + consensus + progressive + parallel), backends (tuicr/hunk/custom),
queue, doctor, setup, update, version, open-cmd, `--bg`, and the noise filters
are all ported and exercised.

## Crates
clap (CLI) · tokio (async + parallel agents, JoinSet+Semaphore) · serde/serde_json ·
toml (config) · regex (diff parse) · anyhow · dirs · which · owo-colors.
Still shells out to `gh`, the agent CLIs, and tuicr/hunk (those stay external).

## Module map (bash → rust)
| bash | rust | status |
|------|------|--------|
| arg parsing / dispatch | `cli.rs` + `main.rs` | ✅ |
| config (`~/.config/sniffr/config.toml`) | `config.rs` | ✅ |
| finding JSON shapes + mkbadge | `finding.rs` | ✅ |
| default prompts (embedded via include_str!) | `prompt.rs` | ✅ |
| run_agent / run_model | `agent.rs` | ✅ |
| the python resolver (diff parse + content-anchor) | `resolve.rs` | ✅ |
| target parse + `gh pr diff` | `target.rs` | ✅ |
| review orchestration (pool → merge → inject, `--bg`, filters) | `review.rs` | ✅ |
| tuicr/hunk/custom inject + `open-cmd` | `backend.rs` | ✅ |
| consensus merge (cheap model) | `consensus.rs` | ✅ |
| `queue` | `queue.rs` | ✅ |
| `doctor` / `setup` / `update` / `version` | `doctor.rs`/`setup.rs`/`update.rs`/main | ✅ |

## Verified
`version`, `doctor`, `setup`, `open-cmd` (tuicr+hunk), `--format json` (canned →
anchors L10/L18), `--min-severity`/`--min-confidence` filters, and a **live tuicr
inject** (attach mode: 26 → 28 comments).

## Deferred hardening (from codex arch review — v2, would diverge from bash parity)
- Make "validated anchored candidate" the boundary; reject unanchored findings in inject mode.
- Strict (exact) `code` anchoring; linear balanced-JSON `extract_array`; deterministic path matching.
- Validated enums (severity/kind/format/backend) + provenance struct; config validation; agent-failure propagation (currently swallowed, like the bash).
- These are tracked; the bash behaves the same today, so parity-first keeps them for a follow-up.

## Distribution (next)
`cargo dist init` → cross-compiled release binaries + Homebrew tap + curl installer + CI.
Build now: `cargo build --release` → `target/release/sniffr`.
