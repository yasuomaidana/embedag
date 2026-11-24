# Contributing to embedag

You do not need Dendrite to contribute. Fork, branch, change, and open a pull request as usual.

## Checks to run before a pull request

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check licenses bans   # licence policy, see deny.toml
```

## Dendrite is optional

The maintainers organise work with [Dendrite](https://github.com/yasuomaidana) tickets, and the repository carries the files it generates (`AGENTS.md`, `.agents/`, `.claude/`, `.gemini/`, `.githooks/`). They never block you when `dendrite` is not installed:

- `.githooks/pre-commit` only runs when you enable it with `git config core.hooksPath .githooks`. Without `dendrite` on your `PATH` it prints a note and lets the commit through.
- If you do use Dendrite but want to commit without a ticket claim, set `DENDRITE_TICKET_GUARD=off` for that command (it skips the ticket lock only).

## Licences

`deny.toml` allows permissive licences, and BUSL-1.1 only from projects whose source repository is public (`scripts/check-busl-public-repos.py` checks that). A pull request that adds a dependency under another licence will fail `cargo deny check licenses`.
