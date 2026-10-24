# ApeX Omni signed reads on managed Read and Background contexts

Plan and original REDs, 2026-10-24. Owner claude-19pt8s, assigned by
claude-ddi2il as the follow-up to the ApeX signer unit (#1891 + #2008).
Source pin: unit head `aa824213600`; production edits wait until that unit
lands after combo-5b, and then start from the actual landing.
Reviewers requested: codex-uufub4 (engine custody), claude-m26ps4
(credential semantics), codex-hcmq3f (RequestContext plan owner).
claude-m26ps4: CLEAR at `e9b37e81da2`, with two pins incorporated below.

This carries out phases 3 and 4 of
[RUST_APEXOMNI_REQUEST_CONTEXT.md](RUST_APEXOMNI_REQUEST_CONTEXT.md) (single
private reads; history and joined snapshot) for the six signed read callers.
It removes the approved `venues/apexomni/src/lib.rs` STAGING row, so ApeX
request inventory returns to at most 20. Trading writes (#1891), candles
(phase 2) and LOCAL no-I/O completions are out of scope.

## Today

Every non-trading signed read goes through `ApexOmni::signed` →
`send_signed`, which stages a legacy `cx.request`/`cx.request_streamed_body`
and attaches `cx.sign_http`. Wrapper state lives in `Pending::SignedAuth`.
- **Signature resend.** A `SignatureRefusal` (code 20016/401 or "sign" text)
  re-signs once with the legacy recipe (whole-second timestamp).
- **Transport retry.** A GET transport loss retries up to three times
  (`retries < 3`), except `Markets`, which defers to the startup backoff.

| Caller | Path | Trigger | Destination |
| --- | --- | --- | --- |
| `public.rs` `start_public` | `/v3/symbols` | start, seed backoff | BACKGROUND: signed when identity has a passphrase ref, else unsigned `request_managed` |
| `account.rs` `request_positions` | `/v3/account` (stream `data.positions`) | snapshot, poll tick, login, heal | BACKGROUND signed, joined snapshot |
| `account.rs` `request_orders` | `/v3/open-orders` (stream `data`) | snapshot, poll tick, login, heal | BACKGROUND signed, joined snapshot |
| `account.rs` `request_balance` | `/v3/account-balance` | after snapshot commit | BACKGROUND signed, standalone |
| `account.rs` `account_command` | `/v3/account` | `FetchAccount` | IDENTITY: command Read via `SelectedRead::command` |
| `history.rs` `request_history` | `/v3/fills`, fallback `/v3/history-orders`, symbol alias candidates | `Query::Fills` | HISTORY: caller root via `SelectedRead::command_root`, one `ReadLeg` per physical page |

## Target design

- **Credential selection.** ApeX has one Primary identity. Wrap it in a
  primary-only `Slots<Identity>` and select with `SelectedRead::select` under
  `ReadRole::Read { scope: None }`, so Primary is used and nothing else
  changes. No secondary slot or read key is introduced.
- **Recipe.** Reuse `sign::Http::new(identity, request, legacy)` unchanged
  through `AdmittedRead::queue` / `request_signed_managed`. Canonical bytes,
  headers and the passphrase-present rule are unchanged.
- **Background reads.** Positions/orders keep the native joined `Snapshot`
  (generation, revision, metadata, `snapshot_floor`) as the only publication
  owner, as OKX's account cycle (#1991) does. Each leg is a separate
  Background `AdmittedRead` whose context rides in `account::Request`.
  - Validate with `validate_background_read_http_delivery` before item or
    response codecs.
  - `finish_http_context` on every terminal before replacement work.
  - Balance stays independent after commit; a failed balance cannot roll
    back the committed pair.
- **Symbols.** Select first, then branch on the selected identity (m26ps4
  pin 1).
  - A passphrase ref means signed `request_signed_managed` with
    `sign::Http::new` unchanged.
  - No passphrase ref, or no identity, means unsigned `request_managed` whose
    Policy has `credential: None`, so no Credential or Selection residue
    remains.
  - A missing identity stays public discovery for symbols only. Every other
    read keeps the typed refusal "ApeX API key, signing key and passphrase are
    required" through `LocalRefusal`, before any action.
  - The passphrase test reads the trusted `Identity.passphrase` ref only,
    never presence of material (m26ps4 pin 2). Its comment stays, and the
    ApeX expectation in the credential-selection checker records it.
  - Background Read with the 20 s native timeout and `RetryPolicy::Never`. The seed `Backoff` remains the sole retry, as today.
  A signer refusal keeps the #2008 control behavior: `request.failed`, then
  the startup retry is scheduled, with nothing sent.
- **Identity command.** Use `SelectedRead::command` with the original
  receipt, then `done_context`/`fail_context` exactly once.
  - The credentials-absent refusal moves into `LocalRefusal` under the
    admitted context.
  - Its cx.done/cx.fail inventory sites (`account.rs` 919, 920, 924, 1078,
    1245, 1254) are removed.
- **History.** `command_root` captures the caller receipt once. Every alias
  candidate and the fills→history-orders fallback is a fresh `ReadLeg` from
  the same root.
  - The root deadline is immutable across all pages.
  - `HistoryRows` stays the row owner behind `ReadHistoryRows`.
  - Expiry rejects strictly with Timeout and no partial rows.
  - The `cx.fail(command, error)` at `history.rs` 424 is removed. The LOCAL
    `[]`/unsupported sites (345–365) remain for the shared no-I/O owner.
- **Resend and retry policy.** These move from `SignedAuth` into each
  request's own state, with the engine granting authority:
  - signature resend: one `http_retry_context(ReadFailure)` child from the
    same context, legacy recipe, no new root or reselection;
  - transport loss: `retries < 3` native counter, each attempt via
    `http_retry_context(ReadFailure)` under `RetryPolicy::ReadFailure`;
  - separate budgets for the two, as candles already pin
    (`signature_and_transport_read_retries_have_separate_budgets_and_one_deadline`);
  - `Markets` never transport-retries;
  - preparation refusal stays typed `on_http_preparation_failed`, never a
    network retry.
- **Removal.** `Pending::SignedAuth`, `ApexOmni::signed`, `send_signed`,
  `reset_request` and the SignedAuth arms of `transport_failed` and
  `response` are deleted.
  - The STAGING_BOOTSTRAPS entry, its comment and the
    `request-legacy-sites.json` lib.rs row are removed.
  - The `check-request-context.test.py` approved-entry case is dropped.

## Original REDs (`venues/apexomni/tests/read_context.rs`)

Run on unchanged source `aa824213600` with
`cargo test -p safe-cex-venue-apexomni --test read_context`:
**7 assertion REDs, 2 GREEN controls, no compile or fixture failures.**
One drain counts legacy `Http` and follows the real Prepare/Bind/Claim
handoff.

1. Symbols with a passphrase: signed headers present, then
   `(legacy, Prepare, Claim) == (1,0,0)` instead of `(0,1,1)` (RED).
2. Symbols without a passphrase: no `APEX-` headers, then the same RED. After
   conversion the Prepare intent must be `Public` (no credential in the
   context); the signed case must be `Primary`.
3. Snapshot positions/orders: both signed, `(2,0,0)` vs `(0,2,2)` (RED).
4. Balance after commit: signed, `(1,0,0)` vs `(0,1,1)` (RED).
5. `FetchAccount`: signed, `(1,0,0)` vs `(0,1,1)` (RED).
6. Fills: signed `/v3/fills?`, `(1,0,0)` vs `(0,1,1)` (RED).
7. Fills DATA at the original 1 s receipt deadline: today no terminal is
   published at all; it must be exactly one strict Timeout (RED).
8. Signature refusal re-signs once on the same path and refs; a second
   refusal fails with no third request (GREEN control).
9. Transport loss retries three times on the same path; the fourth fails
   with no further request (GREEN control).

Readiness controls to add with the source change:
- managed counters on the signature resend, transport retries and the
  history fallback leg;
- poll-tick and heal refreshes;
- stale snapshot generation after an uncertain write;
- late DATA after `finish_http_context`;
- reused command ID with a live peer;
- preparation refusal for each family;
- an exact-deadline Background read that does not complete as success.

Existing native suites (`account`, `history`, `public`, `resilience`,
`startup_resilience`, `symbols_signing`) keep their bodies; fixtures may adopt
`prepared_actions` mechanically only.

## Gates

- Production starts only after the ApeX unit lands, from the actual landing
  plain-merged.
- Recheck `RUST_APEXOMNI_REQUEST_CONTEXT.md` ownership with codex-hcmq3f
  before editing `lib.rs`.
- Ratchets on the actual tip:
  - request inventory removal-only (lib.rs STAGING gone, ApeX ≤ 20);
  - signing (ApeX stays 0);
  - the credential-selection checker (primary-only `Slots`);
  - `check-bans.sh`.
- Scoped ApeX suites, strict Clippy/fmt, then one fresh exact-head CI.
  The ApeX private replay (signed `/v3/symbols`) must stay green.
- CALLERS block: managed host preparation for all ApeX reads, strict
  history expiry, and an unchanged one-time resend and three-retry budget.
- Architecture gate: no new engine API, no Background fallback for command
  reads, no venue-side retry policy beyond native counters.
