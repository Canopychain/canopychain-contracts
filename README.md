# Canopy — Contracts

Soroban smart contracts powering Canopy, a milestone-verified reforestation
funding platform on Stellar. Donors fund a GPS-bounded reforestation plot;
funds release in tranches when satellite-derived forest-cover data confirms
real growth, attested on-chain by a trusted attestor service.

## Contracts

- `project-registry` — on-chain reforestation project application and registry
- `milestone-vault` — tranche-release donation vault (deposit / attest / release / refund)

## Related repositories

- [canopy-backend](https://github.com/canopy/canopy-backend) — indexer, forest-cover polling & API
- [canopy-frontend](https://github.com/canopy/canopy-frontend) — donor & operator web app
- [canopy-docs](https://github.com/canopy/canopy-docs) — documentation

## Status

Early development.

## License

Apache-2.0 — see [LICENSE](./LICENSE).
