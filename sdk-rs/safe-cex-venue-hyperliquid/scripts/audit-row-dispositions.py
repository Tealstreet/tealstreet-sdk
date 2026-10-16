#!/usr/bin/env python3
import argparse
import collections
import json
from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parents[3]
MANIFEST = ROOT / 'sdk-rs/parity/manifest/exchanges/hyperliquid'
CRATE = Path(__file__).resolve().parents[1]
EVIDENCE = {
    'hyperliquid.builderFee.ts.toml': {
        'field_identity': 'approval_leaves.rs: default fee bits/rate text, both domain members and ordered EIP-712 field names/types',
        'roundtrip': 'native static fields are asserted directly; signing/submission remains pending',
        'nan': 'approval constants have fixed finite chain IDs and the default binary64 fee',
        'processes': 'approval-leaves.cjs loads actual core/approval modules once for the whole corpus',
    },
    'hyperliquid.agent.ts.toml': {
        'field_identity': 'approval_leaves.rs: four fresh fallible random operations, native name/error identity/read order, host-bound wallet predicate effects',
        'roundtrip': 'native string/boolean/error pointer and host reads precede any serialization; browser connection/signing remains pending',
        'nan': 'random NaN follows JS charAt index-zero conversion; signed zero/infinity/out-of-range cases are independently exercised',
        'processes': 'approval-leaves.cjs runs 50 RNG/name cases and 18 wallet cases in one invocation',
    },
    'hyperliquidspot.types.ts.toml': {
        'field_identity': 'spot_types.rs earlier tables/order roots; approval_leaves.rs cancellation roots; spot_models.rs 50 new raw-JSON model roots, 400 strict source cases plus five independent witnesses; native UTF-16 units/binary64 bits/optional presence and tags, including nested duplicate keys and frontend flags',
        'roundtrip': 'native variants/members/optional presence and price bits are read directly; no encode assertions',
        'nan': 'JSON typed-input scope; signed-zero and overflow infinity bits asserted before wire; no NaN token',
        'processes': 'spot-models-oracle.cjs independently resolves actual declarations, JSON.parse native projections and all 405 strict checker cases in one invocation; stops at root case 399 plus witness 404',
    },
    'hyperliquid.constants.ts.toml': {
        'field_identity': 'builder_constants.rs: native constant roots and builder members/UTF-16 units/fee bits; getter read order/errors and configured pointer identity',
        'roundtrip': 'native values are read directly; provided configuration remains Cow::Borrowed; no production record serialization',
        'nan': 'source descriptors canonicalize NaN only; signed-zero and finite fee bits remain exact before JS rate text',
        'processes': 'builder-constants.cjs executes 105 data function cases plus five getter cases and constant inventory in one process per binary',
    },
    'hyperliquidspot.constants.ts.toml': {
        'field_identity': 'builder_constants.rs: three spot constant roots with independent source arrays including spotClearinghouseState position',
        'roundtrip': 'native static arrays/string are directly compared to the actual module exports',
        'nan': 'spot subscription/API constants contain no floating-point capture',
        'processes': 'spot constants share the single whole-corpus builder source invocation',
    },
    'hyperliquid.order-inference.ts.toml': {
        'field_identity': 'order_inference.rs: native type, price bits, source variant, TIF and coercion error members against actual module; distinct 91.25/17.5 price pair',
        'roundtrip': 'native result and enum variants are read directly; no derived record serialization',
        'nan': 'finite-or-zero helper; native signed-zero bits remain exact, nonfinite parse results become zero as in source',
        'processes': 'order-inference.cjs executes the complete 62-case source corpus in one process',
    },
    'hyperliquid.types.ts.toml': {
        'field_identity': 'field_identities.rs: decoded native projections and enum alternatives; Any/intersection exclusions remain findings',
        'roundtrip': 'identity supplements read native projections; the separate legacy types.rs roundtrip alone is not identity',
        'nan': 'typed_numbers.rs and typed_ingress_audit.rs: native signed-zero/overflow bits; JSON Number grammar supplies no NaN token',
        'processes': 'types, numeric, string and accepted-set source programs run whole corpora; no Node invocation per numeric field',
    },
    'hyperliquid.order-response.ts.toml': {
        'field_identity': 'api.rs: waiting/deferred, native ID vector and coercion errors; constructor attribution remains finding',
        'roundtrip': 'ParsedHyperliquidOrderResponse members are inspected directly',
        'nan': 'parser/signer coercion outputs are compared as language strings or native tagged values, not CPU NaN signs',
        'processes': 'api and earlier-response probes execute one Node process per corpus',
    },
    'hyperliquid.order-history.ts.toml': {
        'field_identity': 'order_history.rs: native historical fields and source pre-serialization inherited hits',
        'roundtrip': 'native identity supplement is separate from legacy serialized comparisons; legacy cardinality remains open',
        'nan': 'amount/filled language strings preserve Infinity before serialization; legacy comparison scope is not universal native identity',
        'processes': 'history main, boundaries, helpers and identity each execute a whole corpus',
    },
    'hyperliquid.api.ts.toml': {
        'field_identity': 'api.rs: signed action/request values, native signatures, ID and error results; residual target cardinality remains open',
        'roundtrip': 'native signing views are asserted separately from outbound JSON; external API construction remains chosen',
        'nan': 'nonfinite preimages/native number semantics are asserted before wire projection; hashes are not floating-point bit captures',
        'processes': 'whole API/source corpora; no case-level child invocation',
    },
    'hyperliquid.signing.ts.toml': {
        'field_identity': 'api.rs and crypto field-identity consumers; general signature/EIP712/vault findings remain named',
        'roundtrip': 'native signature r/s, action views and digests are not a serde record roundtrip',
        'nan': 'number normalization/preimages use JS scalar rules; encoded bytes do not freeze a CPU NaN sign',
        'processes': 'source signing/action corpora execute whole arrays in one Node process',
    },
    'hyperliquid.rateLimiter.ts.toml': {
        'field_identity': 'api.rs: weight corpus plus limiter ticket/waker/cancellation effects; constructor defaults remain finding',
        'roundtrip': 'stateful clock/ticket effects have no serde decode/encode symmetry',
        'nan': 'numeric language/coercion witnesses do not assert platform NaN payloads',
        'processes': 'clock and source corpora are whole-process invocations',
    },
    'hyperliquid.exchange.ts.toml': {
        'field_identity': 'exchange.rs unit tests: nonce, privacy, TIF/cancel state and placement native/read effects; numeric_helpers.rs inspects pure-helper scalar values/errors',
        'roundtrip': 'state caches and read counters are native assertions; class/constructor composition remains incomplete',
        'nan': 'nonce capture canonicalizes NaN only; signed-zero and finite bits stay exact',
        'processes': 'nonce/state/placement and numeric-helper source programs return whole corpora',
    },
    'hyperliquid.ws-public.ts.toml': {
        'field_identity': 'ws_ping_lifecycle.rs ten public/private native timer/opening roots, callback captures/guards, source-order effects and primitive state,123fixedcases through122; public_subscriptions.rs: Number/Null, NaN/Infinity/zero and callback state precede colliding wire keys',
        'roundtrip': 'two input evaluations and sender/error/reentry effects are inspected directly',
        'nan': 'native NaN is checked as NaN; signed-zero/finite inputs remain distinct before stringify',
        'processes': 'subscription/public-topic/user-topic programs each return one complete corpus',
    },
    'hyperliquidRetry.ts.toml': {
        'field_identity': 'api.rs: exact URL/retry predicate witnesses; scalar boolean output',
        'roundtrip': 'no record deserialization or serialization on the predicate',
        'nan': 'predicate output contains no floating-point capture',
        'processes': 'one complete URL corpus per Node invocation',
    },
}
EVIDENCE.update({
    'hyperliquid.cross-cache.ts.toml': {
        'field_identity': 'json_boundaries.rs tests: two declared cache-entry models, native enum/price/amount/timestamp bits; cache lifecycle mapped by separate cache lane, see CACHE_RUNTIME_GATE_EXITS.md; that lane proof not run here',
        'roundtrip': 'direct decoded fields, no model serialization',
        'nan': 'raw JSON signed-zero/overflow/precision binary64; no NaN token',
        'processes': 'independent json-models-oracle.cjs strict71case whole corpus',
    },
    'hyperliquid.fast-asset-ctxs.ts.toml': {
        'field_identity': 'json_boundaries.rs tests: declared optional UTF-16 mark/mid values and ordered UTF-16-key dictionary; inflater/decoder function remains pending',
        'roundtrip': 'native units/optional presence and last duplicate before typed decoding, no serialization',
        'nan': 'model price fields are strings; no CPU NaN capture',
        'processes': 'independent json-models-oracle.cjs strict71case whole corpus',
    },
    'hyperliquid.ws-diagnostics.ts.toml': {
        'field_identity': 'json_boundaries.rs tests: five actual-source leaves, native UTF-16 text/booleans/object handles/receiver arguments and opaque errors/read order including revoked classifiers',
        'roundtrip': 'decoded native values/errors/effects precede any wire assertion; host JSON/String primitives are supplied independently',
        'nan': 'host number descriptors canonicalize NaN only and keep signed-zero/finite bits',
        'processes': 'json-boundaries-oracle.cjs actual seven leaves/215cases in one process, through214',
    },
})
EVIDENCE['hyperliquidspot.types.ts.toml']['field_identity'] += '; three new json_models.rs dictionary/array-alternative roots with UTF-16 key order and native values against71strict cases'
EVIDENCE['hyperliquid.agent.ts.toml']['field_identity'] += '; json_models.rs AgentWalletCreationResult native optional string units and boolean'
EVIDENCE['hyperliquid.exchange.ts.toml']['field_identity'] += '; json_boundaries.rs UTF-16 mask and host-bound arbitrary JSON payload leaf native value/error/identity/receiver effects'

