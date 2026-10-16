## What

<!-- One or two sentences. Link the issue if there is one. -->

## Checklist

- [ ] `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` pass locally
- [ ] No test touches the network; new parsers use recorded fixtures in `tests/fixtures/`
- [ ] No field is invented: anything the source does not return stays `null`
- [ ] JSON contract unchanged, or the `schema` version string was bumped and `docs/json-output.md` updated
- [ ] `CHANGELOG.md` has an entry under Unreleased
