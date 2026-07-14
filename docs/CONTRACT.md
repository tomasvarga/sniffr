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
| `type` | string            | one of `bug` \| `risk` \| `question` \| `note` |
| `body` | string            | the finding, concise |

Empty array = nothing significant. Example:

```json
[
  {"file":"auth.py","code":"    query = \"SELECT * FROM users WHERE token = '\" + token + \"'\"","line":null,"type":"bug","body":"SQL injection via string concatenation."},
  {"file":"auth.py","code":"    if token == None:","line":null,"type":"risk","body":"Use `is None`, not `== None`."}
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
| `body`  | string            | the finding |
| `agent` | string            | which agent flagged it (`codex`, `claude`, …) |

```json
[
  {"file":"auth.py","line":8,"type":"bug","body":"SQL injection via string concatenation.","agent":"codex"},
  {"file":"auth.py","line":6,"type":"risk","body":"Use `is None`, not `== None`.","agent":"codex"}
]
```

`sniffr <pr> --format json` prints exactly this to stdout and exits — no reviewer
needed. It's the building block: pipe it into your own tool, a file, or a review
API.

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
