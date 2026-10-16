# Security policy

## Supported versions

Only the latest release receives fixes.

## Reporting a vulnerability

Email desk@midoriresearch.com with a description and reproduction steps. Please
do not open a public issue for anything that could be exploited before a fix
ships. You will get an acknowledgement within a few days.

## What `idx` does and does not do

- It makes outbound HTTPS requests to the public data sources listed in the
  README. It never sends anything about you other than a browser-like
  `User-Agent`.
- It writes a response cache and a config file under your user directory
  (`idx cache path`, `idx config path`). The Yahoo session cookie and crumb are
  stored there too; they are anonymous tokens, not account credentials.
- It needs no API keys, accounts or secrets.
- The MCP server (future release) speaks only over stdio to the process that
  launched it.
