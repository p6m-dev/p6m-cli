# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

P6m CLI (`p6m`) is a Rust-based developer productivity tool for the p6m development platform. It handles repository management, organization context switching, SSO/auth integration (Auth0, AWS, Azure), and workstation environment validation.

## Essential Commands

```bash
# Build
cargo build --release

# Run tests
cargo test -- --show-output

# Run a single test
cargo test test_name -- --show-output

# Install locally for manual testing
cargo install --force --path .

# Lint and format
cargo fmt --check
cargo clippy -- -D warnings
```

## Architecture

### Bootstrap Flow (main.rs)

```
CLI parsing (clap) → logging init → P6mEnvironment::init() → subcommand dispatch
```

Errors bubble up as `anyhow::Result<T>` and are printed as chained messages at the top level.

### Module Map

| Module | Purpose |
|--------|---------|
| `cli.rs` | Clap v4 command definitions + `P6mEnvironment` struct |
| `auth/` | `TokenRepository` — token read/write/refresh lifecycle, OpenID Connect device flow, claims assertion |
| `auth0/` | Auth0 HTTP client (`api.rs`) and domain types (`types.rs`: `AuthN`, `App`, `AuthToken`) |
| `sso/` | Kubernetes cluster SSO config: `auth0.rs` (primary), `aws.rs`, `azure.rs`, `vcluster.rs` |
| `context.rs` | Org context switching — renders MiniJinja templates for Maven, NPM, Poetry, Cargo |
| `models/` | Domain types: `artifact.rs` (StorageProvider), `git.rs` (GithubLevel), `aws.rs`, `azure.rs` |
| `workstation/` | `check/` has per-ecosystem validators (Docker, Java, JS, Python, .NET, K8s, Git, self-update) |
| `login.rs` | Interactive device-code login flow |
| `whoami.rs` | User info display; `--output k8s-auth` mode used as kubectl exec credential plugin |
| `repositories.rs` | GitHub org repo clone/push via octocrab |
| `open.rs` | Opens org resources (GitHub, ArgoCD, Artifactory) in browser |
| `jwt/` | Insecure JWT generation for local development |
| `tilt.rs` | Tiltfile generation from templates |
| `purge.rs` | IDE file and Maven cache cleanup |

### Key Abstractions

**TokenRepository** (`auth/token_repository.rs`) — Central auth abstraction. Builder-style: `new() → with_organization() → with_scope() → try_login()/try_refresh()`. Manages token files under `~/.p6m/auth/` with org and app subdirectories.

**GithubLevel** (`models/git.rs`) — Enum representing Enterprise → Organization → Repository hierarchy. Detects context from `~/orgs/` path structure. Used by `repos`, `open`, and `context` commands.

**P6mEnvironment** (`cli.rs`) — Holds config paths (`~/.p6m` or `~/.p6m-dev` in dev mode), Auth0 settings, and kube dir. Passed to auth-dependent subcommands.

**Claims** (`auth/token_repository.rs`) — JWT claims with p6m-namespaced fields (`https://p6m.dev/v1/*`). Supports assertion logic with wildcards and merging.

### Template System

`resources/` contains MiniJinja templates rendered by `context.rs`:
- `settings.xml` — Maven
- `npmrc` — NPM
- `poetry/auth.toml.j2`, `poetry/config.toml.j2` — Poetry
- `cargo/credentials.toml.j2` — Cargo
- `aws_config` — AWS CLI config
- `Tiltfile` — Tilt dev environment

Templates branch on `StorageProvider` (Artifactory vs Cloudsmith).

### Build-Time Code Generation

`build.rs` generates `version_constants.rs` with `GIT_COMMIT_HASH` and `GIT_IS_DIRTY` from git state.

## Environment Variables

```bash
# Artifactory (for `p6m context`)
ARTIFACTORY_USERNAME=your-email@domain.com
ARTIFACTORY_IDENTITY_TOKEN=your-token

# GitHub (for `p6m repos`)
GITHUB_TOKEN=your-github-token
```

## Release Process

Releases are triggered manually via the GitHub Actions "Publish new version" workflow dispatch (major/minor/patch). Uses `cargo-workspaces` for version bumping. Builds natively on 5 platform runners (Linux x86_64/arm64, macOS x86_64/arm64, Windows x86_64). Distributes via GitHub Releases, Homebrew tap (`p6m-dev/homebrew-tap`), and winget.
