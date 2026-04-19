# Contributing to RustStream

Thanks for your interest in improving RustStream.

## Scope

Please keep contributions focused and easy to review:
- one clear change per commit or pull request
- avoid mixing refactors, new features, and unrelated cleanup in one pass
- prefer incremental improvements over large rewrites

## Before You Change Code

1. Read the root `README.md` and `ruststream-core/README.md`.
2. Check whether the change belongs in `ruststream-core`, `docs`, or CI.
3. If you are changing behavior, add or update tests when practical.

## Development Basics

From `ruststream-core/`:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

If your change affects performance-sensitive paths, include benchmark notes or before/after measurements when possible.

## Documentation Expectations

Please update documentation when you change:
- setup requirements
- CLI behavior
- repository structure
- workflow or contributor expectations

Historical migration notes and one-off cleanup reports belong in `docs/`, not in the repository root.

## Commit Style

Use short, descriptive commit messages such as:
- `docs: clarify FFmpeg requirement`
- `test: add probe fixture coverage`
- `ci: tighten clippy and test steps`

## Pull Requests

A good pull request should explain:
- what changed
- why it changed
- how it was validated
- any follow-up work that is still pending

## Code Style

Prefer clear, explicit Rust over clever abstractions. Keep hot paths readable, error handling specific, and public-facing docs honest about current limitations.
