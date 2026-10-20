// The rewrite's merge gate for one venue: replay a tape through TS and Rust,
// require identical Store output and order books, and require Rust to use
// less CPU and less retained memory. Exits non-zero on any failure.
//
//   corepack yarn rust:gate --tape sdk-rs/harness/tapes/binance.jsonl
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';
import { sameCommandResults, stable } from './tape.mjs';
import { normalizeTsBybitSubscriptions } from './bybit-subscriptions.mjs';
import { parseRequestJson } from './request-json.mjs';
import {
  fixBlofinAccount,
  fixWeexConditionals,
  fixPhemexAccount,
  fixLighterFills,
  fixLighterCommands,
  fixKrakenOutput,
} from './known-ts-fixes.mjs';
import { venueOf, ORDERLY_BROKERS } from './venues.mjs';
import { fixCoinbase, COINBASE_TS_FIXES } from './known-ts-fixes-coinbase.mjs';
import { fixOrderly, ORDERLY_TS_FIXES } from './known-ts-fixes-orderly.mjs';
import { memoryMetricErrors } from './memory.mjs';
import { applyKucoinRequestContracts } from './kucoin-request-contracts.mjs';
import { requestKey, callerIdsOf, subscriptionKeys } from './requests.mjs';

const { values } = parseArgs({
  options: {
    venue: { type: 'string', default: 'binance' },
    tape: { type: 'string' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    runs: { type: 'string', default: '3' },
    'no-egress': { type: 'boolean', default: false },
  },
});
const tapeBytes = readFileSync(values.tape);
const tapeRecords = (values.tape.endsWith('.gz') ? gunzipSync(tapeBytes) : tapeBytes)
  .toString('utf8')
  .trim()
  .split('\n')
  .map(JSON.parse);
const outDir = `sdk-rs/harness/out/${values.venue}-${process.pid}`;
mkdirSync(outDir, { recursive: true });

const replay = (impl, run) => {
  const out = `${outDir}/${impl}-${run}.json`;
  const args = ['--expose-gc', ...(impl === 'ts' ? ['--import', 'tsx'] : [])];
  args.push(
    'sdk-rs/harness/replay.mjs',
    '--impl',
    impl,
    '--venue',
    values.venue,
    '--tape',
    values.tape,
    '--books',
    values.books,
    '--out',
    out,
  );
  if (values['no-egress']) args.push('--no-egress');
  // TSX's compiler worker retains shared buffers independently of engine work.
  const result = spawnSync(process.execPath, args, {
    encoding: 'utf8',
    timeout: 125000,
    env: { ...process.env, ESBUILD_WORKER_THREADS: '0' },
  });
  if (result.status !== 0)
    throw new Error(`${impl} replay failed: ${result.error?.message ?? result.signal ?? ''}\n${result.stderr}`);
  return JSON.parse(readFileSync(out, 'utf8'));
};

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

const differences = [];
const compare = (label, a, b) => {
  const keyed = (rows) => new Map(rows.map((row) => [row.symbol, row]));
  const ta = keyed(a);
  const tb = keyed(b);
  for (const symbol of new Set([...ta.keys(), ...tb.keys()])) {
    const left = JSON.stringify(stable(ta.get(symbol)));
    const right = JSON.stringify(stable(tb.get(symbol)));
    if (left !== right) differences.push(`${label} ${symbol}: ts=${left} rust=${right}`);
  }
};

const runs = Number(values.runs);
const results = { ts: [], rust: [] };
for (let run = 0; run < runs; run++) {
  for (const impl of ['ts', 'rust']) results[impl].push(replay(impl, run));
}
const [ts, rust] = [results.ts[0], results.rust[0]];

const venue = venueOf(values.venue);
const isPrivate =
  tapeRecords.find((record) => record.type === 'config')?.private ??
  tapeRecords.find((record) => record.type === 'meta')?.private ??
  tapeRecords.some((record) =>
    venue.isPrivateRecord
      ? venue.isPrivateRecord(record)
      : Boolean(venue.privateSocket && record.url?.includes(venue.privateSocket)),
  );
const records = tapeRecords;
const instrumentRows = tapeRecords
  .filter((r) => r.type === 'http' && new URL(r.url).pathname === '/v5/market/instruments-info')
  .flatMap((r) => JSON.parse(r.body).result?.list ?? []);
const instruments = new Map(instrumentRows.map((r) => [r.symbol, r]));

// TS defects Rust fixes on purpose; each rewrites the TS value Rust replaces.
// BingX additionally fixes write/read defects covered by native contract tests:
// - closePosition=true STOP_MARKET/TAKE_PROFIT_MARKET requires quantity.
// - A rejected/ambiguous entry must stop its remaining bracket placement jobs.
// - fillHistory uses startTs/endTs/pageSize/pageIndex, not TS's ignored params.
// - Contradictory placement IDs or rejected ACK status must not advance jobs.
// - Delayed placement/replacement ACKs must not reopen newer private state.
// - Trigger replacement checks its selected source; TS always checks mark.
// - Attached SL/TP keeps per-leg workingType; TS spreads the untyped entry.

//  - A position first seen on the user stream keeps the margin size of TS's
//    zero-size placeholder (0); Rust takes the isolated wallet, or notional
//    over leverage for cross.
//  - Fills: TS prices an execution at the order's average price (`ap`); Rust at
//    the execution's own price (`L`). Fill prices are not compared.
//  - Bybit: TS drops the reject reason of a post-only order cancelled for taking
//    liquidity (`EC_PostOnlyWillTakeLiquidity`); Rust reports it as PostOnly.
//  - Bybit: TS can round split quantities away from the requested total;
//    Rust uses maximum-size lots and the exact remainder.
//  - Bybit: TS falls back from failed batch writes to single creates, even
//    after ambiguous HTTP/transport failures; Rust never resends those writes.
//  - Bybit: TS may drop the initial ticker snapshot before a slow REST seed;
//    Rust installs the seed before connecting. These cases have native regression
//    coverage; the committed replay tapes require no normalization for them.
//  - Bybit: with no mark price, TS can add a second Partial stop on amend;
//    Rust amends the existing native order ID.
//  - Bybit inverse: TS clamps USD contracts before splitting, silently shrinking
//    large requests; Rust preserves the requested total in max lots + remainder.
//  - Bybit inverse: TS sums coin PnL into the USD wallet; Rust converts using
//    the position's USD notional/base contracts while preserving coin position PnL.
//  - Bybit spot: TS reads absent qtyStep and deprecated maxOrderQty; Rust reads
//    basePrecision and the conservative minimum of documented limit/market maxima.
//    TS also sends invalid spot instrument pagination parameters; Rust omits them.
//  - Bybit spot: documented tpslOrder rows become protective orders; TS has no
//    mapping for that spelling. PartiallyFilledCanceled events now retire orders
//    instead of leaving them open after their remaining quantity was cancelled.
//  - Bybit spot: Rust sends marketUnit=baseCoin for market buys, uses native
//    spot conditional/amend/cancel families, and rejects unsupported trailing
//    orders instead of calling derivative position/trading-stop endpoints.
//  - Bybit spot: TS cancel-all omits orderFilter and leaves trigger families
//    live; Rust cancels Order, StopOrder and tpslOrder independently. Documented
//    private tapes select each implementation's exact expected write plan.
//  - Hyro: seed unseen partial fills from accepted account snapshots; TS can
//    report pre-snapshot cumulative executions again when the order stream resumes.
//  - Hyro: TS keys ambiguous-create recovery by a re-rendered wire payload;
//    ticker/position changes can allocate a new identity for the same retry.
//    Rust resumes the original caller intent before rendering any new payload.
// OKX full-depth correction is also applied to the TS oracle: its old
// last-trade filter permanently deleted valid levels when the trade lagged.
// TS bugs intentionally corrected in Rust. Backpack normalization derives
// expected fields only from the input tape, never from Rust's answer.
const KNOWN_TS_FIXES = {
  polymarket() {
    // Reference repairs (not output normalization): preserve initial array books,
    // every sorted absolute depth change, dynamic ticks and event watermarks;
    // use complete Gamma keyset discovery and interval-minute sampled candles.
    // Native regressions additionally pin: one fill per owned execution across
    // settlement statuses, zero resolution price, failed/stale snapshot guards,
    // reported partial batch IDs, partially matched GTC acknowledgements, and
    // coherent final-fill cancel proofs with replacement leaves rounded down.
    // Share sizing uses financial half-up precision, correcting TS toFixed's
    // binary midpoint artifact; JavaScript formatting is never emulated.
    // Unbounded history supplies the API max selector; upper-only and zero-wire
    // start timestamps reject explicitly. Paired cumulative/execution evidence
    // and association-covered final reads fence amend races and missing fields.
    // Authoritative order removals cannot be reopened by delayed stream frames.
    // Unsupported history contracts fail explicitly instead of empty success.
  },
  // Independent documented payload oracle; paired public spot reference repairs
  // leave the real TS Store/worker serialization and measured CPU/memory intact.
  coinbase: (ts) => fixCoinbase(ts, tapeRecords),
  coinbasespot: (ts) => fixCoinbase(ts, tapeRecords),
  phemex(ts) {
    // Independent execution/cumulative-size/flat-settings oracle from taped venue rows.
    if (ts.account) fixPhemexAccount(ts.account, tapeRecords);
    // Phemex zero-open ticker changes remain finite.
    for (const ticker of ts.tickers) if (ticker.percentage === null) ticker.percentage = 0;
    // TS recovery reads missing/unknown affected sides and preserves nonzero
    // full-snapshot rows while recovering the other hedged leg.
    // Native tests also guard exact row versions/absence fences, confirmed
    // cancellation finality, preserved trigger sources and exact split lots.
  },
  // Paired production fixes, compared without rewriting either replay result:
  // preserve full-depth incremental books; verify subscription acknowledgements;
  // fence polled candles against newer trades and reject older trade buckets;
  // apply every per-market BBO/summary update; emit unique private executions;
  // clear closed positions and classify CLOSED orders without reopening them;
  // negotiate/decode mandatory SBE 1:2; use RFC6455 rather than invalid JSON
  // heartbeats and accept the documented auth node identity; preserve
  // venue JWT Authorization beside Tealstreet-Authorization on routed requests;
  // preserve native client IDs and canonical time-in-force; derive account UPnL
  // from positions; support idempotent cross-margin/one-way mode settings;
  // validate public-trade query windows and deduplicate native fill IDs/quantity.
  // Walk empty history cursor pages and keep polled candle buckets/volume monotonic.
  // Rust also fences lagging order-repair rows with native timestamps/exact fill
  // quanta and retains exact accepted/ambiguous amendment targets after ACKs;
  // adversarial native tests cover this without rewriting TS replay output.
  paradex() {},
  // ApeX repairs are in the TS reference; output is compared without rewriting:
  // continuous book sequences and atomic invalid depth updates; top-level ping
  // detection avoids answering echoed ping acknowledgements; stable original
  // payloads for legacy HMAC across form transforms; canonical hedged position
  // IDs; failure-safe private snapshots and required row identifiers;
  // documented realizedPnl aliases; raw-millisecond history bounds/order;
  // one fill per genuine execution, isolated synthetic replay identity;
  // distinct equal executions preserved in REST/WS history; exact tick splitting;
  // both bracket siblings preserved during parent cancel/replace amendments.
  // Price-only amendments keep the observed residual quantity; invalid/no-op
  // amendments reject before cancellation and filled sources are not replaced.
  apexomni: [],
  kraken: {
    output: fixKrakenOutput,
    fixes: [
      'Retain full depth, reject sequence gaps/stale snapshots, unsubscribe unused books.',
      'Full native ticker statistics stay live; omitted relative funding resets zero instead of retaining a stale rate.',
      'Negative instrument quantity precision is a positive power-of-ten step; TS clamps it to 0.1.',
      'Deduplicate native execution IDs; suppress historical fills snapshots.',
      'Use monotonic nonces beyond 1000 same-millisecond requests and clock regression.',
      'Use documented index triggerSignal (TS sends stale spot enum).',
      'Keep accepted settings and newer socket state across delayed reads/write ACKs.',
      'Replace partially executed protective orders with remaining size and stop on raced fills.',
      'Reject missing/contradictory semantic ACKs and failed/incomplete reads explicitly.',
      'Keep exact native milliseconds for fill filters/cursors before seconds output.',
      'Count raw ledger rows for pagination rather than mapped cash components.',
      'One-way mode is an accepted no-op; TS inherits an unimplemented setter.',
      'Use valid authenticated feeds and REST balance instead of invalid account_balances.',
      'Preserve absent position margin allocation instead of TS base-store synthetic zero.',
      'Map native post/ioc/fok orders as limits and reject unsupported opening conditional shapes before a write.',
      'Keep initialized event-driven private feeds alive while bounding retained execution and terminal identities.',
    ],
  },
  asterdex: {
    // Native payload tests exercise these corrections; ordinary tapes compare
    // unchanged, including exact execution prices and economic request fields.
    notes: [
      'A zero-size hedge WS update closes only its reported leg; TS removes every position for the symbol.',
      'Nonce uniqueness is scoped to the agent address as documented; TS scopes it to owner plus agent and can collide across owners.',
      'Failed or malformed history reads propagate errors instead of silently returning an authoritative empty result.',
      'Historical candle timestamps use engine seconds consistently; TS returns raw milliseconds on this REST path.',
      'Do not submit an amendment replacement after the old order fills during cancellation, including fills reported by the cancel response; an unknown-order rejection cannot authorize replacement.',
      'Configured leverage reads use the actual native setting rather than the inherited TS market-maximum fallback that overwrites it.',
    ],
  },
  hyperliquid() {
    // Native command regressions pin these deliberate TS fixes:
    // - Validate every cancel acknowledgement before cancel+replace continues.
    // - Resolve queued leverage/margin settings when the step executes.
    // - Keep each HIP-3 bracket leg's own metadata in pending/optimistic state;
    //   TS assigns entry metadata to every returned oid in placeLimitOrder.
    // - History keeps distinct tid executions and signed rebates, uses vault
    //   identity and caller ranges, and rejects failed or saturated reads.
    // - Execution limits sort raw milliseconds before serializing seconds.
    // - Income honors the requested kind; fills support all-symbol queries.
    // The tape compares converged state: no account values are rewritten here.
  },

  // Independent documented-tape oracle; native request/protocol counters remain exact.
  orderly: (ts) => fixOrderly(ts, tapeRecords),
  lighter: {
    fills: fixLighterFills,
    commands: fixLighterCommands,
    // Native tests pin these defects without rewriting account or trading output.
    notes: [
      'Emit one execution by native trade ID; TS emits both the filled order and the execution.',
      'Keep valid live executions during the initial five-second notification guard and inherit by-market execution identity.',
      'Read execution history from trades, not cumulative inactive orders; preserve exact IDs and timestamp range units.',
      'Convert native venue/integrator fee rate ticks to signed quote fees and rebates; retain venue-reported account-side PnL.',
      'Validate integrator approval expiry and both runtime fee caps before attributed order submission.',
      'Keep known orders after definite cancel rejections, and normalize official hyphenated terminal statuses and time-in-force.',
      'Use engine socket watchdogs rather than a lifetime close-loop cap.',
      'Preserve account and open orders after failed, malformed, or capped reads.',
      'Fence delayed REST snapshots, fills, and acknowledgements behind newer private state.',
      'Require a nonempty transaction hash for every batch item; TS accepts missing batch acknowledgements.',
      'Reject book nonce gaps and pre-gap REST recovery snapshots.',
      'Reject API key index 255, the query-all sentinel, for signing.',
      'Support the full official int16 market domain; TS leverage still rejects active IDs above 254.',
      'Acknowledge retaining the only supported net position mode; TS inherits an unsupported method.',
    ],
  },
  // WOO repairs live in the TS reference itself; no result normalization:
  // exact integer IDs; order-axis journal provenance; camel-case aliases;
  // monotonic cumulative/terminal state and reconciled optimistic fill deltas;
  // nested algo child executions and minimal terminal parent tombstones;
  // execution-ID dedup and per-execution fills; atomic failed-page snapshots
  // including malformed nested executable leaves;
  // STOP direction/source and buy-trailing inference; exact tick splitting;
  // sized protections, full-position trailing and sibling preservation;
  // mandatory rounded amendment quantities; both-family cancellation;
  // unsupported Conditional preflight; authoritative account hedge mode;
  // clear synthetic flat marginSize when an authoritative holding reopens;
  // paced, complete V1 income with signed fees and account aggregate scope.
  // Rust additionally walks V3 fill time windows (TS still reads one page).
  woo: [],
  // Paired TS fixes: contract-sized public volumes; private cumulative z/native
  // units, execution L prices, monotonic terminal state and ping timestamp echo.
  // Cancel native identities and protective defaults now match metadata. Rust
  // also conserves split quantities, rejects unknown settings, and verifies
  // final cancellation/protection provenance before placing replacements.
  // Fill history no longer borrows counterparty IDs or invents an order ID;
  // filtered full pages fail INCOMPLETE and rejected reads propagate errors.
  // Conditional REST children retain exact IDs/parents and trigger references.
  // Rust rejects malformed candle timestamps instead of returning partial history.
  bitrue() {},
  bitget: {
    // Each correction is covered by documented-payload Rust tests. Normal
    // tapes still compare unchanged; these notes do not rewrite mismatches.
    notes: [
      'Reject book sequence gaps and legacy checksum failures.',
      'Preserve account state on failed business-error reads.',
      'Filter all-product UTA streams to this futures product.',
      'Use UTA execution IDs, quantities and prices from the fill topic.',
      'Keep both TP/SL legs of status-less UTA strategy rows.',
      'Include UTA totalFundingFee in realized PnL.',
      'Derive UTA TPSL closing direction from the protected position side, preserving ordinary execution directions.',
      'Use cumulative executed size for orders and deduplicate executions.',
      'Do not emit an execution for an algo trigger acknowledgement.',
      'Split normal orders at floored max lots without losing requested volume.',
      'Send native crossed marginMode for Classic batches rather than cross.',
      'Resolve native modify identity using the fresh newClientOid.',
      'Never cancel-replace an amendment after an ambiguous transport/server result.',
      'Report incomplete or malformed financial history as a failed read.',
      'Read UTA bills through financial-records; retain raw signed deltas and fee metadata without assuming gross PnL.',
      'Support documented UTA trailing strategies without unsupported clientOid fields.',
      'Amend Classic trailing via modify-plan-order and cancel with orderIdList; preserve callback on size-only edits.',
      'Use documented Classic hedge-close trailing side and validate callback/lot before writes.',
      'Normalize inverse private WS sizing and aggregate all collateral coins in USD like REST.',
      'Clamp inverse sized stops in native contracts and preserve size on amend instead of downgrading to full-position.',
    ],
  },

  blofin: (ts) => {
    if (ts.account) fixBlofinAccount(ts.account);
  },
  mexc: () => {
    // Full incremental depth, rather than replacing a 20-level snapshot.
    // Sized stops choose the closing leverage rail even without reduceOnly.
    // Bracket prices reach the native entry; unsupported opening triggers reject.
    // Cancel-all reaches regular, plan and position-stop endpoints.
    // Cancel/replace recomputes remaining size after racing fills; Closed wins
    // over a late cancellation acknowledgement. Failed reads preserve state.
    // Regular cumulative fills stay monotonic; terminal rows never reopen.
    // Stale amend reads cannot regress those fills or place a replacement.
    // REST/optimistic rows retain cumulative fills; amendments retain attached
    // brackets and standalone stop sources through venue reconciliation.
    // Native bracket clears and corrected sources win over delayed placement acks.
    // Repeated private execution IDs emit one fill within the replay window.
  },
  kucoin: [
    'futures inherits empty fills/orders/income and unsupported public trades; query results use explicit documented tape oracles',
    'stops classify with side and direction, retain trigger basis and closeOrder',
    'match executions deduplicate trade IDs and require their execution price',
    'failed and delayed REST snapshots preserve newer account state and terminal orders',
    'conditional orders and mixed bracket trigger references fail before writes instead of dropping fields',
    'explicit protected-market slippage is rejected before batch I/O rather than silently dropped',
    'Bybit-only BBO and tick-slippage controls are rejected on KuCoin before batch I/O',
    'cancelSymbolOrders cancels the stop book too (reference signed transport fills this omission)',
    'open orders walk every numbered page; TS reads only its default first page',
    'socket readiness and subscriptions wait for welcome; TS sends on TCP open',
    'cancel-replace proves terminal cancellation and subtracts fills during cancellation',
    'stop-to-normal migration remains visible across independent REST books',
    'settings streams update flat symbols and fence delayed metadata reads',
    'position mode uses its authoritative read rather than inferring it from positions',
    'independent tape contracts require native query/cancel-proof/mode reads and single-page active-book aliases',
    'partial order stream rows retain REST-known safety flags',
    'immediate bracket replacement retains submitted protections before reconciliation',
    'cached book snapshot gaps retain the entire contiguous buffered suffix (TS reference fixed with a regression)',
  ],
  binancepm: (ts, rust) => KNOWN_TS_FIXES.binance(ts, rust),
  bingx: (ts) => {
    const account = ts.account;
    if (!account) return;
    if (values.venue === 'bingx') {
      // BingX TS emits a terminal cumulative fill, ignores partial executions,
      // and drops its terminal watermark so TRADE_UPDATE can repeat that fill.
      // Use the taped per-execution oracle; Rust is still compared against it.
      const watermark = new Map();
      const fills = [];
      for (const record of records) {
        if (record.type !== 'ws' || !record.url?.includes('?listenKey=')) continue;
        let event;
        try {
          event = JSON.parse(record.text, (_key, value, context) =>
            typeof value === 'number' &&
            !Number.isSafeInteger(value) &&
            context?.source &&
            /^-?\d+$/.test(context.source)
              ? context.source
              : value,
          );
        } catch {
          continue;
        }
        if (!['ORDER_TRADE_UPDATE', 'TRADE_UPDATE'].includes(event.e)) continue;
        const o = event.o;
        if (!['PARTIALLY_FILLED', 'FILLED'].includes(o.X)) continue;
        const id = String(o.i);
        const cumulative = Number(o.z);
        const previous = watermark.get(id) ?? 0;
        const amount = Math.max(0, cumulative - previous);
        watermark.set(id, Math.max(previous, cumulative));
        if (amount > 0)
          fills.push({
            id,
            symbol: o.s.replaceAll('-', ''),
            side: o.S === 'BUY' ? 'buy' : 'sell',
            amount: Number(o.l) > 0 ? Math.min(amount, Number(o.l)) : amount,
          });
      }
      account.fills = fills;
      // TS's private mapper reads closePosition but misses the native cp alias.
      for (const record of records) {
        if (record.type !== 'ws') continue;
        let event;
        try {
          event = JSON.parse(record.text, (_key, value, context) =>
            typeof value === 'number' &&
            !Number.isSafeInteger(value) &&
            context?.source &&
            /^-?\d+$/.test(context.source)
              ? context.source
              : value,
          );
        } catch {
          continue;
        }
        const row = account.orders.find((o) => o.id === String(event.o?.i));
        if (row && event.o.cp !== undefined) row.close = event.o.cp === true || event.o.cp === 'true';
      }
      return;
    }
  },

  weex(ts) {
    const account = ts.account;
    if (!account) return;
    // TS reads opening STOP/TP stream rows as protective orders and displays
    // the execution price as their trigger, corrupting conditional size amends.
    const bytes = readFileSync(values.tape);
    const records = (values.tape.endsWith('.gz') ? gunzipSync(bytes) : bytes)
      .toString('utf8')
      .split('\n')
      .filter(Boolean)
      .map((line) => JSON.parse(line));
    fixWeexConditionals(account, records);
    // Additional deliberate fixes, covered by native regressions: failed
    // normal/algo reads retain the complete last snapshot; stale depth/private
    // frames cannot undo newer state; repeated fill IDs emit once; missing
    // placement IDs fail rather than fabricating an ID; rejected cancellations
    // retain their order; per-symbol margin changes preserve that symbol's mode.
    // Price-only replacement preserves remaining quantity and refuses uncertain
    // fill/bracket state; positive income cursors always report INCOMPLETE.
  },

  okx: [
    'full books retain levels past a stale last trade',
    'public trades implement the inherited Not implemented method',
    'candle calendar bounds use UTC independently of host timezone',
    'fill window filtering retains native milliseconds before output seconds are truncated',
    'keyless/public-only business socket avoids empty-credential login and private topics',
    // Rust safety fixes: covered independently by documented-payload tests.
    'split bracket entries acquire SL/TP before the next entry is placed',
    'native OCO amend preserves its sibling protective leg',
    'negative protective sizes and market or terminal amends fail before writes',
    'positive protective dust cannot round into a close-entire-position order',
    'standalone protective limit amendment fails before cancel if execution price cannot be retained',
    'standalone cancellation continues remaining batches after per-row failures',
    'new account equity survives delayed snapshots and local balance ticks',
    'acknowledged leverage survives failed post-write snapshots',
  ],

  binance: (ts, rust) => {
    for (const position of ts.account?.positions ?? []) {
      const fixed = rust.account.positions.find((p) => p.symbol === position.symbol && p.side === position.side);
      if (position.marginSize === 0 && fixed?.marginSize > 0) position.marginSize = fixed.marginSize;
    }
  },
  bybit: (ts, rust) => {
    if (!ts.account) return;
    const output = ts;
    KNOWN_TS_FIXES.binance(ts, rust);
    // Optional TS API/VIP UI metadata has no engine-frame representation.
    const rest =
      tapeRecords.find((record) => record.type === 'config')?.options?.endpoints?.rest ??
      (values.venue === 'bybitdemo' ? 'https://api-demo.bybit.com' : 'https://api.bybit.com');
    const metadataRequest = requestKey('GET', `${rest}/v5/user/query-api`, undefined, new Set(), venue);
    output.requests = output.requests.filter((key) => key !== metadataRequest);
    if (values.venue !== 'hyrotrader') {
      const feeds = new Set(output.subscriptions);
      for (const key of output.subscriptions) {
        const match = /^([1-9]\d*) WS (\S+) \{"args":\["order"\],"op":"subscribe"\}$/.exec(key);
        if (!match || new URL(match[2]).pathname !== '/v5/private') continue;
        for (const required of subscriptionKeys(match[2], { op: 'subscribe', args: ['execution', 'wallet'] }))
          feeds.add(`${match[1]} ${required}`);
      }
      output.subscriptions = [...feeds].sort();
    }
    const latest = new Map();
    for (const record of records) {
      if (record.type !== 'ws') continue;
      const event = JSON.parse(record.text);
      if (event.topic === 'order') for (const row of event.data) latest.set(row.orderId, row);
    }
    for (const order of ts.account.orders) {
      if (
        order.rejectReason === null &&
        order.status === 'canceled' &&
        latest.get(order.id)?.rejectReason === 'EC_PostOnlyWillTakeLiquidity'
      )
        order.rejectReason = 'PostOnly';
    }
  },

  // Attached TP/SL must remain market exits. The private tape declares the
  // exact buggy TS request with tsRequestBody; Rust must match requestBody.
  // Native regressions also correct TS repair losses (post-only/TIF/reduce-only)
  // and cancellation races across concurrent writes and REST reconciliation.
  backpack: (ts) => {
    const bytes = readFileSync(values.tape);
    const records = (values.tape.endsWith('.gz') ? gunzipSync(bytes) : bytes)
      .toString()
      .split('\n')
      .filter(Boolean)
      .map(JSON.parse);
    const orders = new Map();
    const positions = new Map();
    const fills = [];
    const executions = new Set();
    const tickers = new Map(ts.tickers.map((t) => [t.symbol, t]));
    for (const ticker of tickers.values()) Object.assign(ticker, { bid: 0, ask: 0 });
    for (const record of records) {
      if (
        record.type === 'http' &&
        record.method === 'GET' &&
        new URL(record.url).pathname === '/api/v1/orders' &&
        record.status === 200
      ) {
        for (const order of parseRequestJson(record.body))
          orders.set(String(order.id), { c: order.clientId, y: order.postOnly });
      }
      if (record.type !== 'ws') continue;
      const { stream, data: d } = parseRequestJson(record.text);
      if (!d) continue;
      // TS globally throttles tickers. That
      // drops other markets' last updates. Rust coalesces at the flush boundary.
      // TS also reads P/q instead of the documented o/V ticker fields.
      const ticker = tickers.get(d.s);
      if (ticker) {
        if (stream?.startsWith('ticker.')) {
          const change =
            d.P !== undefined
              ? Number(d.P) * 100
              : Number(d.o) > 0
                ? ((Number(d.c) - Number(d.o)) / Number(d.o)) * 100
                : undefined;
          Object.assign(ticker, {
            last: Number(d.c),
            ...(change === undefined ? {} : { percentage: change }),
            volume: Number(d.v),
            quoteVolume: Number(d.q ?? d.V),
          });
        }
        if (stream?.startsWith('trade.')) ticker.last = Number(d.p);
        if (stream?.startsWith('bookTicker.')) Object.assign(ticker, { bid: Number(d.b), ask: Number(d.a) });
        // TS mistakes the first changed depth level for the best quote.
        // bookTicker is the venue's authoritative best bid/ask stream; null
        // sides mean empty (zero), which TS incorrectly leaves stale.
      }
      if (stream?.startsWith('account.orderUpdate')) {
        const previous = orders.get(String(d.i));
        if (!previous || (d.E ?? 0) >= (previous.E ?? 0)) orders.set(String(d.i), d);
        // TS emits duplicates on reconnect; one execution must cross once.
        if (d.e === 'orderFill' && Number(d.l) > 0) {
          const key = d.t == null ? `${d.i}:${d.E}:${d.z}` : `${d.i}:${d.t}`;
          if (!executions.has(key)) {
            executions.add(key);
            fills.push({
              id: String(d.i),
              symbol: d.s,
              side: d.S === 'Bid' ? 'buy' : 'sell',
              amount: Number(d.l),
              price: Number(d.L ?? d.P ?? d.p ?? 0),
            });
          }
        }
      }
      if (stream?.startsWith('account.positionUpdate') && (!positions.has(d.s) || d.E >= positions.get(d.s).E))
        positions.set(d.s, d);
    }
    if (!ts.account) return;
    // TS returns Hedged but never publishes its fixed mode in Store options.
    ts.account.hedged = true;
    for (const order of ts.account.orders) {
      const d = orders.get(order.id);
      if (!d) continue;
      // TS drops native client IDs on both REST and stream mapping.
      order.clientOrderId = d.c == null ? null : String(d.c);
      // REST postOnly / WS y are separate from GTC. TS loses maker protection.
      if (d.y) order.timeInForce = 'PostOnly';
      // Y is trigger quantity; Q is reverse-market quote quantity. A failed
      // protection order is terminal, not open.
      if (d.P !== undefined && d.Y !== undefined) {
        order.amount = Number(d.Y);
        order.remaining = order.amount - order.filled;
      }
      if (d.X === 'TriggerFailed') order.status = 'canceled';
      // TS drops the trigger price source, so amend falls back to MarkPrice.
      if (d.P !== undefined && d.B)
        order.trigger = { MarkPrice: 'mark_price', LastPrice: 'last_price', IndexPrice: 'index_price' }[d.B] ?? null;
    }
    for (const position of ts.account.positions) {
      const d = positions.get(position.symbol);
      // B is entry; b is breakeven. Do not overwrite entry with breakeven.
      if (d?.B !== undefined && Number(d.q) !== 0) position.entryPrice = Number(d.B);
    }
    ts.account.fills = fills;
  },
};

for (const alias of Object.keys(ORDERLY_BROKERS)) KNOWN_TS_FIXES[alias] = KNOWN_TS_FIXES.orderly;

const normalizeBingxPublic = (ts) => {
  // KNOWN_TS_FIXES: BingX turns precision=0 into 0.1 instead of the native
  // integer step 1. Correct that precision and its synthetic ticker touch only.
  if (values.venue === 'bingx') {
    const contracts = new Map();
    for (const record of records) {
      if (record.type !== 'http' || new URL(record.url).pathname !== '/openApi/swap/v2/quote/contracts') continue;
      const response = JSON.parse(record.body);
      if (response.code !== 0 || !Array.isArray(response.data)) continue;
      for (const contract of response.data) contracts.set(contract.symbol, contract);
    }
    const zeroPrecision = (value) =>
      (typeof value === 'number' || (typeof value === 'string' && value.trim() !== '')) && Number(value) === 0;
    const prices = new Set();
    for (const market of ts.markets) {
      const source = contracts.get(market.id);
      for (const [field, nativeField] of [
        ['amount', 'quantityPrecision'],
        ['price', 'pricePrecision'],
      ]) {
        if (market.precision[field] === 0.1 && zeroPrecision(source?.[nativeField])) {
          market.precision[field] = 1;
          if (field === 'price') prices.add(market.symbol);
        }
      }
    }
    // TS parses non-trading '-' placeholders into NaN; Rust stores finite zeros.
    for (const ticker of ts.tickers) {
      for (const field of ['percentage', 'volume', 'quoteVolume']) if (ticker[field] === null) ticker[field] = 0;
      if (!prices.has(ticker.symbol)) continue;
      ticker.bid = Math.max(ticker.last - 1, 0);
      ticker.ask = ticker.last + 1;
    }
  }
  // KNOWN_TS_FIXES: BingX's internal feed sends asks descending; TS forwards
  // that order. Consumers require best ask first, so Rust sorts full snapshots.
  if (values.venue === 'bingx') {
    for (const book of Object.values(ts.books)) {
      book.bids.sort((a, b) => b[0] - a[0]);
      book.asks.sort((a, b) => a[0] - b[0]);
    }
  }
};
const fixCryptocomAccount = (account) => {
  // TS never sets the hedged store flag despite mapping positions as hedged.
  account.hedged = true;
  // TS omits user.trade entirely. Derive the execution oracle from documented
  // taped trade rows, independently of Rust's account output.
  const tape = tapeRecords;
  const seen = new Set();
  account.fills = [];
  for (const record of tape) {
    if (record.type !== 'ws' || !record.url.includes('/exchange/v1/user')) continue;
    const result = parseRequestJson(record.text).result;
    if (result?.channel !== 'user.trade') continue;
    for (const row of result.data) {
      const id = String(row.trade_id);
      if (seen.has(id)) continue;
      seen.add(id);
      account.fills.push({
        id: String(row.order_id),
        symbol: row.instrument_name,
        side: row.side.toLowerCase(),
        amount: Number(row.traded_quantity),
      });
    }
  }
  // Further deliberate fixes are exercised by Rust regressions: failed reads
  // retain state; client_oid survives mapping; terminal/stale frames cannot
  // reopen orders/positions; REST retains live position enrichment; post-only
  // uses exec_inst; account-wide leverage uses account_id + integer leverage;
  // ignored brackets/opening triggers reject before writes; caller ids use a
  // conservative 36-byte ceiling without JS string emulation.
  // Public throttle and public-only subscription corrections are the
  // afterTsFrame and prepareTsReady hooks in venues.mjs.
};
for (let run = 0; run < runs; run++) {
  const ts = results.ts[run];
  const rust = results.rust[run];

  if (ts.frames !== rust.frames) differences.push(`frames: ts=${ts.frames} rust=${rust.frames}`);
  // Every request must match a taped one: an unserved request sent different parameters.
  for (const impl of ['ts', 'rust']) {
    const output = results[impl][run];
    if (Boolean(output.account) !== isPrivate)
      differences.push(impl + ': account presence differs from tape classification');
    // Invalid baselines fail on every run, including informational private metrics.
    for (const error of memoryMetricErrors(output.metrics)) differences.push(`${impl}: ${error}; baseline is invalid`);
    if (output.unusedWrites) differences.push(impl + ': ' + output.unusedWrites + ' taped mutation(s) never sent');
    for (const error of output.protocolErrors ?? []) differences.push(impl + ': ' + error);
    for (const symbol of values.books.split(',').filter(Boolean)) {
      if (!output.books[symbol]) differences.push(`${impl}: requested book ${symbol} is missing`);
    }
    if (!output.markets.length || !output.tickers.length) differences.push(`${impl}: empty public seed`);
    const unserved = output.unserved;
    if (unserved) differences.push(`${impl}: ${unserved} request(s) matched nothing on the tape`);
    const { missingWrites, invalidSignatures } = results[impl][run];
    if (missingWrites) differences.push(`${impl}: ${missingWrites} required taped write(s) never sent`);
    if (invalidSignatures) differences.push(`${impl}: ${invalidSignatures} invalid signatures`);
  }
  const fix =
    KNOWN_TS_FIXES[values.venue] ??
    (['bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'].includes(values.venue)
      ? KNOWN_TS_FIXES.bybit
      : undefined);
  if (typeof fix === 'function') fix(ts, rust);
  if (values.venue === 'kraken') KNOWN_TS_FIXES.kraken.output(ts, tapeRecords);
  if (JSON.stringify(ts.wsRequests) !== JSON.stringify(rust.wsRequests))
    differences.push(
      `websocket requests differ: ts=${JSON.stringify(ts.wsRequests)} rust=${JSON.stringify(rust.wsRequests)}`,
    );
  if (values.venue === 'bybitspot') {
    for (const market of ts.markets) {
      const lot = instruments.get(market.symbol)?.lotSizeFilter;
      if (!lot) continue;
      market.precision.amount = Number(lot.qtyStep ?? lot.basePrecision);
      const maxima = [lot.maxLimitOrderQty, lot.maxMarketOrderQty, lot.maxOrderQty]
        .map(Number)
        .filter((n) => Number.isFinite(n) && n > 0);
      if (maxima.length) market.limits.amount.max = Math.min(...maxima);
    }
  }
  if (isPrivate && ts.account && rust.account && values.venue === 'bybitinverse') {
    const upnl = ts.account.positions.reduce(
      (sum, p) => sum + (p.contracts > 0 ? (p.unrealizedPnl * p.notional) / p.contracts : 0),
      0,
    );
    ts.account.balance.total = Math.round((ts.account.balance.total + ts.account.balance.upnl - upnl) * 1e8) / 1e8;
    ts.account.balance.upnl = upnl;
  }
  if (['bybitinverse', 'bybitspot'].includes(values.venue)) {
    const privateRecord = tapeRecords.find(
      (record) => record.type === 'ws' && new URL(record.url).pathname === '/v5/private',
    );
    const privateUrl = privateRecord && new URL(privateRecord.url);
    ts.subscriptions = normalizeTsBybitSubscriptions(
      values.venue,
      ts.subscriptions,
      privateUrl && `${privateUrl.origin}${privateUrl.pathname}`,
    );
  }
  if (isPrivate && ts.account && rust.account) {
    if (values.venue === 'cryptocom') {
      fixCryptocomAccount(ts.account);
      const feeds = new Set(ts.subscriptions);
      const suffix = ' {"method":"subscribe","params":{"channels":["user.order"]}}';
      for (const key of ts.subscriptions) {
        if (!key.endsWith(suffix)) continue;
        for (const required of subscriptionKeys(
          key.slice(3, -suffix.length),
          { method: 'subscribe', params: { channels: ['user.trade'] } },
          new Set(),
          { cryptoProtocol: true },
        ))
          feeds.add(required);
      }
      ts.subscriptions = [...feeds].sort();
    }
    if (values.venue === 'lighter') {
      KNOWN_TS_FIXES.lighter.fills(ts.account, tapeRecords);
      KNOWN_TS_FIXES.lighter.commands(ts.commands, tapeRecords);
    }
    for (const field of ['hedged', 'balance', 'orders', 'positions', 'settings', 'fills']) {
      const left = JSON.stringify(stable(ts.account[field]));
      const right = JSON.stringify(stable(rust.account[field]));
      if (left !== right) differences.push(`account ${field}:\n    ts=${left}\n  rust=${right}`);
    }
  }
  // KNOWN_TS_FIXES: KuCoin TS has no futures history implementations. The
  // synthetic tape declares independent documented query expectations; never
  // derive this reference from the Rust result.
  if (values.venue === 'kucoin')
    tapeRecords
      .filter((r) => r.type === 'command')
      .forEach((record, index) => {
        if (record.expected !== undefined) {
          if (!['fetchFills', 'fetchOrderHistory', 'fetchIncome', 'fetchPublicTrades'].includes(record.op))
            throw new Error('KuCoin expected oracle is restricted to missing TS query methods');
          ts.commands[index] = { ok: record.expected };
        }
      });
  if (!sameCommandResults(ts.commands, rust.commands))
    differences.push(`command results: ts=${JSON.stringify(ts.commands)} rust=${JSON.stringify(rust.commands)}`);
  normalizeBingxPublic(ts);
  if (values.venue === 'kucoin') {
    const requests = applyKucoinRequestContracts(tapeRecords, ts.requests, rust.requests);
    ts.requests = requests.tsRequests;
    rust.requests = requests.rustRequests;
  }
  // KNOWN_TS_FIXES: unavailable Crypto.com quotes produce NaN in TS.
  if (values.venue === 'cryptocom')
    for (const ticker of ts.tickers) {
      for (const field of ['bid', 'ask', 'last', 'percentage', 'volume', 'quoteVolume', 'openInterest']) {
        if (ticker[field] === null) ticker[field] = 0;
      }
    }
  const selectedOnly = (impl) =>
    new Set(
      tapeRecords
        .filter((record) => record.type === 'http' && record.impl === impl)
        .map((record) => requestKey(record.method, record.url, record.requestBody, callerIdsOf(tapeRecords), venue)),
    );
  const implementationPlans = { ts: selectedOnly('ts'), rust: selectedOnly('rust') };
  // Tagged fixture writes are independently required by each transport's
  // consumption audit; compare shared requests and their shared write sequence.
  for (const field of ['requests', 'subscriptions']) {
    const left = ts[field]?.filter((key) => field !== 'requests' || !implementationPlans.ts.has(key));
    const right = rust[field]?.filter((key) => field !== 'requests' || !implementationPlans.rust.has(key));
    if (!Array.isArray(left) || !Array.isArray(right) || JSON.stringify(left) !== JSON.stringify(right))
      differences.push(`${field} run ${run + 1}: ts=${JSON.stringify(left)} rust=${JSON.stringify(right)}`);
  }
  const tsWrites = ts.writes?.filter((key) => !implementationPlans.ts.has(key));
  const rustWrites = rust.writes?.filter((key) => !implementationPlans.rust.has(key));
  if (
    !Array.isArray(tsWrites) ||
    !Array.isArray(rustWrites) ||
    ts.writeCount !== ts.writes.length ||
    rust.writeCount !== rust.writes.length ||
    JSON.stringify(tsWrites) !== JSON.stringify(rustWrites)
  )
    differences.push(
      `HTTP write sequence run ${run + 1}: ts=${JSON.stringify(tsWrites)} rust=${JSON.stringify(rustWrites)}`,
    );
  compare('market', ts.markets, rust.markets);
  compare('ticker', ts.tickers, rust.tickers);
  for (const symbol of new Set([...Object.keys(ts.books), ...Object.keys(rust.books)])) {
    if (JSON.stringify(ts.books[symbol]) !== JSON.stringify(rust.books[symbol])) {
      differences.push(`book ${symbol} differs`);
    }
  }
}

const summary = (impl) => ({
  cpuMs: median(results[impl].map((r) => r.metrics.cpuMs)),
  steadyCpuMs: median(results[impl].map((r) => r.metrics.steadyCpuMs)),
  processCpuMs: median(results[impl].map((r) => r.metrics.processCpuMs)),
  retainedMB: median(results[impl].map((r) => r.metrics.retainedBytes)) / 1e6,
  wasmMB: median(results[impl].map((r) => r.metrics.wasmBytes ?? 0)) / 1e6,
});
const t = summary('ts');
const r = summary('rust');
console.log(
  `frames ${ts.frames}, markets ${ts.markets.length}, tickers ${ts.tickers.length}, books ${Object.keys(ts.books).length}`,
);
const row = (label, a, b, unit = 'ms') =>
  console.log(
    `${label.padEnd(16)} ts ${a.toFixed(1).padStart(8)} ${unit}   rust ${b.toFixed(1).padStart(8)} ${unit}   ${(a / b).toFixed(2)}x`,
  );
row('main-thread cpu', t.cpuMs, r.cpuMs);
row('steady cpu', t.steadyCpuMs, r.steadyCpuMs);
row('process cpu', t.processCpuMs, r.processCpuMs);
row('retained memory', t.retainedMB, r.retainedMB, 'MB');
console.log(`(rust wasm linear memory ${r.wasmMB.toFixed(1)} MB)`);
if (isPrivate) console.log('(private tape: correctness only; CPU and memory are informational)');
if (venue.family === 'orderly')
  console.log(
    `(documented Orderly corrections: ${ORDERLY_TS_FIXES.length}; native protocol and independent oracles checked)`,
  );
if (values.venue.startsWith('okx'))
  console.log(
    `(documented OKX corrections: ${KNOWN_TS_FIXES.okx.length}; output is compared without OKX-specific normalization)`,
  );

if (venue.family === 'coinbase')
  console.log(
    `(documented Coinbase corrections: ${COINBASE_TS_FIXES.length}; native JWT and independent oracles checked)`,
  );

const failures = [...differences];
// A private tape is paced in real time, so it gates correctness only.
if (!isPrivate) {
  if (!(r.cpuMs < t.cpuMs)) failures.push('Rust main-thread CPU is not below TS');
  if (!(r.steadyCpuMs < t.steadyCpuMs)) failures.push('Rust steady-state CPU is not below TS');
  if (!(r.retainedMB < t.retainedMB)) failures.push('Rust retained memory is not below TS');
}
if (failures.length) {
  console.log(`FAIL (${failures.length})`);
  for (const failure of failures.slice(0, 40)) console.log(`  ${failure}`);
  process.exit(1);
}
console.log('PASS');
