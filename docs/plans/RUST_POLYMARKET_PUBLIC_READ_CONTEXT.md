# Polymarket public read custody

Plan first, 2026-10-24. The branch starts at actual landing
`508b24a080b135d0e9254bc711c8f3cbc589fea4` and integrates landed read-root
expiry #1976 (`6a6845c4491`) before implementation. Owner: codex-hcmq3f.

## Inventory and scope

The checker records 28 Polymarket fingerprints / 35 occurrences. The selected
public/local group has 16 fingerprints / 21 occurrences in history.rs and lib.rs.
The fixed-setting success occurrence is held on #1989 (done_local_setting):
credential-free Mutation success cannot seal an empty write journal. This slice
therefore targets 15 fingerprints / 20 occurrences, leaving 13 / 15. It does not
cast setters to Reads or manufacture credentials, attempts or accepted facts.

The other 14 occurrences remain open: two discovery/midpoint aggregate sends
need G1; five account sites need P6b import/L1-role/admission; seven trading sites
need provider admission, P8 and write-journal adoption. Private sign/account/
trading files, financial reducers and discovery ownership are unchanged.

The accepted seven-family count at 508b24 is 194 fingerprints / 223 occurrences:
Kraken 41/47 (20 workable), Coinbase 31/34 (15), Backpack 19/20 (7), ApeX 28/37
(6), Orderly 28/29 (14), Polymarket 28/35 (21 before the #1989 finding), and
BreakoutProp 19/21 (3). The existing blocked paths retain their inventory.

## Existing shared boundary

Use genuine initial admit_http_command for local read results/refusals and
keyless Mutation refusals. No-I/O paths immediately call done_context or
fail_context, never queue an HTTP attempt. The operation of an unsupported
mutation remains Mutation. Rejected admission already emitted its terminal;
Revoked grants no second terminal. No numeric fallback follows admission.
This uses the landed command-entry mechanism, without opting into Plan features.

Book and price-history requests each issue one unsigned managed Read Context.
Command reads use actual #1976 admit_caller_read_root at the immutable exact
caller receipt deadline, then one physical Read leg bounded by min(root, 20000 ms).
The root owns expiry and the leg owns the managed per-attempt timer. Expired
entry uses reject_expired_command(Read); command_wait None explicitly uses
reject_unstamped_command(Read), preserving its cause-aware typed refusal instead
of inventing a fresh/default cap. The same entry check protects local completion.
The leg bound starts before Prepare/Bind; a host that never prepares times out.
RetryPolicy is Never; no new retry, pacing, page, generation or local timer.
Background candle polls each get one Background Read Context with the same
20000 ms cap and preserve the existing five-second scheduling.

Pending owns the exact Context, with no raw completion ID. Before streamed
staging, native body decoding, command completion or candle publication, validate
the exact current delivery: validate_read_http_delivery(root, leg) for a
command and validate_background_read_http_delivery for a background poll.
Command completion uses done_context/fail_context; background retirement uses
finish_http_context. Typed preparation/failure hooks preserve engine error kinds
and existing local NotSent facts; released reads acquire no money outcome.
Queue failure consumes the returned Pending and its same Context, with no entry
restart. Shared engine expiry owns late/replayed/pre-claim responses.

Preserve exact URLs, empty headers/body, token binding, book sorting/limits/
validation, price-history fidelity/ranges, deduplication, truncation and candle
subscription publication rules. Preserve every local JSON/message and the fixed
setting no-op branch pending #1989. Options capability and symbol-amend refusals
use their own genuine initial read/mutation admission respectively.

## Original controls and implementation order

Commit this plan before tests, then commit original-source tests before source.
Use the real Engine and a fixture that handles both existing raw HTTP and actual
Prepare/Bind/Claim; compilation or fixture failures are not assertion REDs.
Controls cover a pending book versus a same-ID local command, short caller-wait
expiry for book/candles, a host that holds Prepare forever, late/duplicate delivery,
background candle expiry versus live publication, local no-I/O compatibility and
keyless mutation refusal. Preserve existing book token/levels/error assertions,
price-history boundaries/dedup/limits, and private/discovery working controls.

After conversion, prune only the selected fingerprints, retain the setting
success and all unowned entries, and run both inventory checks. Document the new
20000 ms attempt bound and managed host requirement in CALLERS. Run scoped
Polymarket tests, strict Clippy, fmt and bans; CI supplies replay/WASM/size gates.
Obtain independent source, caller and full-map review before readiness. Publish
one draft and request a train window only after its actual integration is green.
No merge is authorized by this plan.

## Target fingerprints

- `venues/polymarket/src/history.rs:70`: `945f9a041721fe973e19a8c7736f8128940587e73a19d29ba2a84a9a8065c02e` — cx.fail(command, "Invalid Polymarket order-book limit")
- `venues/polymarket/src/history.rs:74`: `bd42ab82df9fed6e5ae5dba11b55151c1a9c284d315f49f96a038d5097197544` — cx.fail(command, "Unknown Polymarket symbol")
- `venues/polymarket/src/history.rs:78`: `d3d58e81772917b4f53ec556d28f5ea1cee4f22f903db842b60888fe48bff001` — cx.request( Method::Get, format!("{}/book?token_id={}", crate::REST, crate::encode(&token)), vec![], None, Pending::Book { command, token, limit, }, )
- `venues/polymarket/src/history.rs:104`: `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` — cx.done(command, data)
- `venues/polymarket/src/history.rs:105`: `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` — cx.fail(command, error)
- `venues/polymarket/src/history.rs:110`: `61edeb21b19d7d3397781984067b45fd9091f1e927cd1942f2eea9b42f4dd0ad` — cx.fail( command, "Polymarket self-trade prevention settings are unsupported", )
- `venues/polymarket/src/history.rs:116`: `f834c044840718f11259ba57d11fb7e57a30239e1585cf37c0571b64d6f3d484` — cx.done(command, r#"{"buyLeverage":1,"sellLeverage":1}"#.into())
- `venues/polymarket/src/history.rs:118`: `d8877910dd4935bc942e96557431f8cb07f3f22f072bc567edd25ba564f4dcf9` — cx.done(command, "1".into())
- `venues/polymarket/src/history.rs:121`: `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` — cx.done(command, "[]".into())
- `venues/polymarket/src/history.rs:139`: `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` — cx.done(command, data)
- `venues/polymarket/src/history.rs:140`: `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` — cx.fail(command, error)
- `venues/polymarket/src/history.rs:179`: `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` — cx.done(command, data)
- `venues/polymarket/src/history.rs:180`: `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` — cx.fail(command, error)
- `venues/polymarket/src/lib.rs:640`: `7257e88bed7f0d58ba127a75cfb161582d721862e09978f16249884f06a9f49e` — cx.request_streamed( Method::Get, path, vec![], ArrayAt::Field("history"), Pending::Candles { command, symbol, interval, limit: limit.unwrap_or(u32::MAX) as usize, rows: vec![], from_ms, to_ms, invalid: false, }, )
- `venues/polymarket/src/lib.rs:659`: `0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb` — cx.fail(command, e)
- `venues/polymarket/src/lib.rs:956`: `982f8f0f4b02233b21556691e476824408f6ea831a0ebd301bff2d1c9fdd76da` — cx.fail(c, "Invalid Polymarket price history")
- `venues/polymarket/src/lib.rs:981`: `6cb9a1e96c67a077e2d60646f5ef1786c7274adad1b681cbcddadb9317b35947` — cx.done(c, data)
- `venues/polymarket/src/lib.rs:982`: `f06dce80ebd266653d22bf24f105c411c21c1de8a011cd3d6fe688214267e373` — cx.fail(c, error)
- `venues/polymarket/src/lib.rs:1135`: `f06dce80ebd266653d22bf24f105c411c21c1de8a011cd3d6fe688214267e373` — cx.fail(c, error)
- `venues/polymarket/src/lib.rs:1235`: `cab66fc0399e6d1f53504198a9a5df6893df31bad853bf84c1dcd416c2326cdd` — cx.fail(command, "Options are not enabled on this account")
- `venues/polymarket/src/lib.rs:1315`: `40062be4474042e41b637a57fd5647a9f5db174340233694f3e8de781b16c177` — cx.fail(command, "Symbol-scoped amendment is unsupported")

## Original checkpoint

Plan b4bf10011c2 precedes original controls e9d094f9298. Against unchanged
Polymarket production, the seven-control target executes six assertion REDs and
one compatibility GREEN, zero ignored. The background test first proves live
publication before asserting the expired case. The failures are same-ID cached
completion, late book and candle successes, stalled-prepare noncompletion,
expired background publication and missing NotSent on a keyless mutation refusal.
Fixture compile corrections are not counted as REDs.

The first conversion turns all seven original controls GREEN without changing
any assertion. Existing Polymarket native targets pass 139 tests, including the
original controls and one doc test. Additional bounded-deadline/expired-entry
controls and final strict checks are recorded separately. Existing fixture
changes only drive real Prepare/Bind/Claim and omit engine timer bookkeeping from
exact HTTP/terminal assertions; native wire/financial assertions stay intact.

## Focused implementation evidence

The original seven controls remain unchanged in their assertions. Three further
controls pass: released 20-second attempt expiry keeps typed Timeout/no outcome;
stalled Prepare expires at the shorter attempt timer with typed Timeout/NotSent
and no HTTP; an unstamped local read is a typed InvalidInput/no-outcome refusal.
A proposed zero-wait fixture was removed because handle_command rejects that
invalid host input before reaching the venue; it is not counted as RED evidence.

Scoped native verification before the last entry guard passed 139 tests across
nine targets (including one doc test); the final ten-control target, strict
all-target Clippy, package fmt, both request-context checks (15 checker controls)
and bans pass. The earlier root-level cargo-fmt invocation selected no targets;
the corrected explicit package invocation passes. The manifest delta is exactly
the two Polymarket source maps: history 9/13 -> 1/1, lib 9/10 -> 2/2; all other
maps retain their incoming order and values. Final full-head CI and independent
reviews remain required before readiness; no size/replay or merge claim yet.


## Independent bounded reviews

At source f6a93e74aa9, the caller and full-map reviews are CLEAR. The own delta
against #1976 is ten paths, with all 5588 unowned entries preserved. The original
control file is an exact prefix (three controls appended); other family manifest
maps preserve values and key order. Review wording was corrected to name the
actual command delivery pair (root, leg). Both reviews were read-only, with no
independent build/CI claim. Actual ba3d7da1d3a integrates cleanly afterward and
adds only incoming order-fence documentation; own runtime remains exact f6a93.


Final integrated focused run on ba3d7da1d3a plus this slice passes 142 tests
across nine targets (including the doc test), zero failures or ignored tests.
The ten-control target, strict all-target Clippy and package fmt pass after the
last receipt guard. Full CI/replay/WASM/size remain pending for the draft.