for _file in ['hyperliquid.errors.ts.toml','hyperliquidspot.errors.ts.toml','hyperliquid.utils.ts.toml','hyperliquid.market-startup-helpers.ts.toml']:
    EVIDENCE[_file] = {
        'field_identity': 'error_market_helpers.rs: actual source Error native code/name/message/descriptor/prototype/identity; scalar UTF-16/f64 values and tuple member identity/receiver/iterator/logger effects as applicable',
        'roundtrip': 'native fields/arguments/handles/errors are read directly before wire; no encode/roundtrip gate',
        'nan': 'NaN outputs tested as NaN, not CPU payload/sign; signed-zero and finite/overflow bits kept distinct',
        'processes': 'error-market-oracle.cjs actual twelve symbols/275cases in one invocation, through274; no live network',
    }
EVIDENCE['hyperliquid.fast-asset-ctxs.ts.toml']['field_identity'] += '; fast decoder now source-bound in market_helpers.rs, native object identity and validation pipeline against actual pako/base64/TextDecoder results'
EVIDENCE['hyperliquid.fast-asset-ctxs.ts.toml']['field_identity'] = EVIDENCE['hyperliquid.fast-asset-ctxs.ts.toml']['field_identity'].replace('inflater/decoder function remains pending','intrinsic inflater/decoder operations are host supplied')
EVIDENCE['hyperliquid.exchange.ts.toml']['field_identity'] += '; market_helpers.rs volume/quote and open-interest native binary64 values/errors with actual get-key and coercion effects'

