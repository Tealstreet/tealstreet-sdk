# CLAUDE.md — safe-cex/src/store

`FrameStore` is the account state implementation for LOCAL, COSIGNED and
DELEGATED. It reads SCXF through the pure-JS `@tealstreet/safe-cex-wasm/wire`
reader directly into Rust-generated `StoreData`. It imports no wasm engine.
Mobile is DELEGATED-only and uses this same reader/store on Hermes.

## State and subscriptions

`data` is the current generated snapshot; generated-field getters and cached
market/ticker ID and symbol indexes expose the same rows. State changes come
from frames, with no mutable update API, serialization transport, JSON round
trip, normalizer or per-frame deep clone.

`subscribe(listener)` observes a snapshot plus its section versions.
`subscribe(section, listener)` observes a typed value, version and snapshot.
`version(section)` advances only when the section's value changes. Equal
sections retain identity. Ticker deltas copy their array and changed rows;
other sections remain shared. Each changed row is copied once per frame, and
same-frame observations fill those unpublished copies in place: Hermes (mobile)
pays per allocation, and per-field spreads made ticker ingest twice as slow
(AGT-91). Reentrant notifications are delivered in order.
A failing listener does not prevent peers from running or trigger recovery.

`reset()` releases stream baselines and restores generated unloaded defaults.
Defaults never establish decoded readiness. Generated `updateIds` advance only
for actual order/position changes. Equal and empty stream sections retain IDs,
versions, row identities and UI subscription silence. Separate `observationIds`
track stream receipts, including complete empty censuses; ACCOUNT_READ and
recovery snapshots never advance those receipt counters.
`subscribeAccountObservations` serves equal-census receipts to account logic
without UI subscription notifications; chaser loss confirmation uses that path.
Observation callbacks cannot suppress accepted UI updates: snapshots queue before
callbacks run, and callback failures propagate through the existing subscriber
diagnostic path after UI peers receive the queued changes. Reentrant updates
retain snapshot/version order.

## Source ownership

A Rust worker emits frames and exact native late-subscriber snapshots. There
is no worker `op: store`, serialized seed or derived state side channel.
Worker recovery creates a fresh native source; it does not ship reset objects.

A page mirror applies source frames once through `applyWorkerFrame`. Its
optional page write engine consumes those bytes for signing and write fences;
that engine's output cannot overwrite the source's account state. Public-only
mirrors accept source frames without constructing a page engine.

Derived sections replace socketHealth, activeChasers, activeTWAPs, ordersRisk,
displayedPositions, displayedMargin and balance.info. Absence preserves state;
explicit empty values clear it. Unknown displayed-margin values remain null.
Native subscription diagnostics retain their existing table protocol. Section 33 joins Rust-authored managed-order ownership by exact order ID; TypeScript only copies those facts and clears previous tags on empty replacement.

## Baselines and recovery

`ingest(frame, { snapshot, seq })` accepts optional transport sequence metadata.
A gap refuses deltas until a snapshot establishes a new sequence domain.
Snapshots reset book baselines and decoded readiness. Authoritative replacement
position tables remove omitted rows, including every row on an empty table.
No closed exposure is manufactured. Native POSITIONS projects the generated
`positionSettings` cache by exact position ID, retaining confirmed settings
after closure and recovery snapshots; native SETTINGS updates retained rails. Explicit reset clears this cache. Row update counters remain monotonic across both snapshots and reset.

STATUS loaded bits cannot prove that a section decoded. Accepted empty tables
count as evidence. A rejected section revokes only its evidence; bad market
identity revokes indexed evidence and retains the last accepted catalogue.
Independent balance, diagnostic and derived sections still apply. Stage market
identity before committing a table; explicit execution identity remains usable
while catalogue repair is pending.

Book and candle failures retain symbol/interval recovery scopes. A rejected
book batch invalidates baselines until fresh full books arrive. Full books
replace both sides, including an empty reset. Deltas upsert absolute amounts
and delete amount-zero levels; finite zero prices remain valid. Consumers get
sorted, cumulative levels without mutating previously published books. Numeric
baselines are copied off source buffers before transfer. Delta baselines are
sorted nonzero levels, merged per section rather than copied and re-sorted; a
later row for the same price in one section wins, as with the Map it replaced.
Reset discards cached frame ingestion as well as numeric baselines, allowing a
snapshot to be reused.

