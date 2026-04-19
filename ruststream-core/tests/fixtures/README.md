# Test Fixtures

This directory is reserved for small media fixtures used by integration and smoke tests.

## Goals

- keep fixture files tiny and deterministic
- prefer formats that are easy to validate in CI
- avoid large binary assets in the repository root
- document every fixture in `manifest.toml`

## Planned fixture set

- `sample.wav` for audio decoding and probe smoke tests
- `sample.mp4` for metadata and concat smoke tests
- `invalid.bin` for invalid-media error-path tests

## Rules

- keep individual fixtures small enough for fast CI
- do not add large production media here
- update `manifest.toml` when adding or replacing a fixture