EVIDENCE['hyperliquid.cross-cache.ts.toml']['field_identity'] += '; cross_cache.rs ten method leaves; native key/record/UTF16 fields and handles, storage/channel receiver arguments, map binding-error close, shallow arrays, pending entry alias, seen FIFO and disposal state against80actual-source cases through79; class/constructor/higher cache lifecycle mapped by separate cache lane, see CACHE_RUNTIME_GATE_EXITS.md; that lane proof not run here'

EVIDENCE['hyperliquid.cross-cache.ts.toml']['field_identity'] += '; cache_orders.rs five storage/pre-clear/pending lifecycle leaves; source-captured stale entry/native record mutation and shared entry handles, strict delete/set and clock/iterator/Map/broadcast effects against95cases through94'
EVIDENCE['hyperliquid.exchange.ts.toml']['field_identity'] += '; cache_orders.rs five cache delegate callers: fresh factory/receiver/method/arguments, optional undefined and native result handle/error effects; singleton/class remains pending'

EVIDENCE['hyperliquid.ws-private.ts.toml'] = {'field_identity':'ws_ping_lifecycle.rs ten private/public ping/opening algorithms with native timer/field/Set effects and callbacks; private_position_helpers.rs eleven private helper algorithms; decoded summary f64 pairs/enum sourceField, native string/number/error/receiver arguments and mutable PnL/queue/Set effects', 'roundtrip':'private_position_helpers.rs native decoded fields/values and model state directly before wire; no roundtrip gate', 'nan':'Number finite masks and positive timestamp; NaN semantic tag only, signed-zero and finite/overflow bits distinct', 'processes':'ws-ping-oracle.cjs ten methods/123cases through122 in one invocation; private-position-oracle.cjs actualeleven methods/133cases through132 in one invocation; full WSclass and live operations remain pending'}

