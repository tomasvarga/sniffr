# Rust port (branch `rust`)

The bash `bin/sniffr` on `main` is the reference; this branch ports it to a
single binary. **Functional parity reached** — builds clean, review (json +
inject + consensus + progressive + parallel), backends (tuicr/hunk/custom),
queue, doctor, setup, update, version, open-cmd, `--bg`, and the noise filters
are all ported and exercised.

## Local diffs (beyond the bash — new on `rust-local-diff`)
sniffr no longer needs a GitHub PR. `target.rs` resolves any of:
- `sniffr --diff` → local uncommitted changes (`git diff HEAD`)
- `sniffr --diff --staged` → staged only (`git diff --cached`)
- `sniffr main..HEAD` → a git ref range (`git diff <range>`)
- `sniffr -` → a unified diff on stdin
- `sniffr owner/repo#N | URL | number` → a PR (unchanged)

Backends: `--format json` and `hunk` work on any diff. **tuicr works locally too** —
sniffr keys on the checkout path and discovers the live `kind:"local"` session
(`tuicr review list --repo .`), then injects with `review add --repo . --session <slug>`.
Open the local reviewer with `tuicr -w` (working tree) or `tuicr -r <range>`.

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
inject** (attach mode: 26 → 28 comments). Plus `cargo test` — 8 unit tests over
the resolver (parse_diff / match_file / resolve_line / extract_array).

## Addressed in review (Fable + Sonnet, PR #1)
- **Bare PR numbers restored** — `target.rs` resolves the cwd repo via `gh repo view` (was a parity regression that bailed).
- **Agent failures surface** — `agent.rs` checks child exit status + captures stderr; `review.rs`/`consensus.rs` log a failed agent instead of `.unwrap_or_default()`, so "crashed" ≠ "found nothing". `config.rs` warns on a malformed config.
- **Resolver hardened** — `extract_array` bracket-matching is string/escape-aware (a `code` field with `[`/`]` no longer mis-parses); `parse_diff` resets context on `diff --git` (kills the junk-lines quirk).

## Found by dogfooding (sniffr reviewed PR #1)
A `sniffr tomasvarga/sniffr#1` run (claude, `--format json`) flagged two real, pre-existing resolver bugs — both fixed, both with regression tests:
- **`match_file` suffix now respects path boundaries** — `a.rs` no longer anchors into `banana.rs`; requires a `/` boundary.
- **`+++ ` mid-hunk is content, not a header** — header detection is gated on `newno.is_none()`, so an added line beginning `++ ` (patch/markdown fixtures) no longer desyncs line numbers.
- **Per-agent timeout** — the run also exposed that a hung agent (a 47-min `codex exec` stall) blocks forever. `agent.rs` now wraps each agent in `tokio::time::timeout` (`SNIFFR_AGENT_TIMEOUT` secs, default 1800 = 30 min, `0` = off) with `kill_on_drop`, so a stuck agent is killed and surfaced instead of hanging.
- `cargo test` → 10 tests.

## Deferred hardening (from codex arch review — v2, would diverge from bash parity)
- Make "validated anchored candidate" the boundary; reject unanchored findings in inject mode.
- Strict (exact) `code` anchoring; deterministic path matching.
- Validated enums (severity/kind/format/backend) + provenance struct.
- These are tracked; the bash behaves the same today, so parity-first keeps them for a follow-up.

## Layout
The Rust crate lives in `rust/` (this folder); the bash reference (`bin/sniffr`),
shared prompts (`prompts/`), `SETUP_PROMPT.md`, `config/`, `docs/`, and `assets/`
stay at the repo root — the binary embeds the root `prompts/*.md` and
`SETUP_PROMPT.md` at compile time via `include_str!("../../...")`.

## Distribution (next)
`cargo dist init` → cross-compiled release binaries + Homebrew tap + curl installer + CI.
Build now: `cargo build --release --manifest-path rust/Cargo.toml` → `rust/target/release/sniffr`
(or `cargo build --release` from inside `rust/`).
