# KuCoin synchronous pre-I/O command refusals

Owner: codex-9czdjh. Landing baseline: `6c7f4fcf292d01b8c056127a070783e3fd72dcd2`.
Branch: `feat/rust-kucoin-local-command-refusals`. #1983 remains frozen on its
separate branch at `37bb5c1e81c`; no root/child helper source is changed here.

## Lane inventory and selected slice

The baseline checker manifest exactly matches its lexical source scan:

| Venue | Fingerprints | Occurrences | Workable occurrences | Blocked/held occurrences |
| --- | ---: | ---: | ---: | ---: |
| KuCoin futures and Spot | 51 | 56 | 8 | 48 |
| Bitrue | 20 | 33 | 3 | 30 |
| Bitunix REST | 9 | 10 | 0 | 10 |
| BloFin | 19 | 23 | 0 | 23 |
| Total | 99 | 122 | 11 | 111 |

Counts include conservative macro/field/test expressions, not just HTTP calls.
No binding rename or test rewrite alone may claim a removal. Bitunix's raw WS
login is outside these ten REST entries and stays byte-exact under Sam's ruling.

KuCoin and Bitrue signed/private callers wait for claude-4yjlw0's #1718 family
queue; there is no frozen family dispatch-signer source pin. Session bullet and
listen-key callers additionally need their genuine session adoption, and Bitrue
FetchLeverage joins need the background execution consumer. BloFin's paired
`feat/rust-blofin-signer-migration` S3-S6 cutover owns its 23 occurrences,
including four local refusals; it waits for G1/G2. Bitunix private REST remains
held on G1/G2 and its reviewed signer/history prerequisites.

Three successful no-I/O mutation sites are separately blocked: KuCoin Spot's
fixed SetLeverage projection and shared SetMarginType/SetPositionMode no-op, and
Bitrue's SetPositionMode(false). They succeed BEFORE credential validation and
must retain keyless/public-only support. Authenticated `done_empty_plan` requires
a genuine write credential; ordinary Mutation Context success rejects an empty
attempt journal. No fabricated credential/HTTP/fact or Read cast is allowed.
Overseer assigned the shared local Mutation-success seam to codex-uufub4.

This PR takes KuCoin's eight synchronous refusal occurrences only:

| File / native branch | Fingerprint prefix | Occurrences | Shared path |
| --- | --- | ---: | --- |
| lib.rs invalid candle range | 20065b0c046a | 1 | initial Read admission, fail_context |
| lib.rs unsupported candle interval | 0451c2999329 | 1 | initial Read admission, fail_context |
| trading.rs validation failure | 2f0770d2ea5f | 1 | initial Mutation admission, fail_context |
| trading.rs missing credentials | 1beb1b175218 | 1 | initial Mutation admission, fail_context |
| spot/trading.rs missing amend target | f1f763932a88 | 1 | initial Mutation admission, fail_context |
| spot/trading.rs unsupported command | 110dc698dbd0 | 1 | truthful Read/Mutation admission, fail_context |
| spot/trading.rs missing credentials | c9f18250fdcb | 1 | initial Mutation admission, fail_context |
| spot/trading.rs build failure | 3c98818bcbb2 | 1 | initial Mutation admission, fail_context |

Exact proposed prune: KuCoin 56 -> 48 occurrences and 51 -> 43 fingerprints.
lib.rs 9 -> 7 occurrences; trading.rs 5 -> 3; spot/trading.rs 10 -> 6.
All other venue files and all other manifest keys remain exact incoming.

## Ownership and semantics

Open-PR file lists were checked on the pinned landing; no other PR claims these
three KuCoin source files. claude-4yjlw0 confirmed no unpublished claim on these
local regions. Keep sign.rs, private/public request helpers, credential setup,
WS/bullet paths and valid remote fallthrough unchanged. No other venue source
or engine seam is edited. KCS steady public event work does not gain admission
checks; these are cold synchronous error branches only.

After a native branch has selected its existing local refusal, admit exactly
one genuine command Context before publishing it. Declare Read for candles and
FetchStpMode/FetchOrderBook; declare Mutation for trading and SetStpMode. The
credential is None: these branches perform no remote I/O. Use an explicit
20,000ms/Never/no-pacing policy, matching prior local adoptions. Existing shared
command entry owns invalid-policy, capacity and duplicate/borrowed refusal.
Return on Rejected/Revoked; never emit another numeric fallback. On success,
fail through that exact Context with the original typed native error.

Preserve native validation ordering and message/kind/outcome. In particular:
- futures validation precedes credentials;
- Spot missing-amend-target precedes its generic build/credential gate;
- Spot credentials precede reporting a completed build error;
- unsupported modes keep Unsupported/NotSent;
- invalid candle inputs retain their original unclassified Read errors;
- no new owner exists on a valid signed/public remote fallthrough.

None-credential Contexts do not grant signer or route authority. No local
classifier, terminal registry, retry timer or completion algorithm is added.
Successful local mutations stay held for the shared seam above.

## Tests before production

Create an isolated integration test file using the real KuCoin/KucoinSpot under
a forwarding Probe and genuine Engine callbacks. Fill 256 Background Read roots
in a preceding no-ID callback to create a real capacity denial without forging
a Context. Each of the eight native refusal sites must produce the engine's
single exact Capacity failure instead of bypassing admission. Label and commit
those compiling assertion REDs separately before production. No ignored tests,
renamed bindings, deliberately reselecting fixtures or missing-API RED claims.

Controls pin native errors and ordering with free capacity, no HTTP/signing-provider/
timer/Store changes, recovery after releasing fixture capacity, unrelated-owner
completion, command-ID reuse and valid private/public request wire fallthrough.
Keep truthful Read-vs-Mutation denial outcomes (Read None, Mutation NotSent).
Duplicate callback handling uses genuine original ownership, no direct fake Cx.

Then convert only the eight branches, prune the exact eight fingerprints, and
verify the checker plus its tests. Run focused KuCoin tests with and without
Spot, strict scoped Clippy/fmt, the Binance context fast lane, three independent
source/adversarial/caller/full-map reviews and fresh CI. Update CALLERS/AUDITS/
QUALITY with actual behavior. Open a draft into feat/rust-rewrite; merge only
with green proof, actual integration and a fresh separate guard in an assigned
window. Bitrue's three refusals follow in their own small draft.

Architecture gate: no per-venue cross-cutting fix. These call existing shared
command admission and typed completion; no new shared mechanism is implemented
inside a venue. The separate no-I/O success gap stays with the core owner.

Plan review refinement: Spot placement construction can consume UUID identity
entropy before credentials or a later build failure. Preserve those draws and
credential/error precedence; only HTTP and signing-provider work must be absent.
Pin supplied/generated clientOid and later batch failure separately. The native
guard policies must be forwarded by the genuine Engine Probe. Three independent
plan/source/caller/full-map reviews are CLEAR at8fb8e5b0453.
