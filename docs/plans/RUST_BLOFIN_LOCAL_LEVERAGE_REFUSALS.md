# BloFin local leverage Read refusals

Owner: codex-9czdjh. Base actual `59f50e4f37b` as assigned by overseer.
Signer owner codex-zs5e83 explicitly released only these two account.rs hunks.
Fresh open-PR file map has no BloFin source/test collision. No active production
account.rs WIP is held by the signer; its coherent S3-S6 remains gated on G1/G2.

## Native ordering and direct engine path

account.rs::fetch_leverage currently emits two numeric failures before credentials:
unknown Store market (`Unknown market {symbol}`), then margin unavailable OR
mode not loaded (`BloFin margin mode is not loaded`). Preserve this precedence,
exact strings and their existing kind=None/outcome=None fields for all keyless,
public-only and keyed configurations. These are logical local Read refusals,
not evidence of HTTP, signing, a remote mutation or private-data publication.

In each refusal branch independently, admit the genuine command through existing
`cx.admit_http_command(Self::read_policy())` (Read, credential None, native
20,000 ms, no pacing, RetryNever), returning immediately on Rejected/Revoked.
On success call fail_context on that exact root with the original native error,
then return. No local wrapper, numeric fallback or second terminal. Admission
capacity refusal supersedes native error with its shared typed Read failure
(no Outcome). Synchronous terminal releases the root before return.

Do not admit on the valid fallthrough: its path, Request::Leverage fields,
private_read, credentials, signing, revision checks, callbacks, timers and HTTP
remain byte-exact. Trading's two queued Mutation exits stay with zs5e83 and must
not re-admit. No lib/history/closed/signer source changes.

## Exact remaining inventory and ownership

At this base: account 7 fingerprints/7 occurrences; closed 1/1; history 4/7;
trading 7/8. Venue total 19 fingerprints/23 occurrences. Preserve the complete
original source map in RUST_BLOFIN_REQUEST_CONTEXT.md and all remaining owners.

| Converted site | Fingerprint | Count |
| --- | --- | ---: |
| account.rs unknown market | `37bc238a40045dda234a3daaf68329976cef33caee8f13734a73f526bfef32cd` | 1 |
| account.rs margin/mode unavailable | `e2c8da59c7ed525a6d5342feecd9df9ef825904a02af1c97de61f39d1a7abf89` | 1 |

Remove only those two singleton rows. Account 7->5 and venue23->21 occurrences,
19->17 fingerprints. Other account read/completion sites plus all closed/history/
trading entries remain under the coherent signer/custody migration; no renamed
binding, synthetic signer, inventory exemption or unrelated prune.

## Phases and original controls

1. Commit/review this plan and send the pin to signer owner/overseer.
2. New isolated tests/local_leverage_context.rs: two compiling original capacity
   assertion REDs (one per refusal) and working exact-message/validation-order/
   keyless/public-only/keyed/Store-preservation controls. Saturate256 Background
   Read contexts in a preceding genuine no-id callback, not inside the tested
   command entry. Release exact roots and prove recovery. Keep duplicate original
   owner/diagnostic and >256 sequential local failures as lifetime controls.
   Existing native leverage suite pins signed valid fallthrough and responses.
3. Direct branch substitutions plus exact two-row prune. Original assertions now
   pass. CALLERS/AUDITS/QUALITY document the admission change. Focused BloFin plus
   Binance, optional-feature-off controls, strict Clippy/fmt and both ratchets.
4. Independent source/adversarial, caller and full-map reviews; draft PR and full
   CI. No merge without a window. Architecture: no per-venue admission, retry,
   classification or completion algorithm; consumer of shared command/Read APIs.
