# Engine signing metadata for hosted signing

Owner: claude-fe7lgm (overseer assignment). Reviewers: claude-m26ps4
(credentials) and codex-9czdjh (consumer). Base: `feat/rust-rewrite`
`b0ee0a9a4d5`. This is the engine side of the landed plan
[RUST_HOSTED_SIGNING_ENVELOPE.md](RUST_HOSTED_SIGNING_ENVELOPE.md) (#2156):
the read-only task metadata accessor, the role carrier, and where
`binding.generation` comes from. `Action::Sign`, v1 HMAC hosts and every
single-venue build other than the adopters stay byte-identical.

## Facts on this base

- **One emission site.** Host-visible `Action::Sign { attempt, work, timeout_ms }`
  is pushed only in `dispatch_queued_actions`. `timeout_ms` is
  `ceil(Pending.deadline - now)`, where `Pending` is the signing record.
- **The signing record has a deadline but no owner.** It holds the absolute
  `deadline` (for managed work, `activate_managed` sets it to
  `min(stage deadline, lifecycle deadline)`), but no slot, owner, Context or role.
- **Managed tasks resolve by reverse lookup.** For managed HTTP,
  `Pending.request_id()` leads to `managed_http[request]`, which holds the
  Lifecycle (Context, `Attempt { request, generation }`, deadline) and
  `signing_attempt`.
- **Re-sign issues a new task.** A stale re-sign (`reprepare_http`) moves the
  same record to a NEW signing `RequestId`. The Attempt, generation and
  deadline are retained.
- **The role is discarded.** `Selection` keeps only the slot, `pinned_primary`,
  the evidence scope and `session_auth`. The `CredentialRole` and
  `PrimaryReason` are lost after `Slots::select`. The engine sees a
  `Selection` only through a nonce claim (`ClaimAuthority.selection`, under
  `dispatch-nonce`) or the non-managed `Selection::sign`.
- **Session login is not a separate path.** It is managed HTTP whose Context
  owner is `Owner::Session { generation }`. Non-managed `sign_http`,
  `send_signed` and `connect_signed` have no owner and a fixed 20 s window.
- **Consumers today.** Paradex emits no `Action::Sign`: it uses `SignPayload`
  and signs in-venue. Kraken's managed dispatch signing has no production
  caller yet. Both are consumer prerequisites, not engine work.

## API delta (feature `sign-meta`, engine)

1. **Role carrier.** Under `sign-meta`, `Selection` keeps its exact
   `CredentialRole` (set in `Slots::select`; accessor `Selection::role()`).
   Selection is built only there, so no venue literal changes.
2. **Managed admission captures it.**
   - `Managed` gains `role: Option<CredentialRole>`.
   - It is set from the nonce claim's Selection on the existing claimed path.
   - It is also set from a new entry
     `Cx::request_selected_managed(context, selection, session, method, url, body, step, pending)`.
     That entry refuses `Invalid` unless `selection.slot()` equals the
     Context Credential's slot, then forwards to `request_signed_managed`.
   - Plain `request_signed_managed` records `None`.
   - `Credential` and `Policy` are unchanged (venues build them as literals).
3. **Accessor.** `Engine::sign_meta(task: RequestId) -> Option<SignMeta>`.

   ```rust
   pub struct SignMeta {
       pub task: RequestId,
       pub owner: SignOwner,
       pub deadline_ms: f64,
       pub slot: CredentialSlot,
       pub role: CredentialRole,
   }
   pub enum SignOwner {
       Managed { request: RequestId, generation: u64 },
       Session { generation: u64 },
   }
   ```

   It returns `Some` only when all of these hold:
   - `task` is a prepared signing record whose request is managed, and
     `managed.signing_attempt == Some(task)` (the CURRENT task; an old
     re-signed task is `None`);
   - its Context names a credential;
   - its role was captured.

   The fields come from:
   - `owner`: `Session { generation }` for an `Owner::Session` Context;
     otherwise the Lifecycle `Attempt`;
   - `deadline_ms`: `Pending.deadline`, frozen at first activation and kept
     across re-sign, never `now + timeout_ms`;
   - `slot`: the Context Credential's slot.

   Non-managed and ownerless tasks are `None`, so they refuse v2 and keep v1.
4. **Hosts.**
   - WASM: an `AnyEngine::sign_meta` default `None`, forwarded by the blanket
     impl, and a `signMeta(attempt)` export in a `sign-meta` block. It returns
     a plain object or `undefined`.
   - Native: the same method.
   - Feature wiring: the wasm and native `kraken` and `paradex` features
     enable `safe-cex-engine/sign-meta`.
   - Size scope: single-venue Bybit and Bitget builds stay byte-identical. The
     combined build links the feature and reports its delta.
5. **`binding.generation`.** This is not engine data. Identities and the signer
   are frozen once the engine starts, and no event installs, replaces or clears
   credentials, so the engine has no install generation. The generation is the
   peer-confirmed account/slot generation from capability negotiation, constant
   for one engine instance, as the envelope plan states. A change reconstructs
   the account. The engine supplies `SignMeta.owner`'s generation only.

## Originals (before source)

Each runs on real admission and Prepare, with `sign-meta`:

1. A managed Mutation via `request_selected_managed` gives `Some` with owner
   Managed{Attempt}, slot Primary, role Mutation, and deadline equal to the
   Lifecycle deadline.
2. A `PrimaryRead { PostUnknownOrderRead, scope }` keeps its exact role.
   A secondary `Read { scope }` reports slot Secondary.
3. A Session-owned managed login gives owner Session{generation} and role
   SessionAuth.
4. A stale re-sign: the old task is `None`, and the new task keeps the same
   owner and deadline.
5. Plain `request_signed_managed`, non-managed `sign_http`, `send_signed`,
   unknown, finished and released tasks are all `None`.
6. A selection/Context slot mismatch on `request_selected_managed` is
   `Invalid` with nothing queued.
7. `Action::Sign`'s JSON shape is unchanged (field set pinned).
8. Feature off: the existing suites pass unchanged, and paired Bybit and
   Bitget single-venue sizes are 0.

## Gates

Engine tests with and without `sign-meta`; clippy `-D warnings`; fmt; bans
and ratchets; paired Bybit, Bitget and combined sizes on the composed tip.
Consumer adoption (Kraken managed signing, Paradex moving off `SignPayload`)
and the v2 host channel are separate PRs.
