# sniffr

<img src="assets/icon.png" width="88" align="right" alt="sniffr">

**An AI sniffs your PR for issues before you review it.** Point `sniffr` at a
GitHub pull request: it runs an agent over the diff, anchors each finding to the
exact changed line, and drops them in as **local-draft comments** in the terminal
reviewer you already have open — **[tuicr](https://tuicr.dev)**,
**[hunk](https://hunk.dev)**, or your own tool — or prints them as JSON to pipe
anywhere. By the time you start reading, the risky lines are already flagged.

Agent-agnostic (Codex · Claude · Cursor · Grok · opencode · ollama) and
backend-agnostic (tuicr · hunk · custom · plain JSON). **No editor, TUI, or
workspace manager required** — sniffr attaches to whatever reviewer you run.

> Using [herdr](https://herdr.dev)? Install **[herdr-sniffr](https://github.com/tomasvarga/herdr-sniffr)**
> — the plugin that opens the reviewer pane for you and wires the reload + toast
> hooks. This repo is the host-agnostic engine underneath it.

> **sniffr never posts to GitHub.** Every comment is a **local draft** on your
> machine — you read them, prune the noise, and submit what's left yourself.

## How it works

```
sniffr <pr>
  → runs your agent(s) over the PR diff
  → anchors each finding to the real changed line (content-matched, not line-counted)
  → injects them as LOCAL-DRAFT comments into your OPEN reviewer session (never pushed)
  → fires your --notify-cmd / --after-inject hooks when they land
```

**Attach mode.** sniffr injects into a reviewer that's **already open** — so open
it first, then run sniffr:

```bash
tuicr pr owner/repo#123          # (or: hunk patch <diff>) — in one pane/tab
sniffr owner/repo#123 --bg       # in another — keep reading; comments land in ~30s
```

`--bg` detaches the review so you keep reading while the agent works; a
notification fires when the comments land (drop it for a foreground run). Or skip
the reviewer entirely and take the findings as JSON:

```bash
sniffr owner/repo#123 --format json | jq .
```

## Install

One-liner (clones to `~/.local/share/sniffr`, symlinks `sniffr` into `~/.local/bin`):

```bash
curl -fsSL https://raw.githubusercontent.com/tomasvarga/sniffr/main/install.sh | bash
```

Re-run any time to update. Or from a checkout: `git clone … && ./sniffr/install.sh`.
Then `sniffr doctor` to check your setup.

## Usage

```bash
sniffr <pr>                 # number | owner/repo#N | URL
sniffr <pr> --agent grok    # one agent, or a comma list: --agent codex,claude,grok
sniffr <pr> --backend hunk  # inject into hunk instead of tuicr
sniffr <pr> --model <name>  # model for the agent (else its own default/auto)
sniffr <pr> --consensus     # merge multi-agent findings into one comment per bug
sniffr <pr> --min-severity high   # keep only critical/high (also --min-confidence)
sniffr <pr> --bg            # review in the background; keep reading meanwhile
sniffr <pr> --format json   # print resolved findings as JSON; no reviewer needed
sniffr <pr> --after-inject 'cmd'   # run cmd once findings land (e.g. reload the reviewer)
sniffr <pr> --notify-cmd 'cmd'     # run cmd to notify (env SNIFFR_TITLE / SNIFFR_MSG)
sniffr --set-agent grok     # save the default agent (--show-agent prints it)
sniffr queue                # pick from the PRs actually awaiting your review
sniffr doctor               # preflight: deps, gh auth, agent, backend
```

**Agent** (first match wins): `--agent` flag · `SNIFFR_AGENT` env · saved default
(`~/.config/sniffr/agent`) · `codex`. Built-ins: **codex · claude · cursor-agent ·
grok · opencode · ollama** (`cursor` is an alias for `cursor-agent`). Any other
tool via `SNIFFR_CMD='<command>'` — it gets the prompt on stdin and must print a
JSON array of findings (see the [contract](docs/CONTRACT.md)).

**Multiple agents, one pass** — `--agent codex,claude,grok`. Each reviews the same
diff and its findings are injected **stamped by agent**, so you see who flagged
what (two agents on the same line = high signal). They run sequentially.

**Consensus** — add `--consensus` (or set `[consensus].model` in config) and the
per-agent findings are merged into **one comment per bug**, deduped, stamped with
which agents agreed (`🔴 critical · bug · 3 agents (codex·claude·cursor)`), and
rewritten to good-review-comment standard (what · trigger · `↳ fix:`). The review
agents run in **parallel**, then a **cheap** `[consensus].model` does the merge (it's
just writing up — a small/fast model is ideal). Kills the noise of N near-identical
comments while keeping the "who agreed" signal.

**Cut noise** — `--min-severity critical|high|medium|low` and `--min-confidence
0.0–1.0` (or `min_severity` / `min_confidence` in config) drop findings below the
bar; a finding missing that field is kept. Findings carry an optional `severity`,
`confidence` (used internally, never shown), and `recommendation` — see the
[contract](docs/CONTRACT.md).

**Model** (first match wins): `--model` flag · `SNIFFR_MODEL` env · `model =` in
config · else each agent's own default. Use that agent's naming, e.g. `--model
gpt-5.6-codex-high` (cursor-agent), `provider/model` (opencode), or a tag like
`llama3.1` (ollama).

**Tune the review** — set `prompt` in `~/.config/sniffr/config.toml` to change
*what* the agent looks for; sniffr always appends the machine-readable output
contract itself, so you only describe the focus. `max` caps findings per review.

```toml
prompt = "You are a security-focused reviewer. Prioritize auth and input validation; ignore style."
max    = 8
```

## Backends — where the findings go

sniffr's core (diff → agent → line-anchored findings) is backend-agnostic; a
**backend** decides how they're presented. Native support for **tuicr** and
**hunk**; anything else plugs in as a **custom** backend or via `--format json`.
Selection (first match wins): `--backend` flag · `SNIFFR_BACKEND` env · `backend
=` in config · else auto-detect an installed reviewer (tuicr, then hunk).

- **`tuicr`** (default) — [tuicr.dev](https://tuicr.dev); injects local-draft
  comments into the live `tuicr pr` session.
- **`hunk`** — [hunk.dev](https://hunk.dev); injects into the live `hunk patch`
  session via its comment API.
- **custom** — define `open`/`inject` in `config.toml`; sniffr resolves the
  findings and pipes them (as JSON) to your `inject` command. See
  [`config/config.example.toml`](config/config.example.toml).
- **JSON** — `--format json` prints the resolved findings and exits; wire them
  wherever you like.

The two JSON shapes that make all of this composable are specified in
**[docs/CONTRACT.md](docs/CONTRACT.md)**.

### `sniffr queue` — pick from your review list

Instead of pasting a URL, ask GitHub what's waiting on you and pick from a menu:

```bash
sniffr queue                # PRs awaiting your review → pick → sniff it
sniffr queue --since 2mo    # widen the window (default: last 3 weeks)
sniffr queue --all          # full list: bots + drafts + no date cutoff
```

One `gh search` for open PRs where you're a requested reviewer (across all your
repos), filtered by default to **no bots · no drafts · active in the last 3
weeks**. Picking one runs the normal `sniffr <pr>` path — `queue` is purely a
target-picker.

## Requirements

`gh` (authenticated), `jq`, `python3` (≥3.11 for config), and at least one **agent
CLI** on your `PATH`. Plus the binary for your backend — **tuicr** or **hunk** —
unless you use `--format json` (which needs neither). GitHub PRs only for now.
macOS and Linux. `sniffr doctor` verifies all of it.

## Limitations

- **Attach mode** — sniffr injects into a reviewer you already opened; it does not
  launch one (that's what the [herdr-sniffr](https://github.com/tomasvarga/herdr-sniffr)
  plugin adds). Use `--format json` if you have no reviewer.
- **Line anchoring** is by content: the agent quotes the exact line, and sniffr
  resolves the real new-side line number (the agent's own count is only a
  tiebreaker). A quote that can't be matched becomes a file-level comment.
- Comments are **local drafts** — never pushed until you submit them yourself.
- **Re-running duplicates comments.** sniffr appends; it doesn't dedup against a
  previous pass. Prune in your reviewer before re-sniffing the same PR.

## License

MIT — see [LICENSE](LICENSE).
