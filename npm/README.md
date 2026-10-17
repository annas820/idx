# @midoriresearch/idx

npm wrapper for [idx](https://github.com/fzn0x/idx), the Indonesian stock
market (IDX / IHSG) CLI and MCP server. `postinstall` downloads the prebuilt
binary for your platform from the matching GitHub release and verifies its
SHA-256 against the release's `SHA256SUMS`.

```sh
npx @midoriresearch/idx stocks quote BBCA
npm i -g @midoriresearch/idx && idx stocks fundamental TLKM

# MCP for Claude Code without a global install
claude mcp add idx -- npx -y @midoriresearch/idx mcp
```

Platforms: Linux x64/arm64 (static musl), macOS x64/arm64, Windows x64.
Prefer `install.sh` or `cargo install --git https://github.com/fzn0x/idx` if
you do not want Node involved. MIT.
