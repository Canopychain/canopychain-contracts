# Canopychain — Contracts

Soroban smart contracts powering Canopychain, a milestone-verified reforestation
funding platform on Stellar. Donors fund a GPS-bounded reforestation plot;
funds release in tranches when satellite-derived forest-cover data confirms
real growth, attested on-chain by a trusted attestor service.

## Contracts

- `project-registry` — on-chain reforestation project application and registry
- `milestone-vault` — tranche-release donation vault (deposit / attest / release / refund)

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
