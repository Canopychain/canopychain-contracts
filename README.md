# Canopychain — Contracts

Soroban smart contracts powering Canopychain, a milestone-verified reforestation
funding platform on Stellar. Donors fund a GPS-bounded reforestation plot;
funds release in tranches when satellite-derived forest-cover data confirms
real growth, attested on-chain by a trusted attestor service.

## Contracts

- `project-registry` — on-chain reforestation project application and registry
- `milestone-vault` — tranche-release donation vault (deposit / attest / release / refund)

## Error codes

Each contract's entry points return a numbered error code on failure. These
are the raw codes a failed transaction reports back to a caller.

### `project-registry`

| Code | Name | Meaning |
| ---- | ---- | ------- |
| 1 | `AlreadyInitialized` | `init` was called on a contract that already has an admin set. |
| 2 | `NotInitialized` | The contract has no admin set yet — `init` hasn't been called. |
| 3 | `ProjectNotFound` | No project is registered under the given project id. |

### `milestone-vault`

| Code | Name | Meaning |
| ---- | ---- | ------- |
| 1 | `AlreadyInitialized` | `init` was called on a contract that already has an admin set. |
| 2 | `NotInitialized` | The contract has no admin set yet — `init` hasn't been called. |
| 3 | `VaultNotFound` | No vault is open for the given project id. |
| 4 | `InvalidAmount` | A deposit amount was zero or negative. |
| 5 | `TokenMismatch` | A deposit's token doesn't match the token the vault was opened with. |
| 6 | `VaultCancelled` | The project's vault has been cancelled. |
| 7 | `ScheduleAlreadySet` | `configure_milestones` was called on a project that already has a schedule. |
| 8 | `InvalidSchedule` | A milestone schedule was empty, had non-increasing thresholds, or payouts summing to more than 100%. |
| 9 | `ScheduleNotFound` | No milestone schedule has been configured for the given project id. |
| 10 | `AllMilestonesComplete` | `attest_milestone` was called after every milestone in the schedule was already attested. |
| 11 | `ContractPaused` | An admin has paused the vault, blocking deposits and attestations. |
| 12 | `NotCancelled` | `refund` was called on a project whose vault hasn't been cancelled. |
| 13 | `NothingToRefund` | The calling donor has no remaining donation to refund. |

## Deploying to testnet

`scripts/deploy-testnet.sh` builds both contracts, deploys them to Stellar
testnet, initializes each with the deploying identity as admin, and writes
the resulting contract IDs to `deployments.json` at the repo root. It
requires the [`stellar` CLI](https://developers.stellar.org/docs/tools/cli).

Environment variables:

- `STELLAR_SOURCE_ACCOUNT` (required) — name of a funded testnet identity to
  deploy and initialize the contracts with. The script fails immediately if
  this is not set.

```sh
STELLAR_SOURCE_ACCOUNT=my-testnet-identity ./scripts/deploy-testnet.sh
```

## Related repositories

- [canopychain-backend](https://github.com/canopychain/canopychain-backend) — indexer, forest-cover polling & API
- [canopychain-frontend](https://github.com/canopychain/canopychain-frontend) — donor & operator web app
- [canopychain-docs](https://github.com/canopychain/canopychain-docs) — documentation

## Status

Early development.

## License

Apache-2.0 — see [LICENSE](./LICENSE).
