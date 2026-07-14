Set up sniffr on my machine — a CLI that, pointed at a GitHub PR, runs an AI
agent over the diff and drops local-draft review comments on the risky lines,
either into a terminal reviewer I already have open (tuicr/hunk) or as JSON I can
pipe anywhere. Work through this, asking me only when there's a real choice; show
me any install command before running it. sniffr never posts to GitHub (comments
are local drafts), so don't push code or post to a PR.

1. If `sniffr` isn't installed, run:
   curl -fsSL https://raw.githubusercontent.com/tomasvarga/sniffr/main/install.sh | bash
   and make sure ~/.local/bin is on my PATH.
2. Run `sniffr doctor` and fix each ✗: install missing core deps (gh, jq,
   python3). If gh isn't authenticated, tell me to run `gh auth login` myself.
3. Backend (where comments land): sniffr injects into a reviewer I open first and
   auto-detects tuicr → hunk. If neither is installed, ask which I prefer and
   install it (tuicr: tuicr.dev; hunk: `brew install modem-dev/tap/hunk`). For a
   non-default choice write `backend = "hunk"` to ~/.config/sniffr/config.toml.
   (I can also skip a reviewer entirely with `sniffr <pr> --format json`.)
4. Agent: detect which of codex/claude/cursor-agent/grok/opencode/ollama is
   installed + authenticated, then `sniffr --set-agent <name>`. If none, tell me
   which to install.
5. Verify: `sniffr doctor` (all ✓), then `sniffr <a real PR of mine> --format
   json` and confirm it prints anchored findings. If I use a reviewer, show me
   the attach flow: open `tuicr pr <pr>` in one pane, then `sniffr <pr>` in
   another, and confirm draft comments appear. Remind me they're local drafts I
   prune and submit myself.
