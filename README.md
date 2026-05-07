# living-postmortem-mcp

MCP server for incident postmortems, built with Rust and `rmcp` over stdio.

[![crates.io](https://img.shields.io/crates/v/living-postmortem-mcp.svg)](https://crates.io/crates/living-postmortem-mcp)
[![docs.rs](https://docs.rs/living-postmortem-mcp/badge.svg)](https://docs.rs/living-postmortem-mcp)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Zandelok/living-postmortem-mcp/blob/main/LICENSE)

It stores incidents as markdown files in your vault and exposes 4 tools:

- `create_incident`
- `add_timeline_entry`
- `search_similar_incidents`
- `resolve_incident`

## What it does

- Creates incident postmortem markdown files under your vault
- Keeps timeline entries and resolution updates in the same file
- Searches similar incidents by lexical overlap (title + root cause + patterns)
- Uses a status-emoji markdown template with sections for:
  - Timeline
  - Root Cause
  - Action Items (Detect / Mitigate / Prevent)
  - Patterns

## Requirements

- Rust toolchain (stable)
- An MCP host/client that supports stdio servers
- A writable vault directory path

## Install

### Option 1: Install published release (crates.io)

```bash
cargo install living-postmortem-mcp --version 0.1.1
```

### Option 2: Build from source

```bash
git clone https://github.com/Zandelok/living-postmortem-mcp.git
cd living-postmortem-mcp
cargo build --release
```

Binary path:

```bash
./target/release/living-postmortem-mcp
```

### Option 3: Run directly in dev mode

```bash
cargo run -- --vault /absolute/path/to/vault
```

### Published package links

- Crate: https://crates.io/crates/living-postmortem-mcp
- API docs: https://docs.rs/living-postmortem-mcp
- Source: https://github.com/Zandelok/living-postmortem-mcp

## Maintainer release (trusted publishing)

This repository publishes through crates.io trusted publishing (OIDC) via:

- `.github/workflows/publish.yml`

### One-time crates.io setup

In crates.io crate settings (`living-postmortem-mcp`), add a Trusted Publisher with:

- Owner/organization: `Zandelok`
- Repository: `living-postmortem-mcp`
- Workflow file: `publish.yml`
- Environment: leave empty unless you later add a GitHub Actions environment gate

### Release workflow usage

- Manual validation only: run `Publish crate` with `dry_run=true` (default).
- Real publish: push a tag in the form `v<version>` (for example `v0.1.1`).
- Safety guard: workflow checks that tag version matches `Cargo.toml` `version` before publishing.

## Server CLI

```bash
living-postmortem-mcp --vault <path>
```

Notes:

- `--vault` is required
- The server auto-creates `<vault>/incidents/`
- Files are saved as markdown: `<vault>/incidents/<incident_id>.md`

## MCP client setup

Use the compiled binary with stdio transport and pass `--vault`.

### Cursor (`.cursor/mcp.json`)

```json
{
  "mcpServers": {
    "living-postmortem": {
      "command": "/absolute/path/to/living-postmortem-mcp/target/release/living-postmortem-mcp",
      "args": ["--vault", "/absolute/path/to/your/vault"]
    }
  }
}
```

### Claude Desktop (`claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "living-postmortem": {
      "command": "/absolute/path/to/living-postmortem-mcp/target/release/living-postmortem-mcp",
      "args": ["--vault", "/absolute/path/to/your/vault"]
    }
  }
}
```

## Tool reference

### `create_incident`

Create a new incident markdown file.

Input:

- `title` (string, required)
- `root_cause` (string, optional)
- `detect_actions` (string[], optional)
- `mitigate_actions` (string[], optional)
- `prevent_actions` (string[], optional)
- `patterns` (string[], optional)
- `initial_timeline` (string[], optional)

Returns:

- `incident_id`
- `file_path`
- `status` (`open`)

### `add_timeline_entry`

Append a timeline entry to an existing incident.

Input:

- `incident_id` (string, required)
- `summary` (string, required)
- `timestamp` (RFC3339 string, optional; defaults to now)

Returns:

- `incident_id`
- `file_path`
- `entry` (rendered markdown timeline line)

### `search_similar_incidents`

Find related incidents via lexical token overlap.

Input:

- `query` (string, required)
- `limit` (number, optional, default `5`, clamped `1..50`)

Returns:

- `query`
- `incidents[]` with:
  - `incident_id`
  - `title`
  - `score`
  - `matched_terms`

### `resolve_incident`

Mark an incident as resolved and optionally append a resolution summary.

Input:

- `incident_id` (string, required)
- `resolution_summary` (string, optional)
- `resolved_at` (RFC3339 string, optional; defaults to now)

Returns:

- `incident_id`
- `status` (`resolved`)
- `resolved_at`
- `file_path`

## Markdown output format

Each incident file contains frontmatter and a structured body:

- Frontmatter: `incident_id`, `status`, `created_at`, `resolved_at`
- Header: `# <emoji> <title>`
- `## Timeline`
- `## Root Cause`
- `## Action Items`
  - `### Detect`
  - `### Mitigate`
  - `### Prevent`
- `## Patterns`

## Development

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

## License

MIT (see `LICENSE`).
