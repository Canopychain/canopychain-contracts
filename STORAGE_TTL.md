# Storage TTL strategy

Soroban doesn't keep ledger entries alive forever: every instance and
persistent entry has a **TTL**, expressed in ledgers, after which it is
archived and must be explicitly restored (at a fee, by anyone) before any
transaction can touch it again. Both contracts in this repo extend their
entries' TTLs on every state-changing call so that active projects never
get archived out from under their donors, operators, and recipient.

## The 5-second-ledger assumption

Both contracts derive their TTL constants from one assumed constant,
defined identically in each `lib.rs`:

```rust
const DAY_IN_LEDGERS: u32 = 17_280; // 86_400 seconds / 5 seconds per ledger
```

This assumes Stellar's current ~5-second ledger close time. If that close
time ever changes materially, the "days" figures below drift accordingly —
`DAY_IN_LEDGERS` counts ledgers, not wall-clock time, and the contracts
have no way to observe the real close time and adjust it.

## TTL windows per entry

Both contracts follow the same two-tier pattern: a shorter window for the
instance entry, a longer one for persistent, per-project entries.

| Contract | Storage | Keys | Bump amount | Extended on |
|---|---|---|---|---|
| project-registry | instance | `Admin`, `NextProjectId` | 30 days | every call to `init`, `register`, `approve_project` |
| project-registry | persistent | `Project(id)` | 90 days | `register`, `approve_project` for that project |
| milestone-vault | instance | `Admin`, `Paused` | 30 days | every call to `init`, `deposit`, `configure_milestones`, `attest_milestone`, `pause`, `unpause`, `set_attestor`, `cancel_project` |
| milestone-vault | persistent | `Vault(id)` | 90 days | `deposit`, `attest_milestone`, `set_attestor`, `cancel_project` for that project |
| milestone-vault | persistent | `Donation(id, donor)` | 90 days | `deposit`, `refund` for that donor/project |
| milestone-vault | persistent | `Schedule(id)` | 90 days | `configure_milestones` for that project |

Instance entries get the shorter window because they back admin
operations, which are expected to happen routinely; persistent per-project
entries get the longer window because a project can legitimately go quiet
for a while (waiting on forest-cover growth) without any party wanting to
interact with the contract.

## How `extend_ttl` is actually called

Every bump uses the pattern:

```rust
env.storage().<instance|persistent>().extend_ttl(&key, THRESHOLD, BUMP_AMOUNT);
```

where `THRESHOLD = BUMP_AMOUNT - DAY_IN_LEDGERS` (29 days for the 30-day
window, 89 days for the 90-day window). `extend_ttl` is a no-op unless the
entry's *remaining* TTL is already at or below `THRESHOLD`; only then does
it get bumped back out to the full `BUMP_AMOUNT`. In practice this means:

- The first time an entry is written, its TTL is set to the full 30 or 90
  days.
- Repeated calls within the same ~day don't re-write the TTL (saving fees
  on entries touched multiple times in quick succession, e.g. several
  donors depositing into the same project the same day).
- Any state-changing call that touches the entry within its window resets
  it to a full fresh 30/90 days, indefinitely, for as long as the project
  stays active.

## What happens if a project goes quiet

If no state-changing call touches a project's entries before its TTL
expires, that entry (not the whole contract) is archived by the network:

- **Persistent entries** (`Project`, `Vault`, `Donation`, `Schedule`) for
  that specific project archive after **90 days** of inactivity on that
  project. Other projects' entries are unaffected — TTL is per-key, not
  per-contract.
- **Instance entries** (`Admin`, `NextProjectId`/`Paused`) archive after
  **30 days** with no calls to *any* state-changing entry point on that
  contract at all, since every one of them refreshes the instance TTL.

Archival doesn't destroy the data — Soroban keeps it, just not "live." But
any transaction whose footprint includes an archived entry fails until
that entry is restored. Concretely, once a project's persistent entries
archive:

- `deposit`, `attest_milestone`, `set_attestor`, `cancel_project`, and
  `refund` for that project all fail until the relevant entries are
  restored.
- If the instance entry itself has also archived, *every* call to that
  contract fails until it's restored, including reads like `admin` or
  `paused`.
- No funds are lost — restoring an entry brings back its exact
  last-written value, including `total_deposited`, `total_released`, and
  every donor's recorded donation.

Restoring an archived entry is a network-level operation (a
`RestoreFootprint` operation, or `stellar contract restore` via the CLI)
that anyone can submit and pay for — it requires no admin, attestor, or
donor authorization, since it doesn't change contract state, only revives
it. Whoever next wants to interact with a stalled project (the operator,
an admin, even a donor trying to claim a refund) can restore it themselves
and then proceed. Deployers should budget for this when a project is
expected to be quiet for a stretch approaching 90 days: either restore
proactively, or expect the first interaction after a long gap to require
an extra restore step first.
