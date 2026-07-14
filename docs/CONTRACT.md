# sniffr JSON contract

sniffr is a pipeline: **PR diff → agent → line-anchored findings → a backend**.
Two JSON shapes make it composable. If your tool speaks either one, it plugs in.

```
                 agent output                     resolved findings
   diff  ─►  [{file,code,line,type,body}]  ─►  [{file,line,type,body,agent}]  ─►  backend
            (the agent's contract)              (--format json / inject)
```

## 1. Agent output — what an agent (or `SNIFFR_CMD`) must print

An agent receives the review prompt (instructions + the unified diff) on **stdin**
(or as an argument, per that agent's CLI) and must print **only** a JSON array to
**stdout** — no prose, no markdown fences. Each element:

| field  | type              | meaning |
|--------|-------------------|---------|
| `file` | string            | path on the **new** side of the diff |
| `code` | string            | the **one** line the finding is about, copied **verbatim** from a `+` or context line in the diff, **without** the leading `+`/space. This is what anchors the finding. |
| `line` | integer \| `null` | new-side line number if the agent is certain; else `null`. Only a **fallback** — `code` wins. |
| `type` | string            | one of `bug` \| `risk` \| `question` \| `note` (category) |
| `severity` | string \| omit | `critical` \| `high` \| `medium` \| `low` (impact). Optional. |
| `confidence` | number \| omit | `0.0`–`1.0`, how sure the agent is. Optional; used internally for ranking/filtering, never shown. |
| `body` | string            | the finding, concise |
| `recommendation` | string \| omit | one-line concrete fix. Optional; rendered as `↳ fix: …`. |

`type`/`severity`/`confidence`/`recommendation` are **additive & optional** — a tool that omits them still works. Empty array = nothing significant. Example:

```json
[
  {"file":"auth.py","code":"    query = \"… WHERE token = '\" + token + \"'\"","line":null,"type":"bug","severity":"critical","confidence":0.95,"body":"SQL injection via string concatenation.","recommendation":"Use a parameterized query."},
  {"file":"auth.py","code":"    if token == None:","line":null,"type":"risk","severity":"low","confidence":0.8,"body":"Use `is None`, not `== None`."}
]
```

**Why `code` and not just `line`?** Agents are unreliable at counting diff lines
but reliable at quoting them. sniffr parses the diff itself and resolves each
`code` quote to the real new-side line number; `line` is only used to break ties
when the same quote appears more than once. A quote that can't be matched
degrades to a file-level comment rather than landing on the wrong line.

Wire any tool in via `SNIFFR_CMD='<command>'`: it gets the prompt on stdin and
must print this array on stdout.

## 2. Resolved findings — what `--format json` emits / a backend receives

After anchoring, sniffr emits the resolved array. `code` is gone (consumed by the
resolver), `line` is now the **real** resolved line, and each finding is stamped
with the `agent` that produced it (so multi-agent runs stay attributable):

| field   | type              | meaning |
|---------|-------------------|---------|
| `file`  | string            | new-side path |
| `line`  | integer \| `null` | resolved new-side line; `null` → file-level comment |
| `type`  | string            | `bug` \| `risk` \| `question` \| `note` |
| `severity` | string \| omit | `critical` \| `high` \| `medium` \| `low` (if the agent set it) |
| `body`  | string            | the finding |
| `recommendation` | string \| omit | one-line fix |
| `agent` | string            | which agent flagged it (`codex`, `claude`, …) |

```json
[
  {"file":"auth.py","line":8,"type":"bug","severity":"critical","body":"SQL injection via string concatenation.","recommendation":"Use a parameterized query.","agent":"codex"},
  {"file":"auth.py","line":6,"type":"risk","severity":"low","body":"Use `is None`, not `== None`.","agent":"codex"}
]
```

`sniffr <pr> --format json` prints exactly this to stdout and exits — no reviewer
needed. It's the building block: pipe it into your own tool, a file, or a review
API. (`confidence` is dropped from the delivered comment but kept in `--format
json`.)

### 2a. Merged (consensus) findings

With `--consensus` (or `[consensus].model` set) and several agents, sniffr merges
the pool into one finding per bug. Same shape, except **`agent` (string) becomes
`agents` (array)** — the reviewers that agreed — and near-duplicate findings are
collapsed:

```json
[
  {"file":"auth.py","line":8,"severity":"critical","type":"bug","body":"SQL injection… Trigger: a crafted token alters the WHERE clause.","recommendation":"Bind the value as a query parameter.","agents":["codex","claude","cursor"]}
]
```

The comment renders as `🔴 critical · bug · 3 agents (codex·claude·cursor)`.

## 3. Custom backend — consume resolved findings

A custom backend receives the **resolved** array (shape #2) on the **stdin** of
its `inject` command. Define it in `~/.config/sniffr/config.toml`:

```toml
[backends.myreviewr]
open   = "myreviewr open {url}"   # optional; {url} {repo} {num} {diff}
inject = "myreviewr import --stdin"
```

Placeholders expanded in both commands: `{url}`, `{repo}`, `{num}`, `{diff}`
(path to the PR's unified-diff file). `open` runs first (optional), then the
resolved findings JSON is piped to `inject`.

Equivalent, without config:

```bash
sniffr <pr> --format json | myreviewr import --stdin
```
