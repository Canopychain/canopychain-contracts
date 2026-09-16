# Contributing

This file covers the toolchain and checks for working in this repository.
For the full contribution guide (issue triage, PR process, coding
conventions across the Canopychain project), see
[canopychain-docs](https://github.com/canopychain/canopychain-docs).

## Toolchain

- Rust (stable), with the `wasm32v1-none` target and the
  `rustfmt` and `clippy` components:

  ```sh
  rustup target add wasm32v1-none
  rustup component add rustfmt clippy
  ```

- The [`stellar` CLI](https://developers.stellar.org/docs/tools/cli) if
  you're deploying to testnet (see `scripts/deploy-testnet.sh`).

## Building

```sh
cargo build --workspace --target wasm32v1-none --release
```

## Checks to run before opening a PR

CI (`.github/workflows/ci.yml`) runs the following against every push and
pull request; run them locally first so review isn't spent on things CI
would catch:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --target wasm32v1-none --release
cargo test --workspace
```

## Repository layout

- `contracts/project-registry` — on-chain reforestation project
  application and registry
- `contracts/milestone-vault` — tranche-release donation vault (deposit /
  attest / release / refund)

Each contract's tests live alongside it in `src/test.rs`.

## Security

Please don't open a public issue for a security vulnerability — see
[SECURITY.md](./SECURITY.md) for how to report one privately.
