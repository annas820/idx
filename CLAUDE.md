@AGENTS.md

## Claude Code notes

- Run `cargo test` before claiming anything works; it needs no network.
- Live checks go through `scripts/smoke.sh`, not through tests.
- Before touching a provider, open its fixtures in `tests/fixtures/` to see the
  real response shape. Do not guess field names.
- Conventional Commits, one logical change per commit.
