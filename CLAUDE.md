# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

P6m CLI is a Rust-based developer productivity tool for organizations using the p6m development platform. It provides repository management, context switching, SSO integration, and workstation environment validation across multiple cloud providers and development ecosystems.

## Essential Commands

### Build and Test
```bash
# Build the project
cargo build --release

# Run tests with output
cargo test -- --show-output

# Install locally for testing
cargo install --force --path .
```

### Development Workflow
```bash
# Check code formatting
cargo fmt --check

# Run clippy for linting
cargo clippy -- -D warnings

# Build for all platforms (requires cross-compilation setup)
cargo build --target x86_64-pc-windows-gnu
cargo build --target x86_64-apple-darwin
cargo build --target aarch64-apple-darwin
```

## Architecture Overview

### Core Module Structure
- **cli.rs**: Main CLI definition using Clap with comprehensive subcommands
- **auth/**: Authentication and JWT token management with OpenID Connect
- **sso/**: Cloud provider SSO integration (AWS, Azure, Auth0)
- **workstation/**: Development environment validation and setup checks
- **repositories.rs**: GitHub organization repository management
- **context.rs**: Organization context switching with template-based configuration

### Key Design Patterns
- **Template-driven configuration**: Uses MiniJinja templates in `resources/` to generate tool-specific configs (Maven, NPM, Poetry, etc.)
- **Context switching**: Automatically configures local development environment when switching between organization contexts
- **Async throughout**: All I/O operations use Tokio for async execution
- **Error handling**: Uses `anyhow` for user-friendly error messages with context

### Configuration Templates
The `resources/` directory contains Jinja2 templates for:
- Maven settings.xml
- NPM .npmrc
- Poetry auth/config.toml
- Cargo credentials.toml
- AWS config

## Environment Requirements

### Required Environment Variables
```bash
# For Artifactory operations
ARTIFACTORY_USERNAME=your-email@domain.com
ARTIFACTORY_IDENTITY_TOKEN=your-token

# For GitHub operations (repository management)
GITHUB_TOKEN=your-github-token
```

### OAuth/SSO Configuration
The CLI integrates with Auth0 for platform authentication and manages AWS/Azure SSO profiles automatically. Token storage is handled securely through the auth module.

## Testing Strategy

- Unit tests are embedded within modules (following Rust conventions)
- Key test coverage in `auth/token_repository.rs` for authentication flows
- CI/CD runs smoke tests across Ubuntu, macOS, and Windows
- Integration testing occurs through multi-platform builds in CI

## Cross-Platform Considerations

- Windows support with Inno Setup installer generation
- macOS support with Homebrew tap distribution
- Alpine Linux Docker container for containerized usage
- Cross-compilation configured for multiple targets in CI

## Version Management

- Version defined in Cargo.toml (currently 0.7.3)
- Git-based versioning via build.rs
- Automated version bumping through GitHub Actions
- Self-update checking capability built into CLI