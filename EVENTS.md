# Events

Both contracts publish events via `env.events().publish((topics...), data)`.
Soroban events have two parts:

- **Topics** — the tuple passed as the first argument. The first topic is
  always a short symbol naming the event; any further topics are indexed
  context values.
- **Data** — the second argument, an XDR value (or tuple of values) carrying
  the rest of the payload.

This is the complete list of events emitted by `project-registry` and
`milestone-vault`, for anyone building a second consumer (indexer,
monitoring script) against them.

## project-registry

Source: `contracts/project-registry/src/lib.rs`

| Event | Topics | Data | Emitted when |
|---|---|---|---|
| `register` | `("register", project_id: u64)` | `(operator: Address, name: String)` | A new project is registered via `register`. |
| `approved` | `("approved", project_id: u64)` | `()` | An admin approves a project via `approve_project`. |

## milestone-vault

Source: `contracts/milestone-vault/src/lib.rs`

| Event | Topics | Data | Emitted when |
|---|---|---|---|
| `deposit` | `("deposit", project_id: u64, donor: Address)` | `(amount: i128, total_deposited: i128)` | A donor deposits into a project's vault via `deposit`. `total_deposited` is the vault's new running total, not just this deposit. |
| `schedule` | `("schedule", project_id: u64)` | `milestone_count: u32` | An admin sets a project's tranche schedule via `configure_milestones`. |
| `attested` | `("attested", project_id: u64, milestones_completed: u32)` | `payout: i128` | The attestor confirms a milestone via `attest_milestone`. `milestones_completed` is the count *after* this attestation (i.e. this milestone's 1-based index); `payout` may be `0` if the tranche's `payout_bps` is `0`. |
| `pause` | `("pause",)` | `()` | An admin pauses the vault via `pause`. |
| `unpause` | `("unpause",)` | `()` | An admin unpauses the vault via `unpause`. |
| `attestor` | `("attestor", project_id: u64)` | `()` | An admin rotates a project's attestor via `set_attestor`. The new attestor address is not included in the event — read it back with `get_vault`. |
| `cancelled` | `("cancelled", project_id: u64)` | `()` | An admin cancels a project via `cancel_project`. |
| `refund` | `("refund", project_id: u64, donor: Address)` | `refund_amount: i128` | A donor claims their refund from a cancelled project via `refund`. |

## Notes for consumers

- Event topic symbols are created with `symbol_short!`, which limits them
  to 9 characters — the strings above (`"register"`, `"approved"`,
  `"deposit"`, `"schedule"`, `"attested"`, `"pause"`, `"unpause"`,
  `"attestor"`, `"cancelled"`, `"refund"`) are the exact topic values, not
  abbreviations.
- Neither contract emits an event on `init`, `configure_milestones`
  failing validation, or reads (`get_project`, `get_vault`,
  `get_donation`, `get_schedule`, `admin`, `paused`) — those are plain
  contract calls with no on-chain event.
- To reconstruct full vault/project state, pair these events with the
  corresponding `get_*` read calls rather than relying on event data
  alone (e.g. `attestor` doesn't carry the new attestor address).
