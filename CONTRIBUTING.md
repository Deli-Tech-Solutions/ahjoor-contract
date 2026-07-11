# Contributing to ahjoorxmr-contract

Thank you for your interest in contributing! This guide covers everything you need to get started — from forking the repo to opening a well-formed pull request.

---

## Table of Contents

- [Getting Started](#getting-started)
- [Branch Naming](#branch-naming)
- [Development Workflow](#development-workflow)
- [Pre-PR Checklist](#pre-pr-checklist)
- [Opening a Pull Request](#opening-a-pull-request)
- [Reporting Bugs](#reporting-bugs)
- [Requesting Features](#requesting-features)
- [Code Style](#code-style)

---

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- `wasm32-unknown-unknown` target:
    ```bash
    rustup target add wasm32-unknown-unknown
    ```
- [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) for coverage:
    ```bash
    cargo install cargo-llvm-cov --locked
    ```

### Fork & Clone

1. **Fork** the repository on GitHub using the _Fork_ button at the top right.

2. **Clone** your fork locally:

    ```bash
    git clone https://github.com/<your-username>/ahjoorxmr-contract.git
    cd ahjoorxmr-contract
    ```

3. **Add the upstream remote** so you can keep your fork in sync:

    ```bash
    git remote add upstream https://github.com/ussyalfaks/ahjoorxmr-contract.git
    git fetch upstream
    ```

4. **Keep your fork up to date** before starting new work:
    ```bash
    git checkout main
    git pull upstream main
    git push origin main
    ```

---

## Branch Naming

Create a dedicated branch for every contribution. Use one of the following prefixes:

| Prefix   | When to use                                 |
| -------- | ------------------------------------------- |
| `feat/`  | New feature or enhancement                  |
| `fix/`   | Bug fix                                     |
| `docs/`  | Documentation only changes                  |
| `test/`  | Adding or improving tests                   |
| `chore/` | Maintenance tasks (deps, CI, tooling, etc.) |

**Examples:**

```bash
git checkout -b feat/add-milestone-cancellation
git checkout -b fix/escrow-timeout-edge-case
git checkout -b docs/contributing-guidelines
```

Keep branch names lowercase, hyphen-separated, and descriptive.

---

## Development Workflow

All contracts live under `contracts/`. Each contract is an independent Cargo workspace member.

### Run tests for a specific contract

```bash
cargo test --manifest-path contracts/<contract-name>/Cargo.toml
```

### Run tests for all contracts

```bash
cargo test --workspace
```

### Build WASM artifacts

```bash
cargo build \
  --manifest-path contracts/<contract-name>/Cargo.toml \
  --target wasm32-unknown-unknown \
  --release
```

### Format code

```bash
cargo fmt --all
```

### Lint (deny warnings)

```bash
RUSTFLAGS="-Dwarnings" cargo clippy --all-targets --all-features
```

### Check coverage thresholds

Coverage must meet **≥ 90% line** and **≥ 85% region** coverage per contract. Verify locally before pushing:

```bash
cargo llvm-cov \
  --manifest-path contracts/<contract-name>/Cargo.toml \
  --summary-only \
  --fail-under-lines 90 \
  --fail-under-regions 85
```

---

## Pre-PR Checklist

Run through every item below before opening a pull request:

- [ ] Branch follows the naming convention (`feat/`, `fix/`, `docs/`, etc.)
- [ ] `cargo fmt --all` — no formatting changes outstanding
- [ ] `RUSTFLAGS="-Dwarnings" cargo clippy --all-targets --all-features` — zero warnings
- [ ] `cargo test --workspace` — all tests pass
- [ ] `cargo llvm-cov` coverage thresholds pass (≥ 90% lines, ≥ 85% regions) for affected contracts
- [ ] New functionality is covered by tests
- [ ] Documentation/comments updated where relevant
- [ ] PR description is filled out using the [PR template](.github/PULL_REQUEST_TEMPLATE.md)
- [ ] Related issue is linked with `Closes #<issue-number>` in the PR description

---

## Opening a Pull Request

1. Push your branch to your fork:

    ```bash
    git push -u origin <your-branch-name>
    ```

2. Open a PR from your fork's branch against `ussyalfaks/ahjoorxmr-contract:main` on GitHub.

3. Fill in all sections of the [PR template](.github/PULL_REQUEST_TEMPLATE.md) — partial descriptions slow down review.

4. Link the issue your PR addresses using `Closes #<number>` in the description. This automatically closes the issue when the PR merges.

5. Wait for CI to pass. All checks (tests, WASM build, coverage gate) must be green before review.

6. Address review feedback by pushing additional commits to the same branch. Do **not** force-push after a review has started.

---

## Reporting Bugs

Use the [Bug Report template](.github/ISSUE_TEMPLATE/bug_report.md). Please include:

- A clear description of the problem
- Steps to reproduce
- Expected vs. actual behaviour
- Your Rust version (`rustc --version`) and OS

Search existing issues before opening a new one to avoid duplicates.

---

## Requesting Features

Use the [Feature Request template](.github/ISSUE_TEMPLATE/feature_request.md). Please include:

- The problem or limitation you're facing
- Your proposed solution
- Alternatives you considered
- Any relevant use cases

---

## Code Style

- Follow standard Rust conventions enforced by `rustfmt` and `clippy`.
- Keep functions small and focused; add doc comments (`///`) to public items.
- Treat warnings as errors — CI runs with `RUSTFLAGS="-Dwarnings"`.
- Write tests alongside new code; coverage gates are enforced in CI.

---

By contributing, you agree that your submissions are made under the same licence as the project. Thank you for helping make this project better!
