# @midoriresearch/idx

npm wrapper for [idx](https://github.com/fzn0x/idx), the Indonesian stock
market (IDX / IHSG) CLI and MCP server. The prebuilt binary for your platform
is downloaded from the matching GitHub release and verified against the
release's `SHA256SUMS`: at `postinstall` when npm runs install scripts, or on
the first `idx` command when it does not (npm 12 disables dependency scripts
by default; `--ignore-scripts` and some CI images do too). Set
`IDX_SKIP_DOWNLOAD=1` to always defer to first run.

```sh
npx @midoriresearch/idx stocks quote BBCA
npm i -g @midoriresearch/idx && idx stocks fundamental TLKM

# MCP for Claude Code without a global install
claude mcp add idx -- npx -y @midoriresearch/idx mcp
```

Platforms: Linux x64/arm64 (static musl), macOS x64/arm64, Windows x64.
Prefer `install.sh` or `cargo install --git https://github.com/fzn0x/idx` if
you do not want Node involved. MIT.