EVIDENCE['hyperliquid.ws-public.ts.toml']['field_identity'] += '; ws_transport_leaves.rs four venue-selected connectors, native event/handler/receiver state and source-order strict ws assignment'
EVIDENCE['hyperliquid.ws-private.ts.toml']['field_identity'] += '; ws_transport_leaves.rs connector and private dispatcher source-order native arguments/effects'
for file in ['hyperliquidspot.ws-public.ts.toml','hyperliquidspot.ws-private.ts.toml']:
    EVIDENCE[file] = {'field_identity':'ws_transport_leaves.rs spot connector/opening/ping/private dispatcher algorithms; native event/handler/receiver identities, strict ws/pingAt fields, immediate refresh boolean, UTF16 payload member and source-order effects', 'roundtrip':'native fields, listener record tuples and actual primitive/callee arguments asserted directly; no decode/reencode gate', 'nan':'clock f64 bits retain finite/signed-zero/infinity; NaN semantic tag; testnet/disposed JS truthiness independent of wire encoding', 'processes':'ws-transport-oracle.cjs executes202fixed actual-source cases through201 across ten roots in one invocation; full class/live operation unproved'}

for file in ['hyperliquid.ws-public.ts.toml','hyperliquid.ws-private.ts.toml','hyperliquidspot.ws-public.ts.toml','hyperliquidspot.ws-private.ts.toml']:
    EVIDENCE[file]['field_identity'] += '; ws_emission.rs ten native subscription/diagnostic emission roots: ordered literal field trees, opaque user/coin/formatter identity, conditional spread fields and source-order iterator/callee effects'
    EVIDENCE[file]['processes'] += '; ws-emission-oracle.cjs118fixedcases through117 in one invocation; iterator primitive witnesses separately scoped'

EVIDENCE['hyperliquidspot.exchange.ts.toml'] = {
    'field_identity': 'account_position_leaves.rs source spot config and uncached generator leaves; native provided static config alias/three members and generator receiver/market identity',
    'roundtrip': 'decoded native handles, ordered config fields and binary64 leverage bits asserted directly; no roundtrip gate',
    'nan': 'provided static leverage is independently fixed to finite1; host values retain signed-zero/f64 bits and semantic NaN tags',
    'processes': 'account-position-oracle.cjs actual spot prototype methods execute in the shared129case invocation; full exchange/generator remain pending',
}

for file in ['hyperliquid.exchange.ts.toml','hyperliquidspot.exchange.ts.toml']:
    EVIDENCE[file]['field_identity'] += '; account_position_leaves.rs native account guards/config field trees/default20-vs1bits/cache and requested handles, short-circuit/callee/property order and owned opposite-size predicate/aggregate'
    EVIDENCE[file]['processes'] += '; account-position-oracle.cjs129fixedcases through128 in one invocation; generators/fullclasses/collection primitives remain outsideproof'

for file in ['hyperliquid.exchange.ts.toml','hyperliquidspot.exchange.ts.toml','hyperliquid.ws-public.ts.toml','hyperliquid.ws-private.ts.toml']:
    EVIDENCE[file]['field_identity'] += '; lifecycle_delegates.rs native listener args/cleanup identity, disposal strict fields/collections/base and websocket effects and owned debounce/rejection callbacks with strict threshold/template coercion'
    EVIDENCE[file]['processes'] += '; lifecycle-delegates-oracle.cjs111fixedcases through110 in one invocation; fullclasses/timers/collections/virtualcallees outsideproof'

