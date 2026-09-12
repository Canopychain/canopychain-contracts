# Security Policy

Canopychain's contracts are unaudited and hold donor funds. If you find a
vulnerability, please report it privately so it can be fixed before it's
disclosed publicly.

## Scope

This policy covers the Soroban contracts in this repository:

- `contracts/project-registry`
- `contracts/milestone-vault`

Vulnerabilities in related repositories (backend, frontend, docs) should be
reported against those repositories instead.

## Reporting a vulnerability

**Do not open a public GitHub issue for a security vulnerability.**

Instead, use GitHub's private vulnerability reporting:

1. Go to the [Security tab](https://github.com/Canopychain/canopychain-contracts/security) of this repository.
2. Click "Report a vulnerability" to open a private advisory.
3. Describe the issue, including steps to reproduce, the affected
   contract(s) and entry point(s), and the potential impact (e.g. loss or
   lock of donor/recipient funds, unauthorized attestation, privilege
   escalation).

If you're unable to use GitHub's private reporting for any reason, contact
a maintainer directly rather than filing a public issue.

## What to expect

- We'll acknowledge new reports as soon as we can and work with you to
  understand and confirm the issue.
- We'll aim to keep you updated as a fix is developed and let you know
  before any public disclosure.
- Please give us a reasonable amount of time to address the issue before
  disclosing it publicly.

## What qualifies

Examples of in-scope issues:

- Any path that moves, locks, or misattributes donor funds outside of the
  documented deposit / attest / release / refund flow.
- Authorization bypasses (e.g. calling an admin- or attestor-gated entry
  point without the expected `require_auth`).
- Ways to corrupt or bypass the milestone schedule or tranche-payout math.
- Storage TTL or state issues that could unexpectedly archive or lose
  contract data.

Out of scope: issues that only affect an already-compromised admin or
attestor key, since those roles are trusted by design.
