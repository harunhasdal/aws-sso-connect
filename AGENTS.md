# AGENTS.md

## Project overview

**aws-sso-connect** is a cross-platform CLI tool that auto-discovers all AWS accounts and IAM roles available via an AWS SSO session, then generates or updates `~/.aws/config` with named profiles for each account/role combination. It safely merges new profiles into the existing config without losing manually-configured profiles or settings.

## Project structure

```
.
├── Cargo.toml          # Rust package manifest and dependencies
├── .gitignore          # Ignores target/, .DS_Store, IDE files
├── readme.md           # User-facing documentation
├── AGENTS.md           # This file — AI agent guidance
└── src/
    └── main.rs         # Single-file CLI application
```

This is a standard Rust binary project. All application logic lives in `src/main.rs`.

## Tech stack

| Layer | Technology |
|-------|-----------|
| Language | Rust (2021 edition) |
| Async runtime | tokio |
| AWS SDK | aws-sdk-sso, aws-config |
| CLI parsing | clap (derive macros) |
| Serialization | serde, serde_json |
| Date/time | chrono |
| Regex | regex |
| Cross-platform paths | dirs |

## Build commands

```sh
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run directly without installing
cargo run -- --sso-session my-sso

# Run tests (when added)
cargo test

# Check for compile errors without building
cargo check

# Lint
cargo clippy

# Format code
cargo fmt
```

## Binary output

- Debug: `target/debug/aws-sso-connect`
- Release: `target/release/aws-sso-connect`

## Key design decisions

1. **Single binary** — no runtime dependencies, no Python/Node required on target machine.
2. **Config-safe merging** — parses the full INI file into structured sections, updates in place, preserves comments, ordering, and non-SSO profiles.
3. **Start URL auto-resolution** — reads `sso_start_url` from the `[sso-session <name>]` section so users don't need to pass it explicitly.
4. **Token from cache** — reads the SSO access token from `~/.aws/sso/cache/*.json` (same mechanism the AWS CLI uses internally).
5. **Stdout by default** — config output goes to stdout for review; `--write-config` opts into writing the file directly.

## CLI interface

```
aws-sso-connect [OPTIONS]

Options:
  --sso-session <NAME>     SSO session name (resolves start URL from config)
  --start-url <URL>        SSO start URL (auto-detected if --sso-session given)
  --region <REGION>        AWS region [default: eu-central-1]
  --output <FORMAT>        json | config [default: config]
  --config-file <PATH>     Path to AWS config [default: ~/.aws/config]
  --write-config           Write output directly to config file
```

## Development notes

- Profile names are sanitized: non-alphanumeric chars become `_`, result is lowercased.
- All status/progress messages go to stderr; only the config/JSON output goes to stdout.