for file in ['hyperliquid.exchange.ts.toml','hyperliquidspot.exchange.ts.toml','hyperliquid.ws-private.ts.toml']:
    EVIDENCE[file]['field_identity'] += '; market_data_selectors.rs native lookup/symbol/summary/precision/routing algorithms, map overwrite/record aliases/Set dedup-order/literal member bits/UTF16 and source effects'
    EVIDENCE[file]['processes'] += '; market-data-selectors-oracle.cjs161fixedcases through160; actual CCXT numeric/order-response/LogSeverity/constants source bindings; primitive/library/fullclass implementations remain outsideproof'

for file in ['hyperliquid.exchange.ts.toml','hyperliquidspot.exchange.ts.toml']:
    EVIDENCE[file]['field_identity'] += '; balance_state.rs ten owned pricing/balance/snapshot/refresh leaves and owned market predicate, native binary64/UTF16/prototype/ordinary-set-vs-literal-definition/state fields and source-order effects'
    EVIDENCE[file]['processes'] += '; balance-state-oracle.cjs152fixedsourcecases through151 in one invocation; async refresh/fullclasses and host/intrinsic implementations outsideproof'

for file in ['hyperliquid.exchange.ts.toml','hyperliquidspot.exchange.ts.toml']:
    EVIDENCE[file]['field_identity'] += '; order_request_leaves.rs ten request/telemetry/margin/signing-delegate bodies, native decoded field trees/error identity/call arity and source-order state; body first-poll/host-await scope'
    EVIDENCE[file]['processes'] += '; order-request-oracle.cjs143sourcecases through142 and94await boundaries in one invocation; async entry/scheduling/Promise/imported-callee/fullclass implementations outsideproof'

for file in ['hyperliquid.agent.ts.toml','hyperliquid.builderFee.ts.toml']:
    EVIDENCE[file]['field_identity'] += '; browser_approval_flows.rs eleven owned network/sign/submit/browser/check/factory bodies: native fields/r-s-v/aliases/defaults/catch evaluation order, module-local chain identity and pending host-await cutpoints before wire'
    EVIDENCE[file]['processes'] += '; browser-approval-oracle.cjs134sourcecases through133/227manually expectedhostawaits in one invocation; provider/fetch/ethers/JSON/sibling callee contracts, full microtask/live/signing behavior outsideproof'

for source_file in ['hyperliquid.ws-private.ts.toml', 'hyperliquid.ws-public.ts.toml']:
    EVIDENCE[source_file]['field_identity'] += '; private_message_dispatch.rs: ten core routing/ticker roots, owned filter predicate, source-order payloads and reused buffers; native decoded bool/UTF16/binary64/handle/prototype/map/set/buffer fields in 140case floor through139'
    EVIDENCE[source_file]['roundtrip'] += '; new dispatch tests complete all decoded native assertions before any supplemental wire assertion'
    EVIDENCE[source_file]['processes'] += '; private-message-oracle.cjs loads eight private/two public actual-source bodies in one process; imported decoder/JS primitives/virtual callees are fixture contracts'

EVIDENCE['hyperliquid.exchange.ts.toml']['field_identity'] += '; core_account_snapshots.rs ten credential/account/private-query bodies, native100case floor through99/50independentawaitpoints, owned twofilterpredicates/logsummary, source-order state/getter/receiver and field/array/prototype/handle outputs'
EVIDENCE['hyperliquid.exchange.ts.toml']['roundtrip'] += '; new account native assertions all finish before supplementalwire; pending-once awaitcutpoints and explicitfoldreturnadoption'
EVIDENCE['hyperliquid.exchange.ts.toml']['processes'] += '; core-snapshot-oracle.cjs loads ten actualsource bodies and constants inone process; primitive/runtime/imported/virtualcallee/fullclass scopes explicit'

# Incoming second-lane witnesses are citations only; this lane did not run rest_helpers.
EVIDENCE['hyperliquid.income.ts.toml'] = {
    'field_identity': 'income.rs: ordered native fields, provenance, original row/callback aliases and decoded UTF16/binary64/outcomes; second-lane REST_HELPERS_GATE_EXITS.md at ed98455f4cb3330f1d0f9024109b6585b909fe16 (not run by this lane)',
    'roundtrip': 'second-lane native values and effects precede serialization; no re-encoded result comparisons claimed; finite witness/callee contracts remain explicit',
    'nan': 'source parseFloat finite fallback and decoded NaN class; derived signs/provenance; no full numeric/coercion enumeration claim',
    'processes': 'rest_helpers_oracle.cjs complete unchanged income/startup modules,64fixed cases through63 across eight roots; citation only, no per-row or current-SHA rerun claim by this lane',
}
for category in ['field_identity','roundtrip','nan','processes']:
    EVIDENCE['hyperliquid.market-startup-helpers.ts.toml'][category] += '; two incoming fetch roots: native body/config/parser/log/retry/catch/member identity and eager owned tasks cited in REST_HELPERS_GATE_EXITS.md ed98455f4cb3330f1d0f9024109b6585b909fe16;64case floor through63 across income/startup; separate second-lane gates not run by this lane'