Apply errors request bounded native snapshots. Snapshots contain no executions,
so valid original FILLS must be delivered once. FrameStore exposes book,
candle and fill readers; BaseExchange owns worker subscription dispatch and
bounded RPC recovery. Failing listeners are separate from protocol errors.

## Native quantities and identity

The venue adapter and native frame writer own published SDK quantity units.
COIN-M orders carry native contract counts while positions carry base quantity.
Binance COIN-M's fixed USD contract multiplier remains in native sizing extras.
For Bybit/Bitget inverse, position notional is the native quote-contract size,
published separately as `extra.contractsNative`. Binance COIN-M computes native
position counts by dividing quote notional by market contractSize in Rust.
Copier closes consume native size, rather than rounded base contracts.

MEXC, BloFin, KuCoin futures, Coinbase and OKX preserve native placement-unit
order extras. Exact `remainingNative`, `filledInPlacementUnits`,
`remainingNotional` and `conditionalOrderPrice` values pass through unchanged.
The JS reader does not reconstruct missing quantities from rounded base sizes.
Reported zero remains distinct from the packed absent-value NaN sentinel.
Position notional decodes that sentinel to the generated nullable `null`;
finite values, including zero, pass through exactly, and infinities reject.
Order rejection and bracket trigger metadata remain beside these quantities.

OKX contract-count placement amounts remain separate from base fills and
remaining quantities. Inverse remaining notional uses counts times quote
contract value. Native position counts use base contracts/ctVal for linear
and quote notional/ctVal for inverse products. OKX already normalizes inverse
PnL to USD; do not relabel it as coin-native PnL.

Position revision 2 requires exact native position ID and symbol references,
so Cross/Isolated rows and hedge legs remain distinct. Native-contract presence
bits require finite values; invalid present values cannot select a fallback.
Unknown liquidation price remains null. Native display leverage and PnL are
finite, and the decoder validates the contract without repairing numbers.

SETTINGS retains independent buy/sell leverage rails and exact symbols.
Its replacement table is authoritative, including query results and snapshots.
Unusable/nonfinite/nonpositive cached rails remain unknown to writer preflight.

FILLS revision 2 requires the native-computed event ID and symbol. Attribution
keeps orderId separately. Optional fee/PnL NaN means absent; reported zero stays
reported. Legacy execution frames missing native identity are refused.

## Public metadata and observations

Market decoding preserves prettySymbol, quantity flags, derivative terms,
prediction metadata, underlying tokens and instrument classifications using
the generated Market contract. Native numeric zero remains published; optional
infinity is invalid. Exact position IDs scope Greeks to their current rows.

Ticker tables retain orphan symbol/ID references without manufacturing markets.
Packed delta indexes stay aligned with that table. Midpoint omission preserves
an existing value, a numeric value replaces it and explicit packed absence
clears it. TICKER_OBSERVATIONS retains independent last/mark/index receipt times;
processing or snapshot receipt does not make a cached quote fresh.

Balance frames merge published fields while retaining omitted coins,
currencies and info. Explicit empty maps replace them; native balance.info null
clears it. No TS repair fabricates missing balance dictionaries.

Subscription diagnostics section 25 replaces native-owned groups on snapshots
and patches named topics on periodic updates. Shared-provider groups remain
independent. Shared public-provider section 34 is republished by the receiving native engine, including late snapshots; section 35 carries native HTTP requestStats counters. Row groups are public/business/futures/private; never-received
timestamps stay zero and a retired subscription has zero callbacks.

## Verification

Use package-local `yarn typecheck` and scoped vitest paths. The typecheck includes
Rust-generated shape/parity tests and the actual FrameStore dependency graph
under NX strict language options. Native-unit, nullability, transfer, sequence,
structural-sharing and recovery fixtures must exercise the real wire reader.
Test producers may encode generated DTOs as SCXF; they must not recreate a
mutable store API or bypass native decoding with object patches.