EVIDENCE['hyperliquid.exchange.ts.toml']['field_identity'] += '; core_rest_reads.rs ten new REST bodies plus27owned callbacks, freshfield/default/catch/iterator order, native derived fields/IDs/aliases and ordered Map overwrite/values; eagerroot/callback task-prefix/settlement and three droppedroot witnesses'
EVIDENCE['hyperliquid.exchange.ts.toml']['roundtrip'] += '; every new REST native field/value/callback/task assertion precedes wire; no result encode-decode'
EVIDENCE['hyperliquid.exchange.ts.toml']['nan'] += '; native binary64 bits/NaN class, finite filtering and unchecked fill-sum NaN/negative/zero retained; general numeric/coercion enumeration OPEN'
EVIDENCE['hyperliquid.exchange.ts.toml']['processes'] += '; core-rest-oracle.cjs94sourcecases through93/101awaits inone process;97native replays including3rootdrop witnesses through2/108awaits; library/importedhelper/taskprimitive backend and universal event-loop/live/proxy behavior outsidefiniteproof'

# Incoming spot witnesses are citations only, not native gates run by this lane.
for file in ['hyperliquidspot.ws-public.ts.toml', 'hyperliquidspot.ws-private.ts.toml']:
    EVIDENCE[file]['field_identity'] += '; spot_ws_dispatch.rs incoming dispatch/constructor roots; second-lane SPOT_WS_GATE_EXITS.md merge/gate262cf08660a421484fd7057553c609933dda7c57, scoped native9/9 cited only'
    EVIDENCE[file]['processes'] += '; second-lane spot_ws_oracle.cjs witnesses cited only; not rerun here, no per-row control claim by this lane'
EVIDENCE['hyperliquidspot.exchange.ts.toml']['field_identity'] += '; spot_account_reads.rs incoming nine reads: second-lane SPOT_ACCOUNT_READS_GATE_EXITS.md final preservation116a2b47706a445304e359627a17d571a0676c8f, native24/26 source-host gates cited only'
EVIDENCE['hyperliquidspot.exchange.ts.toml']['processes'] += '; second-lane spot_account_oracle.cjs cited only; not rerun here, finite fixture/helper contract limits preserved'

ORIGINAL_FINDINGS = {
    'AllMids', 'UserRateLimit', 'OrderStatus', 'WsUserEvent',
    'RateLimiter.constructor', 'HyperliquidUnreadableOrderResponseError.constructor',
    'addResponseInterceptor', 'splitSig', 'addSigningInterceptor',
    'addressToBytes', 'actionHash', 'signL1Action', 'signInner',
}


for file in ['hyperliquid.exchange.ts.toml','hyperliquid.ws-private.ts.toml']:
    EVIDENCE[file]['field_identity'] += '; core_position_order_state.rs ten newly owned ingestion bodies and13owned callbacks, distinct Map/uniqBy merge precedence and live/REST clocks; freshreads/coercions/native payload fields/state/callback outcomes before wire'
    EVIDENCE[file]['roundtrip'] += '; position-order native bodies/owned callbacks asserted before any wire; library/virtualcallee contracts remain explicit'
    EVIDENCE[file]['nan'] += '; binary64 operands/results, raw REST leverage and websocket unary coercion remain distinct; current106case floor is not complete numeric enumeration'
    EVIDENCE[file]['processes'] += '; position-state-oracle.cjs106sourcecases through105/9hostawaits in one invocation, sparse/reentrant/prototype/generalasync/fullclass/live cases outside this floor'

def inventory(manifest=MANIFEST):
    entries = []
    findings = set()
    counts = collections.Counter()
    earlier = json.loads((CRATE / 'tests/fixtures/earlier-ported-roots.json').read_text())['rows']
    earlier_keys = {(row['manifest'], row['symbol']) for row in earlier}
    assert len(earlier) == len(earlier_keys) == 63, 'earlier exact root inventory'
    kinds = collections.Counter(row['kind'] for row in earlier)
    assert kinds['type'] == 19 and kinds['const'] == 9 and sum(count for kind, count in kinds.items() if kind not in {'type', 'const'}) == 35, 'earlier 35 callable / 19 type / 9 constant inventory'
    for file in sorted(manifest.glob('*.toml')):
        data = tomllib.loads(file.read_text())
        for row in data['symbol']:
            counts[row['status']] += 1
            if row['status'] not in {'ported', 'divergent'}:
                continue
            assert file.name in EVIDENCE, f'unassigned four-class inventory file {file.name}'
            assert row['rs'], f'missing Rust target attribution {row["ts"]}'
            note = row.get('note', '')
            bucket = note.split(':', 1)[0].split(' ', 1)[0] if row['status'] == 'divergent' else None
            if row['status'] == 'divergent':
                assert bucket in {'FORCED', 'CHOSEN', 'MIXED', 'CORRECTED', 'FINDING'}, f'unbucketed {row["ts"]}'
                if bucket == 'FINDING':
                    findings.add(row['ts'])
            entries.append({
                'manifest': file.name,
                'symbol': row['ts'],
                'status': row['status'],
                'kind': row['kind'],
                'targets': row['rs'],
                'manifest_note': note,
                'earlier_63': (file.name, row['ts']) in earlier_keys,
                'bucket': bucket,
                'canonical_citations': [anchor for anchor in ['shared-utf8-string-boundary', 'closed-typed-output-boundary', 'owned-snapshot-vs-live-alias'] if anchor in note],
                'source_file_evidence': file.name,
            })
    assert ORIGINAL_FINDINGS <= findings, f'original findings missing: {ORIGINAL_FINDINGS - findings}'
    # Five pending inference rows moved: three ported models/predicate, two
    # CHOSEN coercion consumers retaining the shared UTF-8 boundary.
    # Nineteen pending core/spot builder-constant rows moved to ported.
    # Fourteen pending spot table/closed-order-schema rows moved to ported.
    # Ten pending approval/agent/cancellation rows moved to ported.
    # Fifty pending raw-JSON declared spot model roots moved to ported.
    # Fifteen JSON boundary roots/leaves moved to ported.
    # Twelve Error/market/decoder/tuple roots moved to ported.
    assert counts == {'ported': 382, 'divergent': 106, 'pending': 43}, f'refresh the scoped inventory before interpreting changed rows: {counts}'
    keys = {(row['manifest'], row['symbol']) for row in entries}
    assert len(keys) == len(entries) == 488, 'implemented exact root inventory'
    assert earlier_keys <= keys, f'earlier roots missing: {earlier_keys - keys}'
    assert sum(row['earlier_63'] for row in entries) == 63, 'earlier root attribution'
    return {
        'scope': 'four-class evidence/callee inventory; file-level witnesses are not per-row mutation proofs; residual cardinality targets remain OPEN',
        'iteration_stop': 'all 531 rows in the current Hyperliquid manifests; 488 implemented rows listed, 43 pending rows counted only; further old-row audit paused under porting-first priority',
        'counts': dict(sorted(counts.items())),
        'original_findings': sorted(ORIGINAL_FINDINGS),
        'all_finding_rows': sorted(findings),
        'source_file_evidence': EVIDENCE,
        'rows': entries,
    }


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description='Python 3.11+ TOML inventory; no manifests are written.')
    parser.add_argument('--manifest-dir', type=Path, default=MANIFEST)
    parser.add_argument('--check', type=Path)
    args = parser.parse_args()
    result = inventory(args.manifest_dir)
    if args.check:
        assert json.loads(args.check.read_text()) == result, 'four-class committed inventory drift'
        print(f"four-class inventory: {result['counts']['ported']} ported / {result['counts']['divergent']} divergent / {result['counts']['pending']} pending; 63 earlier roots retained; original findings retained by name")
    else:
        print(json.dumps(result, indent=2))
