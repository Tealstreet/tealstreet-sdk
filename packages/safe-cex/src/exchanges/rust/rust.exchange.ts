import type {
  AccountAuthResult,
  AlgoStopAllResult,
  CommandProgress,
  EngineBridgeConstruction,
  EngineCapabilities,
  EngineConstruction,
  EngineEvent,
  EngineHealth,
  EngineHelperCapabilities,
  EngineHost as EngineHostType,
  Transport,
  VenueErrorDetail,
  WasmEngineLike,
} from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeErrorKind } from '../../commandOutcome';
import type { Store } from '../../store/store.interface';
import type {
  AccountGreeks,
  ActiveChaser,
  ActiveTWAP,
  Candle,
  ClosedPnlEntry,
  ExchangeAccount,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  FetchedLeverage,
  HistoricalOrder,
  HistoricalTradeOptions,
  IncomeHistoryOptions,
  IncomeRecord,
  Market,
  MarginType,
  NukeResult,
  OHLCVOptions,
  OptionChainQuery,
  OptionChainRow,
  Order,
  OrderBook,
  OrderHistoryOptions,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PublicTrade,
  RemoteSignerRequest,
  RemoteSignerResponse,
  UpdateOrderOpts,
  ValidateAccountResult,
} from '../../types';
import type { TwapOwner } from '../../webworker/twap/TwapOwner';
import type { WorkerMethodArguments } from '../../webworker/workerMethodArguments';
import type { AsterDEXPlaceOrderOpts, AsterDEXStpMode } from '../asterdex/asterdex.types';
import type {
  CallableMethodsInWorker,
  EngineSnapshot,
  ExchangeCapabilities,
  OHLCVFrameRejection,
  OrderBookFrameRejection,
} from '../base';
import type { BaseWebSocket } from '../base.ws';
import type { ClientOrderIdRule, ClientOrderLookup } from '../clientOrderId';
import type { LighterAccountTier } from '../lighter/lighter.account-tier';
import type { ChaseEnd } from './algoControlWatch';
import type { AsterDEXPublicSetup } from './asterdexSignerSetup';
import type { FailedPrivateReadKind } from './startupDiagnostics';

import { DEFAULT_COMMAND_TIMEOUT_MS, EngineHost, ExchangeError, nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { Frame, LOADED_FLAG, TICKER_FIELDS } from '@tealstreet/safe-cex-wasm/wire';
import { engineEventClassified, isErrorReported, markErrorReported } from '../../commandOutcome';
import { exchangeMetadata, getExchangeMetadata } from '../../exchangeMetadata';
import { copyHistoryCompleteness } from '../../historyCompleteness';
import { mapBitunixPositionHistory } from '../../journal/bitunix.position-adapter';
import { mapBlofinPositionHistory } from '../../journal/blofin.position-adapter';
import { aggregateBybitClosedPnl } from '../../journal/bybit.position-adapter';
import { mapOkxPositionHistory } from '../../journal/okx.position-adapter';
import { wooOrdersToPositionRecords } from '../../journal/woo.position-adapter';
import { SECONDARY_READ_KEYS_ENABLED } from '../../secondaryReadKeys';
import { FrameApplier, FrameApplyError } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import { WorkerFrameRecoveryScopes } from '../../store/workerFrameRecoveryScopes';
import { bybitPublicRateLimitReport } from '../../transport/bybitPublicRateLimitTelemetry';
import { assertEngineAccountRouting, createEngineTransport, withSecondaryReadKeyPolicy } from '../../transport/engine';
import { withRequestsLog } from '../../transport/requestsLog';
import { tunnelProxyFallbackReport } from '../../transport/tunnelProxyFallbackTelemetry';
import { LogSeverity, OrderSide, OrderType, PositionSide } from '../../types';
import { diagnosticConsole } from '../../utils/diagnosticRedaction';
import { nativePositionSize } from '../../utils/nativePositionSize';
import { multiply } from '../../utils/safe-math';
import { subscribeTickIntervalMultipliers } from '../../utils/tickIntervalOverride';
import { uuid } from '../../utils/uuid';
import { virtualClock } from '../../utils/virtualClock';
import { stopAccountAlgos as stopLocalAccountAlgos } from '../../webworker/stopAccountAlgos';
import { captureTwapOwner } from '../../webworker/twap/TwapOwner';
import {
  claimWriteTiming,
  emitWriteTiming,
  runWithWriteTiming,
  TIMED_WRITE_METHODS,
  WriteTiming,
} from '../../writeTiming';
import { isAsterDEXStpMode, resolveAsterDEXApiVersion } from '../asterdex/asterdex.types';
import {
  BaseExchange,
  CAPABILITY_METHODS,
  markBaseCapabilityFallback,
  summarizeSocketHealth,
  WORKER_FORWARDED_METHODS,
  WORKER_WRITE_METHODS,
  workerCallClientOrderIds,
} from '../base';
import { BROKER_ID as BLOFIN_BROKER_ID } from '../blofin/blofin.types';
import {
  HYROTRADER_PROD_URL,
  HYROTRADER_PROD_WS_URL,
  HYROTRADER_SIMULATOR_URL,
  HYROTRADER_SIMULATOR_WS_URL,
} from '../bybit/bybit.types';
import {
  decodeWorkerOrderIds,
  isRegularClientOrderShape,
  patternClientOrderIdRule,
  withReplacementClientOrderId,
} from '../clientOrderId';
import { isKeylessRemoteSigned } from '../hostedSigningVenues';
import { hyperliquidClientOrderIdRule } from '../hyperliquid/hyperliquid.cloid';
import { parseLighterApiKeyIndex } from '../lighter/lighter.utils';
import { BROKER_ID as OKX_BROKER_ID } from '../okx/okx.types';
import { BROKER_ID as PHEMEX_BROKER_ID } from '../phemex/phemex.types';
import { txflowClientOrderIdRule } from '../txflow/txflow.client-id';
import { AlgoControlWatch } from './algoControlWatch';
import { mapAsterDEXSignerSetup } from './asterdexSignerSetup';
import { assertBetaAccount, assertBetaVenue, BETA_VENUES, isBetaVenue } from './betaVenues';
import {
  deriveBridgeConstruction,
  deriveEndpointSetupPolicy,
  derivePublicMode,
  deriveRemoteSigningPolicy,
  deriveSignerSetupMapping,
  deriveSignerSetupPolicy,
} from './construction';
import { DelegatedSignerSetup, selectDelegatedSigningMode, withoutDelegatedSigning } from './delegatedSignerSetup';
import { snapshotDelegatedSigningOptions, withoutDelegatedSigningSnapshot } from './delegatedSigningSnapshot';
import { mapHyperliquidSignerSetup } from './hyperliquidSignerSetup';
import { signHostedHttp, signHostedWsAuth } from './legacySigning';
import { LocalSignerSetup, readCredentialsValidity, withoutSigningCredentials } from './localSignerSetup';
import { mapMEXCSignerSetup } from './mexcSignerSetup';
import { mapParadexSignerSetup } from './paradexSignerSetup';
import {
  IN_PROCESS_PUBLIC_FEED_TIMEOUT_MS,
  inProcessPortPair,
  RustPublicFeedClient,
  rustPublicFeedIdentity,
  serveInProcessPublicData,
} from './publicFeed';
import { RustSocketHandle } from './rustSockets';
import { failedPrivateReadKind, startupDiagnosticTags } from './startupDiagnostics';

/** The public registry and direct construction share the same beta policy. */
export const RUST_VENUES: ReadonlySet<ExchangeName> = new Set(BETA_VENUES);
export const isRustVenue = (name: ExchangeName) => isBetaVenue(name);
export const runsOnRustEngine = (name: ExchangeName, _options?: Partial<ExchangeOptions>) => isBetaVenue(name);

const okxClientOrderIdRule = patternClientOrderIdRule('OKX', /^[A-Za-z0-9]{1,32}$/, {
  mint: () => `${OKX_BROKER_ID}${uuid()}`.slice(0, 32),
});
const bybitClientOrderIdRule = patternClientOrderIdRule('Bybit', /^[A-Za-z0-9_-]{1,36}$/);
const LIGHTER_TIER_SYNC_MS = 15_000;
/** Past a hold's `liftAfterMs`, so the source's read goes out after the write it must cover. */
const SOURCE_ORDERS_SLACK_MS = 500;
/** The engine's unsynced grace, for an `orders.held` event that names none. */
const UNSYNCED_FENCE_GRACE_MS = 30_000;
/** TS replaces `balance.info` on each REST balance tick; the engine caches the same payload. */
const BALANCE_INFO_SYNC_MS = 2_000;
// ApeX and KuCoin Spot retain their existing getAccount command aliases below.
const orderlyClientOrderIdRule = patternClientOrderIdRule('Orderly', /^(?!-)[\x00-\x09\x0b\x0c\x0e-\x7f]{1,36}$/);
const bitgetClientOrderIdRule = patternClientOrderIdRule('Bitget', /^[.A-Z:/a-z0-9_-]{1,32}$/);
const coinbaseClientOrderIdRule = patternClientOrderIdRule('Coinbase', /^\S{1,128}$/u, {
  mint: () => `tealstreet-${uuid()}`,
});
const clientOrderIdRules: Partial<Record<ExchangeName, ClientOrderIdRule>> = {
  coinbase: coinbaseClientOrderIdRule,
  coinbasespot: coinbaseClientOrderIdRule,
  phemex: patternClientOrderIdRule('Phemex', /^.{1,40}$/, { mint: () => `${PHEMEX_BROKER_ID}_${uuid()}` }),
  apexomni: patternClientOrderIdRule('ApeX Omni', /^\S+$/),
  binance: patternClientOrderIdRule('Binance', /^[.A-Z:/a-z0-9_-]{1,36}$/, { mint: () => `x-s3vFFRQp${uuid()}` }),
  binancepm: patternClientOrderIdRule('Binance PM', /^[.A-Z:/a-z0-9_-]{1,32}$/, {
    mint: () => `x-s3vFFRQp${uuid()}`.slice(0, 32),
  }),
  binancespot: patternClientOrderIdRule('Binance Spot', /^[A-Za-z0-9_-]{1,36}$/, { mint: () => `x-s3vFFRQp${uuid()}` }),
  kodiakfi: orderlyClientOrderIdRule,
  mode: orderlyClientOrderIdRule,
  what: orderlyClientOrderIdRule,
  woofi: orderlyClientOrderIdRule,
  woo: {
    label: 'WOO',
    mint: () => String(100_000_000_000_000 + Math.floor(Math.random() * 900_000_000_000_000)),
    requirement: 'be a canonical positive decimal integer within the JavaScript safe-integer range',
    isValid: (id) => /^[1-9]\d*$/.test(id) && Number.isSafeInteger(Number(id)),
  },
  bitget: bitgetClientOrderIdRule,
  bitgetusdc: bitgetClientOrderIdRule,
  bitgetinverse: bitgetClientOrderIdRule,
  backpack: {
    label: 'Backpack',
    isHonorableShape: (opts) => isRegularClientOrderShape(opts) && opts.close !== true,
    requirement: 'be a canonical decimal uint32 between 1 and 4294967295',
    isValid: (id) => /^[1-9]\d{0,9}$/.test(id) && Number(id) <= 0xffff_ffff,
  },
  hyperliquid: hyperliquidClientOrderIdRule,
  hyperliquidspot: hyperliquidClientOrderIdRule,
  paradex: patternClientOrderIdRule('Paradex', /^.{1,64}$/, {
    mint: () => `ts_${uuid()}`,
    isHonorableShape: (opts) => isRegularClientOrderShape(opts) && opts.close !== true,
  }),
  txflow: txflowClientOrderIdRule,
  blofin: patternClientOrderIdRule('BloFin', /^[A-Za-z0-9]{1,32}$/, {
    mint: () => `${BLOFIN_BROKER_ID}${uuid()}`.slice(0, 32),
  }),
  bingx: {
    label: 'BingX',
    isHonorableShape: (opts) => isRegularClientOrderShape(opts) && opts.close !== true,
    mint: () => `ts${uuid()}`.toLowerCase().slice(0, 40),
    requirement: 'be 1-40 characters with no uppercase letters',
    isValid: (id) => id.length >= 1 && id.length <= 40 && id === id.toLowerCase(),
  },
  bitunix: patternClientOrderIdRule('Bitunix', /^\S+$/),
  bitrue: patternClientOrderIdRule('Bitrue', /^.{1,31}$/),
  grvt: {
    label: 'GRVT',
    requirement: 'be a canonical decimal integer between 1 and 9223372036854775807',
    isValid: (id) => /^[1-9]\d{0,18}$/.test(id) && BigInt(id) <= 9223372036854775807n,
  },
  kucoin: patternClientOrderIdRule('KuCoin', /^[A-Za-z0-9_-]{1,40}$/),
  kucoinspot: patternClientOrderIdRule('KuCoin Spot', /^[A-Za-z0-9_-]{1,40}$/),
  cryptocom: patternClientOrderIdRule('Crypto.com', /^.{1,36}$/),
  weex: patternClientOrderIdRule('WEEX', /^[.A-Z:/a-z0-9_-]{1,36}$/, {
    mint: () => `ts${uuid()}`.slice(0, 36),
  }),
  bybit: bybitClientOrderIdRule,
  bybitinverse: bybitClientOrderIdRule,
  bybitspot: bybitClientOrderIdRule,
  bybitdemo: bybitClientOrderIdRule,
  okx: okxClientOrderIdRule,
  okxeu: okxClientOrderIdRule,
  okxdemo: okxClientOrderIdRule,
  okxeudemo: okxClientOrderIdRule,
};

export type WasmEngineModule = {
  resolveConstruction?: (venue: string, optionsJson: string) => string;
  WasmEngine: (new (venue: string, optionsJson: string, signerSetupJson?: string) => WasmEngineLike) & {
    /** Whether `frameFed: true` construction is implemented for this venue's family. */
    frameFedSupported?: (venue: string) => boolean;
  };
};

const frameFedSupported = (wasm: WasmEngineModule, venue: string): boolean =>
  wasm.WasmEngine.frameFedSupported?.(venue) === true;

/** Preserve native constructor classification at the same boundary as command failures. */
type SignerSetup = LocalSignerSetup | DelegatedSignerSetup;
const constructEngine = (
  Constructor: WasmEngineModule['WasmEngine'],
  venue: string,
  options: string,
  setup?: SignerSetup,
): WasmEngineLike => {
  try {
    return setup ? setup.construct(Constructor, venue, options) : new Constructor(venue, options);
  } catch (error) {
    const encoded = (error as { venueError?: unknown } | null)?.venueError;
    let detail: VenueErrorDetail | undefined;
    if (typeof encoded === 'string') {
      try {
        detail = JSON.parse(encoded) as VenueErrorDetail;
      } catch {
        /* Older or custom loaders retain their original error. */
      }
    }
    if (detail && typeof detail.message === 'string') throw new ExchangeError(detail.message, detail);
    throw error;
  }
};

/**
 * How this realm loads the wasm engine for a Rust venue: that venue's family
 * build in browsers and Workers, the combined build on servers and in tests.
 * Registered once by the host, like the transport. Loaders share pending and
 * completed initialization: wasm-bindgen init replaces module-wide memory.
 */
const defaultWasmLoader = async (venue: string): Promise<WasmEngineModule> => {
  const { loadEngine } = await import('@tealstreet/safe-cex-wasm/engine');
  return loadEngine(venue);
};
let loadWasmEngine: (venue: string) => Promise<WasmEngineModule> = defaultWasmLoader;
export const setWasmEngineLoader = (loader: (venue: string) => Promise<WasmEngineModule>) => {
  loadWasmEngine = loader;
};

const synchronizedEngineTransport = (noteClockJump: (now: number, utc: number) => void): Transport => {
  // Keep scheduling stable across clock corrections and clocks that stop in
  // suspend. Clock reads record a correction and defer resync; EngineHost
  // applies protocol time outside the physical HTTP handoff.
  const origin = virtualClock.getCurrentTimeMs();
  const started = performance.now();
  let sampledMonotonic = started;
  let sampledWall = Date.now();
  return {
    ...nativeTransport(),
    now: () => {
      const monotonic = performance.now();
      const wall = Date.now();
      const now = origin + (monotonic - started);
      if (Number.isFinite(monotonic) && Number.isFinite(wall)) {
        const divergence = (wall - sampledWall) - (monotonic - sampledMonotonic);
        if (Math.abs(divergence) > 1_000) {
          noteClockJump(now, virtualClock.getCurrentTimeMs());
          // A wall step leaves virtualClock's cached server offset stale.
          // Defer even Node's synchronous sync notification past this read.
          void Promise.resolve()
            .then(() => virtualClock.syncClientServerTime({ afterClockStep: true }))
            .catch(() => {
              // Best effort; scheduled synchronization can retry.
            });
        }
        sampledMonotonic = monotonic;
        sampledWall = wall;
      }
      return now;
    },
  };
};
let engineTransport: (() => Transport) | undefined;
/** Explicit hosts own their clock; omit the factory to restore the synchronized SDK default. */
export const setRustEngineTransport = (factory?: () => Transport) => {
  engineTransport = factory;
};

const ALGO_CONTROL_OPS = ['chaser:start', 'chaser:terminate', 'twap:start', 'twap:resume', 'twap:pause', 'twap:stop'];
const MULTI_COMMAND_WORKER_METHODS = new Set<CallableMethodsInWorker>([
  'cancelAllOrders',
  'nuke',
  'nukePositions',
  'nukeOrders',
]);
const NUKE_CLOSE_RETRIES = 3;
const NUKE_CLOSE_RETRY_MS = 100;
const NUKE_FLAT_ROUNDS = 3;
const NUKE_FLAT_SETTLE_MS = 2_000;
/** The least wait after a close round before the read that can prove flat. */
const NUKE_READ_SETTLE_MS = 500;
const NUKE_NOT_CONFIRMED = 'Nuke could not confirm the account is flat';
const describeOpen = (positions: Pick<Position, 'symbol' | 'side' | 'contracts'>[]) =>
  positions.map((p) => `${p.symbol} ${p.side} ${p.contracts}`).join(', ');
type FreshPositionsRead = {
  skipped: string[];
  open: Pick<Position, 'symbol' | 'side' | 'contracts'>[];
};
/** Only this exact shape can prove anything; an old array or a partial row proves nothing. */
const isFreshPositionsRead = (body: unknown): body is FreshPositionsRead => {
  const read = body as Partial<FreshPositionsRead> | null;
  return (
    Array.isArray(read?.skipped) &&
    read.skipped.every((symbol) => typeof symbol === 'string') &&
    Array.isArray(read.open) &&
    read.open.every(
      (row) =>
        typeof row?.symbol === 'string' &&
        typeof row.side === 'string' &&
        typeof row.contracts === 'number' &&
        Number.isFinite(row.contracts),
    )
  );
};
const NUKE_ORDERS_MAY_REMAIN = 'Orders, including triggers and TP/SL, may remain on this venue: check open orders';
const NUKE_UNSEEN_ORDERS =
  'Orders placed in the last few seconds may not be cancelled on this venue: check open orders';
/** The one rule for when a nuke warns that unseen orders may rest. */
const unseenOrdersWarning = (capabilities: EngineCapabilities): string | undefined =>
  capabilities.symbolCancelMissesUnseen && !capabilities.nukeAccountCancel ? NUKE_UNSEEN_ORDERS : undefined;
/** The failure, its message ending with the warning; a frozen error is wrapped, not changed. */
const withWarning = (error: unknown, warning: string | undefined): unknown => {
  if (!warning) return error;
  const message = `${errorMessage(error)}. ${warning}`;
  if (error instanceof Error && !Object.isFrozen(error)) {
    error.message = message;
    return error;
  }
  return Object.assign(new Error(message), error instanceof Error ? { ...error } : {}, { cause: error });
};
/** A stop's own result, plus the placements the stopping engine attempted. */
type AlgoStopWithPlacements = NukeResult & { placedSymbols?: string[] };
const errorMessage = (error: unknown): string => (error instanceof Error ? error.message : String(error));
const NATIVE_ACCOUNT_READS = ['fetchOrders', 'fetchPositions', 'fetchBalance', 'fetchPositionMode'] as const;
/**
 * Engine methods a main-thread worker mirror cannot answer and does not forward:
 * they throw by name instead of failing later as "engine is not started".
 */
export const WORKER_MIRROR_UNSUPPORTED_METHODS = ['validateAccount', 'fetchEngineSnapshot', 'snapshotFrame'] as const;
export type WorkerMirrorUnsupportedMethod = (typeof WORKER_MIRROR_UNSUPPORTED_METHODS)[number];
/** The main-thread exchange's surface: everything else forwards or is correct locally. */
export type WorkerMirrorExchange = Omit<RustEngineExchange, WorkerMirrorUnsupportedMethod | 'fetchFillsForOrderIds'>;

/** Account history a public-only account cannot read: refused as not read, never answered empty. */
const PRIVATE_HISTORY_READS: ReadonlySet<string> = new Set([
  'fetchHistoricalFills',
  'fetchHistoricalOrders',
  'fetchIncomeHistory',
  'fetchIncomeHistoryRange',
]);

/** The engine commands behind those reads, refused the same way after the caller's input is checked. */
const PRIVATE_HISTORY_OPS: ReadonlySet<string> = new Set(['fetchFills', 'fetchOrderHistory', 'fetchIncome']);

const notReadOnPublicData = (what: string): ExchangeError => {
  const message = `${what} was not read: this account is on public data`;
  return new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
};

/** Rust-only engine methods (not on BaseExchange) a mirror forwards whole. */
const RUST_FORWARDED_METHODS = ['fetchStpMode', 'changeStpMode', 'fetchOrderBook', 'readPositions'] as const;

/** A frame-fed mirror signs and sends these itself; everything else still goes to the account's data source. */
/** Placements that wait for the page engine on a mirror instead of taking the source's slower leg. */
const PAGE_ENGINE_PLACEMENTS: ReadonlySet<string> = new Set([
  'placeOrder',
  'placeOrders',
  'placeOrderWithClientOrderId',
]);

const FRAME_FED_WRITES: ReadonlySet<string> = new Set([
  'placeOrder',
  'placeOrders',
  'placeOrderWithClientOrderId',
  'updateOrder',
  'cancelOrders',
  'cancelSymbolOrders',
  'cancelAllOrders',
  'nuke',
  'nukePositions',
  'nukeOrders',
  'setLeverage',
  'setAllLeverage',
  'changeMarginType',
  'changePositionMode',
  'changeStpMode',
  'changeAccountTier',
  'scheduleCancel',
]);

/**
 * The hosted server answers `fetchOrders`/`fetchPositions` with its engine's
 * snapshot frame, so the rows come from the same FrameApplier as the worker's,
 * native-unit extras and NaN handling included. A worker's rows pass through.
 */
const rowsFromAccountFrame = (method: string, result: unknown, symbols: unknown): unknown => {
  const frame = (result as { accountFrame?: unknown } | null | undefined)?.accountFrame;
  if ((method !== 'fetchOrders' && method !== 'fetchPositions') || typeof frame !== 'string') return result;
  const bytes = Uint8Array.from(atob(frame), (char) => char.charCodeAt(0));
  const store = new DefaultStore();
  new FrameApplier(store).apply(new Frame(bytes.buffer));
  const rows: ReadonlyArray<{ symbol: string }> = method === 'fetchOrders' ? store.orders : store.positions;
  return Array.isArray(symbols) ? rows.filter((row) => symbols.includes(row.symbol)) : rows;
};
type NativeAccountRead = (typeof NATIVE_ACCOUNT_READS)[number];
const MULTI_COMMAND_WAIT_FACTOR = 5;
/** Engine ops safe to delay until the engine exists; anything unlisted is treated as a write. */
const COLD_START_READS = /^(fetch|find|refresh)/;
/** How long a write may queue for startup; it must not outlive its caller's wait. */
const STARTUP_WRITE_WAIT_MS = 30_000;
/** How long a worker read may wait while the account loads, matching the write refusal. */
const STARTUP_READ_WAIT_MS = 30_000;
const WORKER_CAPABILITY_RETRY_MS = 30_000;
const WORKER_BRIDGE_WAIT_MS = 30_000;
const WORKER_BRIDGE_POLL_MS = 25;
/** A page candle read stays well under the chart's 60 s hung-history verdict, end to end. */
const PAGE_CANDLE_MARKETS_WAIT_MS = 10_000;
/** A source still without markets this long after construction is stuck, not slow: rebuild it. */
const PAGE_CANDLE_MARKETS_STUCK_MS = 60_000;
const PAGE_CANDLE_COMMAND_TIMEOUT_MS = 20_000;
/** Rebuilding a broken page candle source backs off from 2 s, doubling to a minute. */
const PAGE_CANDLE_REBUILD_MIN_MS = 2_000;
const PAGE_CANDLE_REBUILD_MAX_MS = 60_000;
const LOADED_SECTIONS = [
  [LOADED_FLAG.MARKETS, 'markets'],
  [LOADED_FLAG.TICKERS, 'tickers'],
  [LOADED_FLAG.ORDERS, 'orders'],
  [LOADED_FLAG.POSITIONS, 'positions'],
  [LOADED_FLAG.BALANCE, 'balance'],
] as const;

/** A source answers an orders ask on dispatch; its orders come in a later frame, within the refresh deadline. */
const SOURCE_ORDERS_FRAME_DEADLINE_MS = 30_000;

const sourceFrameHasOrders = (buffer: ArrayBuffer): boolean => {
  try {
    return Boolean(new Frame(buffer).orders());
  } catch {
    return false;
  }
};

/** Frames whose clock samples estimate the data source's offset, and the change worth resetting it for. */
const SOURCE_CLOCK_SAMPLES = 20;
const SOURCE_CLOCK_STEP_MS = 25;

/** A nuke waits this long for stopped Rust chases to settle their cancels (the engine's bound). */
const NUKE_ALGO_STOP_MS = 10_000;
/** Past the engine's bound, a host that stopped advancing no longer holds the nuke. */
const NUKE_ALGO_STOP_GRACE_MS = 1_000;
/** Algo ops held while the engine host starts; past this they are refused. */
const MAX_QUEUED_ALGO_OPS = 100;

type AlgoRows = { activeChasers: ActiveChaser[]; activeTWAPs: ActiveTWAP[] };

/** Store sections flush at the worker's store cadence. */
const STORE_FLUSH_MS = 250;
const SOCKET_HEALTH_SYNC_MS = 5_000;

/**
 * An exchange account run by the Rust engine. Venue I/O, parsing and state
 * live in wasm; each flush frame updates this exchange's store through
 * `FrameApplier`, so the worker shell (chasers, TWAP, CLI, post-processors)
 * reads the same store it always has. Trading methods are engine commands.
 */
const VALIDATE_ACCOUNT_TIMEOUT_MS = 45_000;

export class RustEngineExchange extends BaseExchange {
  private workerOwnerSource?: () => { ownerId: string; generation: number } | undefined;

  /** Optional owner fencing; hosts without a binding retain legacy bridge dispatch behavior. */
  setWorkerOwner = (source: () => { ownerId: string; generation: number } | undefined): void => {
    if (typeof source !== 'function') throw new TypeError('Worker owner source must be a function');
    this.workerOwnerSource = source;
  };

  protected readonly clientOrderIdRule = clientOrderIdRules[this.name];
  private readonly defaultHistoricalPositions = (this as BaseExchange).fetchHistoricalPositions;
  private host?: EngineHostType;
  private readReconnectDiagnostics?: ReturnType<typeof createEngineTransport>['reconnectDiagnostics'];
  private engineNow = (): number => virtualClock.getCurrentTimeMs();
  private utcNow = (): number => virtualClock.getCurrentTimeMs();
  private engineFailure?: Error;
  private publicFeedDisabled = false;
  /** Why a start that never finishes has not: a venue answer is not a slow connection. */
  lastReadFailure?: string;
  lastFailedPrivateReadKind?: FailedPrivateReadKind;
  private nativeStartupTags: Record<string, string> = {};
  private nativeStartupSampleAt?: number;
  private httpStartupDiagnostics?: () => Record<string, string>;
  private invalidateInfoCatalogue?: () => void;
  get startupDiagnostics(): Record<string, string> {
    return {
      ...this.nativeStartupTags,
      ...(this.nativeStartupSampleAt === undefined
        ? {}
        : { startup_native_sample_age_ms: String(Math.max(0, Date.now() - this.nativeStartupSampleAt)) }),
      ...(this.lastFailedPrivateReadKind
        ? { startup_last_failed_private_read_kind: this.lastFailedPrivateReadKind }
        : {}),
      ...this.httpStartupDiagnostics?.(),
    };
  }

  /** A native trap is terminal for this instance, including after disposal. */
  get isEngineDead(): boolean {
    return this.engineFailure !== undefined;
  }

  get publicFeedMode(): 'direct' | 'disabled' | 'shared' {
    if (this.options.publicFeed?.mode === 'shared' || this.usesInProcessPublicData) return 'shared';
    // exchange-server keys its shared source by venue only, so testnet runs its own feed.
    if (this.options.externalPublicData?.type === 'in-process' && this.options.testnet) return 'direct';
    return this.publicFeedDisabled || this.isPrivateOnly ? 'disabled' : (this.options.publicFeed?.mode ?? 'direct');
  }

  private get usesInProcessPublicData() {
    return this.options.externalPublicData?.type === 'in-process' && !this.options.testnet;
  }

  override disablePublicWebsockets(): void {
    if (this.hasStarted) throw new Error('Disable Rust public sockets before startup');
    this.publicFeedDisabled = true;
  }
  private sharedPublicFeed?: RustPublicFeedClient;
  override signHttpRequest = async (request: RemoteSignerRequest): Promise<RemoteSignerResponse> => {
    if (this.bridgeConstruction.dispatchSigning) {
      try {
        await this.ensureSigningHost();
        return await signHostedHttp(
          this.name,
          this.engineOptions,
          Boolean(this.signerSetup?.primaryComplete),
          this.host!,
          request,
        );
      } catch {
        return { id: request.id, error: 'Hosted HTTP signing failed' };
      }
    }
    return { id: request.id, error: 'Hosted HTTP signing is unsupported in this build' };
  };

  override signWsAuth = async (request: RemoteSignerRequest): Promise<RemoteSignerResponse> => {
    if (this.bridgeConstruction.dispatchSigning) {
      try {
        await this.ensureSigningHost();
        return await signHostedWsAuth(this.host!, request);
      } catch {
        return { id: request.id, error: 'Hosted WS auth signing failed' };
      }
    }
    return { id: request.id, error: 'Hosted WS auth signing is unsupported in this build' };
  };
  private stopInProcessFeed?: () => void;
  private requestSharedSnapshot = false;
  private sharedNativeSymbols = new Set<string>();
  private sharedValidatedMarkets?: Store['markets'];
  private sharedPublicReady = false;
  private nativeLoaded = 0;
  /** A mirror's credential-free source for chart history; its reads are bounded. */
  private pageCandleSource = false;
  private readonly commandTimeoutMs: number;
  private readonly bridgeConstruction: EngineBridgeConstruction;
  private publicDemandReady = false;
  private readonly publicFeedObservers = new Set<{ updated(catalogChanged: boolean): void; closed(): void }>();

  /** Public providers observe applied native ticker frames, without replacing onFrame. */
  observePublicFeed(observer: { updated(catalogChanged: boolean): void; closed(): void }): () => void {
    this.publicFeedObservers.add(observer);
    return () => {
      this.publicFeedObservers.delete(observer);
    };
  }
  private hostInitialization?: Promise<void>;
  private startup?: Promise<void>;
  private stopTimeSync?: () => void;
  private startupState: 'idle' | 'pending' | 'ready' | 'failed' = 'idle';
  private startupFailure?: Error;
  private readonly attemptedPlacementSymbols = new Set<string>();
  private readonly startupWaiters = new Set<() => void>();
  private flushTimers: Array<ReturnType<typeof setInterval>> = [];
  private readonly applier: FrameApplier;
  private readonly bookListeners = new Map<string, Map<number, (book: OrderBook) => void>>();
  private nextBookListenerId = 0;
  private readonly candleListeners = new Map<string, Set<(candle: Candle) => void>>();
  private readonly feedBookListeners = new Set<(book: OrderBook) => void>();
  private readonly feedCandleListeners = new Set<(candle: Candle) => void>();

  /**
   * Receives each frame after this exchange has applied it. When the worker
   * forwards frames to a main thread that reads them, book and candle
   * listeners here stay silent: main decodes those from the frame itself.
   */
  /** `snapshot`: the frame is a full baseline, so a frame-fed consumer resets before applying it. */
  onFrame?: (frame: ArrayBuffer, snapshot?: boolean) => void;
  /**
   * Whether the `onFrame` consumer decodes books and candles from the frames
   * itself (a main thread applying worker frames), so listeners here stay quiet.
   * A frame-fed page engine ingests only account sections: forwarding to it
   * keeps books and candles flowing to this exchange's own listeners.
   */
  framesCarryMarketData = true;

  private get frameConsumerDecodesMarketData(): boolean {
    return Boolean(this.onFrame) && this.framesCarryMarketData;
  }

  /**
   * A main-thread mirror whose family supports it runs its own engine fed by the
   * account's data source (`ingestSourceFrame`): that engine's frames are the UI
   * store, and it signs and sends manual writes itself. Others stay plain mirrors.
   */
  readonly frameFedReady: Promise<boolean>;
  private frameFed = false;
  private frameFedSettled = false;
  private frameFedEngine = false;
  private sourceBaseline = false;
  /** Recent (source write time - arrival) samples, and the offset last handed to the engine. */
  private sourceClockSamples: number[] = [];
  private sourceClockOffset?: number;
  get isFrameFed(): boolean {
    return this.frameFed;
  }
  private stopPollingConfig?: () => void;
  private pollingPolicy?: { scale: number | null; ceiling?: number; promise?: Promise<void> };
  private syncingAccountState = false;
  private warnIgnoredCoinbaseReadOptions = false;
  private readonly accountAuthStreaks = new Uint8Array(10);
  fetchFillsForOrderIds?: (ids: string[]) => Promise<Map<string, number>>;
  /** Lighter: switch the account tier; the engine re-reads limits afterwards. */
  changeAccountTier?: (tier: LighterAccountTier) => Promise<void>;
  /** Lighter: the engine's cached account tier, when known. */
  fetchAccountTier?: () => Promise<LighterAccountTier | undefined>;

  override loadCapabilities = async (): Promise<ExchangeCapabilities> => {
    // A main-thread mirror asks the worker's engine rather than building its own.
    if (this.isWorkerMirror || (this.routesToWorker && !this.host)) await this.loadWorkerCapabilities();
    else await this.ensureHost();
    return this.capabilities;
  };

  /** Worker side of `loadCapabilities` for a main-thread mirror. */
  fetchEngineCapabilities = async (): Promise<EngineCapabilities | undefined> => {
    if (this.isWorkerMirror) {
      await this.loadWorkerCapabilities();
      return this.capabilityRecord;
    }
    await this.ensureHost();
    return this.capabilityRecord;
  };

  private workerCapabilities?: Promise<void>;
  private readonly helperCapabilities: Readonly<EngineHelperCapabilities>;
  private workerCapabilityRetryAt = 0;
  private stopWorkerCapabilitySync?: () => void;

  private loadWorkerCapabilities = (): Promise<void> =>
    (this.workerCapabilities ??= this.callWorker('fetchEngineCapabilities').then(
      (record) => {
        if (record && !this.capabilityRecord && !this.isDisposed)
          this.adoptEngineCapabilities(record as EngineCapabilities);
      },
      (error: unknown) => {
        this.workerCapabilities = undefined;
        this.workerCapabilityRetryAt = Date.now() + WORKER_CAPABILITY_RETRY_MS;
        throw error;
      },
    ));

  // A mirror learns capability-defined methods (Lighter tiers, history axes) once the worker has loaded.
  private syncWorkerCapabilities = () => {
    if (this.capabilityRecord || this.host || this.workerCapabilities || this.isDisposed) return;
    if (!this.store.loaded.markets || !this.routesToWorker || Date.now() < this.workerCapabilityRetryAt) return;
    void this.loadWorkerCapabilities().catch(() => {
      /* Retried on a later store update; explicit loadCapabilities callers see the failure. */
    });
  };

  /** What the venue answers, as its engine reports it; unknown until the engine exists. */
  get engineCapabilities(): EngineCapabilities | undefined {
    return this.capabilityRecord;
  }
  private capabilityRecord?: EngineCapabilities;

  override get capabilities() {
    const answers = super.capabilities;
    if (!this.capabilityRecord?.options) {
      answers.optionChain = false;
      answers.optionExpiries = false;
      answers.accountGreeks = false;
    }
    return { ...this.capabilityRecord, ...answers };
  }

  /** Venue-specific answers wait for the engine rather than guessing from the base contract. */
  private requireCapabilities(method: string): EngineCapabilities {
    if (this.capabilityRecord) return this.capabilityRecord;
    return this.capabilitiesUnavailable(method);
  }

  private capabilitiesUnavailable(method: string, startupFailed = false): never {
    const message = startupFailed
      ? `${this.name}.${method}: exchange has not started because startup failed; reconnect before calling ${method}.`
      : `${this.name}.${method}: exchange has not started; call start() or loadCapabilities() first.`;
    const error =
      (!startupFailed && this.engineFailure) ||
      new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
    const cause = this.engineFailure ?? this.startupFailure;
    if (startupFailed && cause) Object.defineProperty(error, 'cause', { value: cause, configurable: true });
    this.reportCommandError(error);
    throw error;
  }

  /** Preserve immediate dispatch after readiness; only a pending startup queues user calls. */
  private capabilitiesFor(method: string): EngineCapabilities | Promise<EngineCapabilities> {
    if (this.startupState === 'pending' && this.startup) {
      return this.startup.then(
        () => this.requireCapabilities(method),
        () => this.capabilitiesUnavailable(method, true),
      );
    }
    if (this.startupState === 'failed') return this.capabilitiesUnavailable(method, true);
    return this.requireCapabilities(method);
  }

  /** `capabilitiesFor` for a write: queued past the bound it is refused and can never send. */
  private writeCapabilitiesFor(method: string): EngineCapabilities | Promise<EngineCapabilities> {
    const startup = this.startup;
    if (this.startupState !== 'pending' || !startup) return this.capabilitiesFor(method);
    return new Promise<EngineCapabilities>((resolve, reject) => {
      // Whichever runs first clears the other, so the refusal and the send are exclusive.
      const timer = setTimeout(() => {
        const message = `${this.name} ${method} was not sent: the account did not load within ${
          this.startupWriteWaitMs / 1000
        }s. Retry once it loads.`;
        const error = new ExchangeError(message, { message, kind: 'timeout', outcome: 'notSent' });
        settled = true;
        this.reportCommandError(error);
        reject(error);
      }, this.startupWriteWaitMs);
      let settled = false;
      const finish = (answer: () => EngineCapabilities) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        try {
          resolve(answer());
        } catch (error) {
          reject(error);
        }
      };
      startup.then(
        () => finish(() => this.requireCapabilities(method)),
        () => finish(() => this.capabilitiesUnavailable(method, true)),
      );
    });
  }

  private get startupWriteWaitMs(): number {
    return Math.min(STARTUP_WRITE_WAIT_MS, this.commandTimeoutMs);
  }

  // History axes the engine lacks take the branded base fallbacks, which
  // `exchange.capabilities` reports absent. A main-thread mirror adopts the
  // record from the worker's snapshot frame; the record never changes.
  protected override adoptEngineCapabilities(capabilities: EngineCapabilities) {
    if (this.capabilityRecord) return;
    this.capabilityRecord = capabilities;
    if (!capabilities.fillHistory)
      this.fetchHistoricalFills = markBaseCapabilityFallback('fetchHistoricalFills', async () => []);
    if (!capabilities.orderHistory)
      this.fetchHistoricalOrders = markBaseCapabilityFallback('fetchHistoricalOrders', async () => []);
    if (!capabilities.incomeHistory) {
      this.fetchIncomeHistory = markBaseCapabilityFallback('fetchIncomeHistory', async () => []);
      this.fetchIncomeHistoryRange = markBaseCapabilityFallback('fetchIncomeHistoryRange', async () => []);
    }
    // Fill lookups serve the worker's chasers; a mirror has no engine to ask.
    if (capabilities.fillsByOrderId && !this.isWorkerMirror) this.fetchFillsForOrderIds = this.fetchFilledQuantities;
    if (capabilities.accountTier) {
      this.changeAccountTier = async (tier: LighterAccountTier) => {
        if (this.routesToWorker && !this.frameFed) {
          await this.callWorker('changeAccountTier', tier);
          return;
        }
        await this.command<null>({ op: 'setAccountTier', tier });
        this.writeLighterTier(tier);
      };
      this.fetchAccountTier = async () => {
        if (this.routesToWorker) return (await this.callWorker('fetchAccountTier')) as LighterAccountTier | undefined;
        return (await this.command<LighterAccountTier | null>({ op: 'fetchAccountTier' })) ?? undefined;
      };
    }
    if (capabilities.closedPositions) this.fetchClosedPnl = this.fetchReportedClosedPnl;
  }

  private fetchFilledQuantities = async (ids: string[]) => {
    const quantities = new Map<string, number>();
    for (const id of new Set(ids)) {
      const fills = await this.command<Execution[]>(
        { op: 'fetchFills', orderId: id, from: 0, limit: 1000 },
        { report: false },
      );
      for (const fill of fills) {
        if (fill.orderId === id) quantities.set(id, (quantities.get(id) ?? 0) + fill.qty);
      }
    }
    return quantities;
  };

  constructor(exchangeName: ExchangeName, options: ExchangeOptions, store: Store) {
    assertBetaVenue(exchangeName);
    options = snapshotDelegatedSigningOptions(exchangeName, options);
    assertBetaAccount(exchangeName, options);
    const accountOptions = withoutDelegatedSigningSnapshot(options);
    const bridgeConstruction = deriveBridgeConstruction(exchangeName, options);
    // Coinbase drops its read egress with the key, as it always has (secondaryReadKeys.ts).
    if (
      !SECONDARY_READ_KEYS_ENABLED &&
      bridgeConstruction.primaryCredentialsOnly &&
      [options.secondary_key, options.secondary_secret, options.secondary_passphrase].some((v) => v !== undefined)
    ) {
      const { secondaryProxyUrl: _readProxy, ...primaryOnly } = options;
      options = primaryOnly;
    }
    options = withSecondaryReadKeyPolicy(options);
    const coinbaseReadOptions = bridgeConstruction.primaryCredentialsOnly
      ? [options.secondary_key, options.secondary_secret, options.secondary_passphrase]
      : [];
    // Empty saved fields are stripped too, but only an actual read key is worth a warning.
    const ignoredCoinbaseReadOptions = coinbaseReadOptions.some(Boolean);
    if (coinbaseReadOptions.some((value) => value !== undefined)) {
      // The native JWT signer uses primary credentials; its egress must use the same account policy.
      const {
        secondary_key: _readKey,
        secondary_secret: _readSecret,
        secondary_passphrase: _readPassphrase,
        secondaryProxyUrl: _readProxy,
        ...effectiveOptions
      } = options;
      options = effectiveOptions;
    }
    const lighterL1Address =
      deriveSignerSetupMapping(exchangeName) === 'lighter' && typeof options.key === 'string'
        ? options.key.match(/0x[0-9a-fA-F]{40}(?![0-9a-fA-F])/)?.[0]
        : undefined;
    if (options.localSigner !== undefined && !bridgeConstruction.dispatchSigning) {
      const message = 'Local signer setup is not supported for this Rust venue yet';
      throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
    }
    const delegatedPolicy = deriveRemoteSigningPolicy(exchangeName);
    const delegatedMode = selectDelegatedSigningMode(options);
    const delegated = delegatedPolicy?.enabled === true && delegatedMode !== undefined;
    // A setup refusal surfaces at start(), where callers route its message.
    let aster: ReturnType<typeof mapAsterDEXSignerSetup> | undefined;
    let setupError: ExchangeError | undefined;
    const agentMapping = deriveSignerSetupMapping(exchangeName);
    const agentWallet = agentMapping === 'hyperliquid' || agentMapping === 'txflow';
    const agentAccount = options.key;
    let agentSetup: ExchangeOptions | undefined;
    if (bridgeConstruction.dispatchSigning && !delegated && agentWallet) {
      try {
        agentSetup = mapHyperliquidSignerSetup(options, exchangeMetadata[agentMapping].prettyName);
      } catch (error) {
        if (!(error instanceof ExchangeError)) throw error;
        setupError = error;
      }
    }
    let paradexSetup: ExchangeOptions | undefined;
    if (bridgeConstruction.dispatchSigning && !delegated && deriveSignerSetupMapping(exchangeName) === 'paradex') {
      try {
        paradexSetup = mapParadexSignerSetup(options);
      } catch (error) {
        if (!(error instanceof ExchangeError)) throw error;
        setupError = error;
      }
    }
    if (bridgeConstruction.dispatchSigning && deriveSignerSetupMapping(exchangeName) === 'asterdex') {
      try {
        aster = mapAsterDEXSignerSetup(options);
      } catch (error) {
        if (!(error instanceof ExchangeError)) throw error;
        setupError = error;
      }
    }
    const signerSetup =
      bridgeConstruction.dispatchSigning && !setupError
        ? delegated
          ? new DelegatedSignerSetup(exchangeName,
              deriveSignerSetupMapping(exchangeName) === 'mexc' ? mapMEXCSignerSetup(options) : options)
          : aster
            ? new LocalSignerSetup(
                aster.setupOptions,
                deriveSignerSetupPolicy(exchangeName),
                aster.secondarySigningFromPrimary,
              )
            : deriveSignerSetupMapping(exchangeName) === 'apexomni'
              ? new LocalSignerSetup(
                  options,
                  deriveSignerSetupPolicy(exchangeName),
                  false,
                  options.omniKeySeed || undefined,
                )
              : new LocalSignerSetup(
                  agentSetup ??
                    paradexSetup ??
                    (deriveSignerSetupMapping(exchangeName) === 'mexc' ? mapMEXCSignerSetup(options) : options),
                  deriveSignerSetupPolicy(exchangeName),
                )
        : undefined;
    const signingMode = signerSetup
      ? derivePublicMode(
          exchangeName,
          options,
          undefined,
          agentWallet
            ? { key: Boolean(agentAccount), secret: signerSetup.credentialPresence.secret }
            : signerSetup.credentialPresence,
        )
      : undefined;
    if (
      signingMode &&
      !signingMode.publicDataOnly &&
      delegatedMode === 'payload' &&
      delegatedPolicy &&
      !delegatedPolicy.payloadSupported
    ) {
      signerSetup?.dispose();
      const message = 'This Rust account has no REST-only PayloadSigner lifecycle';
      throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
    }
    options = withoutDelegatedSigningSnapshot(options);
    if (signerSetup || setupError)
      options = delegated ? withoutDelegatedSigning(options) : withoutSigningCredentials(options);
    if (agentWallet && (signerSetup || setupError)) options = { ...options, key: agentAccount };
    const commandTimeoutMs = options.extra?.rustCommandTimeoutMs ?? DEFAULT_COMMAND_TIMEOUT_MS;
    if (!Number.isSafeInteger(commandTimeoutMs) || commandTimeoutMs <= 0 || commandTimeoutMs > 2_147_482_647) {
      signerSetup?.dispose();
      throw new Error('rustCommandTimeoutMs must be a positive integer no greater than 2147482647');
    }
    // The web forwards a mirror's options to the worker that owns the engine, so a
    // mirror keeps the account's own; its signing host uses the stripped copy.
    super(exchangeName, options.workerMirror === true ? accountOptions : options, store);
    this.engineOptions = options;
    this.bridgeConstruction = bridgeConstruction;
    this.signerSetup = signerSetup;
    this.asterPublicSetup = aster?.publicSetup;
    this.lighterL1Address = lighterL1Address;
    this.setupError = setupError;
    if (signerSetup instanceof DelegatedSignerSetup) {
      this.delegatedIntent = signerSetup.intent;
      this.delegatedUsesPayloadSigner = signerSetup.usesPayloadSigner;
    }
    if (signingMode) this.constructionRecord = { bridge: bridgeConstruction, ...signingMode };
    this.warnIgnoredCoinbaseReadOptions = ignoredCoinbaseReadOptions;
    this.commandTimeoutMs = commandTimeoutMs;
    // Handles satisfy the BaseWebSocket members callers read; venues own the sockets.
    const socket = (channel: RustSocketHandle['channel']) =>
      new RustSocketHandle(channel, this) as unknown as BaseWebSocket<any>;
    this.publicWebsocket = socket('public');
    this.businessWebsocket = socket('business');
    if (!this.isPublicDataOnly) this.privateWebsocket = socket('private');
    this.isSpot = bridgeConstruction.spot;
    this.isDemo = bridgeConstruction.demo;
    this.applier = new FrameApplier(store);
    this.helperCapabilities = bridgeConstruction.helpers ?? {};
    if (this.helperCapabilities.resolveLeverageContext === true)
      this.resolveLeverageContext = this.resolvePhemexLeverageContext;
    this.on('activeTickerSymbols:changed', this.updateTickerSymbols);
    this.stopPollingConfig = subscribeTickIntervalMultipliers(this.updatePollingPolicy);
    this.forwardToWorker();
    if (this.isWorkerMirror) this.watchAlgoControls();
    if (this.isWorkerMirror) this.refuseOnMirror();
    this.frameFedReady = this.isWorkerMirror && !this.isPublicDataOnly ? this.startFrameFed() : Promise.resolve(false);
    const frameFedSettled = () => {
      this.frameFedSettled = true;
    };
    this.frameFedReady.then(frameFedSettled, frameFedSettled);
    // Hosts that write through `callMethodInWorker` reach the frame-fed engine too.
    const viaWorker = this.callMethodInWorker;
    const members = this as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>;
    this.callMethodInWorker = ((method: string, ...args: unknown[]) =>
      this.frameFed && FRAME_FED_WRITES.has(method)
        ? members[method](...args)
        : method === 'fetchOHLCV'
          ? this.callCandleWorker(viaWorker, args)
          : (viaWorker as (m: string, ...a: unknown[]) => Promise<unknown>)(method, ...args)) as typeof viaWorker;
    this.stopWorkerCapabilitySync = this.store.subscribe(this.syncWorkerCapabilities);
  }

  private algoControlWatch?: AlgoControlWatch;

  /** The page clicks algo ops; only their account rows say they landed. */
  private watchAlgoControls() {
    const watch = new AlgoControlWatch({
      rows: () => this.store,
      subscribe: (listener) => this.store.subscribe(listener),
      resend: (op, data) => this.emitter.emit(op, data),
      tell: (message) => this.emitter.emit('error', message, { forceToast: true }),
    });
    this.algoControlWatch = watch;
    for (const op of ALGO_CONTROL_OPS) this.on(op, (data: unknown) => watch.observe(op, data));
    this.on('chaser:complete', (event?: ChaseEnd) => {
      if (event) watch.chaseCompleted(event);
    });
  }

  override fetchHistoricalPositions = async (opts: Parameters<BaseExchange['fetchHistoricalPositions']>[0]) => {
    if (this.routesToWorker && !this.host && !this.capabilityRecord) await this.loadWorkerCapabilities();
    const ready = this.capabilitiesFor('fetchHistoricalPositions');
    const capabilities = ready instanceof Promise ? await ready : ready;
    switch (capabilities.positionHistory) {
      case 'fillsAccount':
        return this.foldClosedPositionRecords(opts, { symbolScoped: false });
      case 'fillsSymbol':
        return this.foldClosedPositionRecords(opts, { symbolScoped: true });
      case 'bybit':
        return this.bybitJournal(opts);
      case 'okx':
        return this.okxJournal(opts);
      case 'blofin':
        return this.blofinJournal(opts);
      case 'wooOrders':
        return this.wooJournal(opts);
      case 'bitunix':
        return this.bitunixJournal(opts);
      case 'unsupported':
        return this.defaultHistoricalPositions(opts);
    }
  };

  private wooJournal = async (opts: Parameters<BaseExchange['fetchHistoricalPositions']>[0]) => {
    const to = opts.to ?? this.utcNow();
    const from = opts.from ?? to - 30 * 24 * 60 * 60 * 1000;
    const orders = await this.fetchHistoricalOrders({ from, to });
    const records = wooOrdersToPositionRecords(orders, {
      account: opts.account,
      from,
      resolveSettleCurrency: (symbol) => this.store.marketsBySymbol[symbol]?.quote || 'USDT',
    });
    return copyHistoryCompleteness(orders, records);
  };

  get isPublicDataOnly(): boolean {
    return (
      this.constructionRecord?.publicDataOnly ??
      derivePublicMode(this.name, this.options, this.constructionRecord?.asterdexApiVersion).publicDataOnly
    );
  }

  private constructionRecord?: EngineConstruction;
  /** Options with signing material removed: everything that builds an engine reads these. */
  private readonly engineOptions: ExchangeOptions;
  private signerSetup?: SignerSetup;
  private delegatedIntent?: Readonly<{ remoteSigner: boolean; payloadSigner: boolean }>;
  private delegatedUsesPayloadSigner?: boolean;
  /** Resolved by trusted setup before credentials were stripped. */
  private asterPublicSetup?: AsterDEXPublicSetup;
  private lighterL1Address?: string;
  private setupError?: ExchangeError;

  private get asterApiVersion() {
    // Construction/getters are synchronous; WASM loads asynchronously on first use.
    return (
      this.constructionRecord?.asterdexApiVersion ??
      this.asterPublicSetup?.apiVersion ??
      resolveAsterDEXApiVersion(this.options)
    );
  }

  protected override get canSign(): boolean {
    return (
      this.constructionRecord?.canSign ??
      derivePublicMode(this.name, this.options, this.constructionRecord?.asterdexApiVersion).canSign
    );
  }
  private updateTickerSymbols = () => {
    this.sharedPublicFeed?.send({ op: 'tickerSymbols', symbols: this.getActiveTickerSymbols() });
    this.subscriptionCommand({ op: 'setTickerSymbols', symbols: this.getActiveTickerSymbols() });
  };

  override recordAuthFailure(): void {
    this.receiveAccountAuth({ credential: 'primary', scope: 'account', outcome: 'rejected' });
  }

  private authFailureNoted = false;
  /** One engine dispatch often reports a rejection twice (`auth.rejected` and `deactivate`). */
  private noteAuthFailure = () => {
    if (this.authFailureNoted) return;
    this.authFailureNoted = true;
    queueMicrotask(() => {
      this.authFailureNoted = false;
    });
    this.recordAuthFailure();
  };

  override recordAuthSuccess(): void {
    this.receiveAccountAuth({ credential: 'primary', scope: 'account', outcome: 'accepted' });
  }

  private receiveAccountAuth = ({ credential, scope, outcome }: AccountAuthResult): void => {
    if (this.isDisposed || this.engineFailure) return;
    const index =
      (credential === 'secondary' ? 5 : 0) + ['account', 'balance', 'orders', 'positions', 'settings'].indexOf(scope);
    // Reuse Base's cap for each context, then expose their maximum to its policy.
    this.accountAuthStreaks[index] = this.setAuthFailureStreak(
      outcome === 'accepted' ? 0 : this.accountAuthStreaks[index] + 1,
    );
    this.setAuthFailureStreak(Math.max(...this.accountAuthStreaks));
    this.updatePollingPolicy();
  };

  private syncPollingPolicy = (): Promise<void> | undefined => {
    if (!this.host || this.isDisposed || this.engineFailure) return;
    const resolved = this.getTickInterval();
    if (resolved === undefined) return;
    // Venues keep their own reconcile cadences; the resolved tick (override,
    // server multiplier, auth backoff) scales them relative to the metadata tick.
    const reference = getExchangeMetadata(this._exchangeName).tickInterval;
    const scale = resolved === Number.POSITIVE_INFINITY ? null : reference ? resolved / reference : 1;
    // The resolved tick still bounds a slowdown (auth backoff and server multiplier
    // caps), unless a venue's native cadence is already slower.
    const ceiling = scale === null ? undefined : resolved;
    if (this.pollingPolicy?.scale === scale && this.pollingPolicy.ceiling === ceiling)
      return this.pollingPolicy.promise;
    // Install before dispatch: native actions can synchronously change policy again.
    const policy: NonNullable<RustEngineExchange['pollingPolicy']> = { scale, ceiling };
    this.pollingPolicy = policy;
    policy.promise = this.command<void>({ op: 'setPollScale', scale, ...(ceiling !== undefined && { ceiling }) }).catch(
      (error) => {
        if (this.pollingPolicy === policy) this.pollingPolicy = undefined;
        throw error;
      },
    );
    return policy.promise;
  };

  private updatePollingPolicy = (): void => {
    void this.syncPollingPolicy()?.catch(() => {
      /* command() owns error reporting; the next check can retry. */
    });
  };

  /** Startup is single-use; metadata factories can replace an already-started instance. */
  get hasStarted(): boolean {
    return this.startup !== undefined;
  }

  private startFrameFed = async (): Promise<boolean> => {
    try {
      // Asked before any engine exists: a family without it builds nothing on main.
      const wasm = await loadWasmEngine(this.name);
      if (!frameFedSupported(wasm, this.name) || this.isDisposed) return false;
      await this.ensureSigningHost();
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      this.log(`${this.name} manual writes stay on the account's data source: ${reason}`, LogSeverity.Warning);
      return false;
    }
    const host = this.host;
    const construction = this.constructionRecord as { frameFed?: boolean } | undefined;
    // Never start an engine that would open the venue's own streams on main.
    if (!host || this.isDisposed || !this.frameFedEngine || typeof host.applyFrame !== 'function') return false;
    if (construction?.frameFed !== true) return false;
    if (this.signerSetup instanceof DelegatedSignerSetup) await host.prepareAddressSetup();
    if (this.isDisposed || this.host !== host) return false;
    this.frameFed = true;
    host.start();
    const multiplier = this.options.tuning?.dataUpdateThrottleMultiplier || 1;
    this.flushTimers.push(setInterval(() => this.host?.flush(), STORE_FLUSH_MS * multiplier));
    return true;
  };

  /**
   * One frame from the account's data source (its worker, or the hosted server),
   * as its exact SCXF bytes. A snapshot resets the engine's account state; a delta
   * before the first snapshot is dropped, so a host asks the source for one once
   * `frameFedReady` resolves, and again after any gap or reconnect.
   */
  ingestSourceFrame = (frame: ArrayBuffer, snapshot: boolean): void => {
    const host = this.host;
    if (!this.frameFed || !host || this.isDisposed || this.engineFailure) return;
    if (!snapshot && !this.sourceBaseline) return;
    this.noteSourceClock(host, frame);
    try {
      host.applyFrame(frame, snapshot);
    } catch (error) {
      // A refused frame leaves no baseline to apply deltas to: drop them until a fresh snapshot.
      this.sourceBaseline = false;
      diagnosticConsole.error(`${this.name} source frame refused`, error);
      if (!this.engineFailure) this.emitter.emit('frameFed:needsSnapshot');
      return;
    }
    this.sourceBaseline = true;
    try {
      this.applier.applyMessageHandlerStats(new Frame(frame));
    } catch (error) {
      diagnosticConsole.error(`${this.name} source subscription stats refused`, error);
    }
    host.flush();
    if (this.sourceOrdersAwaited !== undefined && sourceFrameHasOrders(frame)) this.settleSourceOrders();
  };

  /**
   * A frame-fed engine confirmed a settings write: its data source re-reads the
   * settings, so the hold the engine keeps over the source's frames can clear.
   */
  private refreshSourceSettings = (symbol: unknown) => {
    void this.refreshAccountSettings(typeof symbol === 'string' ? symbol : undefined).catch((error: unknown) => {
      const reason = error instanceof Error ? error.message : String(error);
      this.log(`${this.name} could not ask its data source to re-read settings: ${reason}`, LogSeverity.Warning);
    });
  };

  private sourceOrdersAsk: { symbols: string[] | undefined; due: number } | undefined;
  private sourceOrdersTimer: ReturnType<typeof setTimeout> | undefined;
  private sourceCancelChecks: { id: string; symbol: string }[] = [];

  /**
   * A frame-fed engine holds an uncertain write until its data source sends orders from a read
   * sent after it. The source never saw that write, so the page asks once `liftAfterMs` has
   * passed. Each event names every standing symbol (`''` is the account), timed for the oldest:
   * the latest names replace the ask, and the earliest due time stands.
   */
  private askSourceForOrders = (fields: Record<string, unknown> | undefined) => {
    const liftAfterMs = fields?.liftAfterMs;
    const lift =
      typeof liftAfterMs === 'number' && Number.isFinite(liftAfterMs) && liftAfterMs >= 0
        ? liftAfterMs
        : UNSYNCED_FENCE_GRACE_MS;
    const named = typeof fields?.symbols === 'string' ? fields.symbols.split(',') : [''];
    const symbols = named.includes('') ? undefined : named;
    const due = Math.min(this.sourceOrdersAsk?.due ?? Infinity, Date.now() + lift + SOURCE_ORDERS_SLACK_MS);
    this.sourceOrdersAsk = { symbols, due };
    clearTimeout(this.sourceOrdersTimer);
    this.sourceOrdersTimer = setTimeout(this.sendSourceOrdersAsk, Math.max(0, due - Date.now()));
  };

  private sendSourceOrdersAsk = () => {
    const ask = this.sourceOrdersAsk;
    this.sourceOrdersAsk = undefined;
    this.sourceOrdersTimer = undefined;
    if (!ask || this.isDisposed || this.engineFailure) return;
    this.refreshSourceOrders(ask.symbols);
  };

  /** Source order reads asked for and not yet answered: the held writes they settle are still open. */
  private sourceOrderReads = 0;
  /** An answered ask whose orders frame has not landed yet; the deadline stops a lost frame holding forever. */
  private sourceOrdersAwaited: ReturnType<typeof setTimeout> | undefined;

  private refreshSourceOrders = (symbols?: string[]) => {
    if (this.isDisposed || this.engineFailure) return;
    const checks = this.sourceCancelChecks;
    this.sourceCancelChecks = [];
    this.sourceOrderReads += 1;
    void this.refreshAccountOrders(symbols, checks.length ? checks : undefined)
      .then(
        () => {
          if (this.isDisposed) return;
          clearTimeout(this.sourceOrdersAwaited);
          this.sourceOrdersAwaited = setTimeout(this.settleSourceOrders, SOURCE_ORDERS_FRAME_DEADLINE_MS);
        },
        (error: unknown) => {
          if (this.isDisposed) return;
          const reason = error instanceof Error ? error.message : String(error);
          this.log(`${this.name} could not ask its data source for orders: ${reason}`, LogSeverity.Warning);
        },
      )
      .finally(() => {
        this.sourceOrderReads -= 1;
      });
  };

  private settleSourceOrders = () => {
    clearTimeout(this.sourceOrdersAwaited);
    this.sourceOrdersAwaited = undefined;
  };

  /** Data source: send orders, for these symbols or the account, from a read sent after this call. */
  refreshAccountOrders = async (symbols?: string[], cancelChecks?: { id: string; symbol: string }[]): Promise<void> => {
    if (this.routesToWorker) {
      if (!this.workerOwnerSource) {
        // Preserve the legacy refresh's disposal no-op as well as its bridge wait.
        await this.untilWorkerBridge();
        if (this.isDisposed) return;
        await (this.callMethodInWorker as (m: string, ...a: unknown[]) => Promise<unknown>)(
          'refreshAccountOrders',
          ...(cancelChecks?.length ? [symbols, cancelChecks] : symbols !== undefined ? [symbols] : []),
        );
        return;
      }
      try {
        await this.callWorker('refreshAccountOrders',
          ...(cancelChecks?.length ? [symbols, cancelChecks] : symbols !== undefined ? [symbols] : []));
      } catch (error) {
        if (!this.isDisposed) throw error;
      }
      return;
    }
    try {
      if (cancelChecks?.length) {
        await this.command({ op: 'registerCancelChecks', orders: cancelChecks }, { report: false });
      }
    } finally {
      // Registration grants no read authority; a refusal must not suppress the repair read.
      await this.command({ op: 'refreshOrders', ...(symbols !== undefined ? { symbols } : {}) }, { report: false });
    }
  };

  /** Data source: re-read the settings a frame-fed engine changed, for one symbol or the account. */
  refreshAccountSettings = async (symbol?: string): Promise<void> => {
    if (this.routesToWorker) {
      await this.callWorker('refreshAccountSettings', ...(symbol !== undefined ? [symbol] : []));
      return;
    }
    await this.command({ op: 'refreshSettings', ...(symbol !== undefined ? { symbol } : {}) }, { report: false });
  };

  /**
   * The source's clock minus this engine's, from each frame's flush time (header
   * bytes 8-15): transit only delays a frame, so the largest recent sample is the
   * closest estimate. A LOCAL worker shares this page's clock and measures ~0;
   * a hosted server measures its real offset.
   */
  private noteSourceClock = (host: EngineHost, frame: ArrayBuffer) => {
    if (frame.byteLength < 16) return;
    const written = new DataView(frame).getFloat64(8, true);
    if (!Number.isFinite(written) || written <= 0) return;
    const samples = this.sourceClockSamples;
    samples.push(written - this.engineNow());
    if (samples.length > SOURCE_CLOCK_SAMPLES) samples.shift();
    const offset = Math.round(Math.max(...samples));
    if (this.sourceClockOffset !== undefined && Math.abs(offset - this.sourceClockOffset) < SOURCE_CLOCK_STEP_MS)
      return;
    this.sourceClockOffset = offset;
    host.setSourceClockOffset(offset);
  };

  /** The account's running chasers and TWAPs, stopped before a nuke run elsewhere: the
   * worker's run in its Rust algos; an in-process CLI's run in the TS algos. */
  stopAccountAlgos = async (): Promise<AlgoStopWithPlacements> => {
    const stopped: AlgoStopWithPlacements = this.runsRustAlgos
      ? await this.stopRustAlgos()
      : await stopLocalAccountAlgos(this);
    return { ...stopped, placedSymbols: [...(stopped.placedSymbols ?? []), ...this.attemptedPlacementSymbols] };
  };

  /** Whether this account's engine runs algos, once its engine host exists to say. */
  private algoRoute: 'rust' | 'unsupported' | undefined;
  /** Algo ops that arrived before the engine host, replayed in order once it decides. */
  private queuedAlgoOps: {
    op: string;
    rust: () => void;
    refuse: (message: string) => void;
    owner: TwapOwner | null | undefined;
  }[] = [];
  private twapOwnerProvider?: () => TwapOwner | null;
  private twapOwnerBinding: TwapOwner | null | undefined;

  setTwapOwnerProvider(getOwner?: () => TwapOwner | null, owner = captureTwapOwner(getOwner)) {
    this.twapOwnerProvider = getOwner;
    this.twapOwnerBinding = owner && { ...owner };
  }

  private algoOwnerCurrent = (owner: TwapOwner | null | undefined): boolean => {
    const current = captureTwapOwner(this.twapOwnerProvider);
    if (owner === undefined || owner === null) return current === owner;
    return !!current && current.ownerId === owner.ownerId && current.generation === owner.generation;
  };

  /** Chaser, TWAP and exec ops run in this account's engine (an `algos` build). */
  get runsRustAlgos(): boolean {
    return this.host?.algosRunning === true;
  }

  /** Start the engine's algos under the worker's account name; false on a build
   * without them, or before the engine host exists (nothing is decided then). */
  startAlgos = (accountName: string): boolean => {
    const host = this.host;
    if (!host || this.isDisposed || this.engineFailure) return false;
    if (this.algoRoute) return this.algoRoute === 'rust' && this.runsRustAlgos;
    try {
      const owner = this.twapOwnerBinding;
      host.algoStart(accountName, owner, owner === undefined ? undefined : () => this.algoOwnerCurrent(owner));
      this.algoRoute = 'rust';
    } catch (error) {
      if (!(error instanceof ExchangeError && error.kind === 'unsupported')) throw error;
      this.algoRoute = 'unsupported';
    }
    this.replayQueuedAlgoOps();
    return this.algoRoute === 'rust';
  };

  /**
   * Route one worker algo op to the engine's Rust algos: `rust` runs it there,
   * now or, before the engine host exists, once it starts (bounded, in arrival
   * order). An op that cannot run there reaches `refuse` with the reason.
   */
  routeAlgoOp = (accountName: string, op: string, rust: () => void, refuse: (message: string) => void): void => {
    if (!this.algoRoute) {
      try {
        this.startAlgos(accountName);
      } catch (error) {
        refuse(`${op} was not run: ${error instanceof Error ? error.message : String(error)}`);
        return;
      }
    }
    if (this.algoRoute === 'rust') {
      if (this.runsRustAlgos) rust();
      else refuse(`${op} was not run: the ${this.name} engine stopped before it could run algos`);
      return;
    }
    if (this.algoRoute === 'unsupported') {
      refuse(this.noAlgosMessage(op));
      return;
    }
    if (this.isDisposed || this.engineFailure) {
      refuse(`${op} was not run: the ${this.name} engine stopped before it could run algos`);
      return;
    }
    if (this.queuedAlgoOps.length >= MAX_QUEUED_ALGO_OPS) {
      refuse(`${op} was not run: too many algo requests are waiting for the ${this.name} engine to start`);
      return;
    }
    this.queuedAlgoOps.push({ op, rust, refuse, owner: captureTwapOwner(this.twapOwnerProvider) });
  };

  private noAlgosMessage = (op: string) => `${op} was not run: this ${this.name} engine build has no algos`;

  private replayQueuedAlgoOps() {
    const queued = this.queuedAlgoOps.splice(0);
    for (const { op, rust, refuse, owner } of queued) {
      try {
        if (!this.algoOwnerCurrent(owner)) refuse(`${op} cancelled-for-owner-change`);
        else if (this.algoRoute === 'rust' && this.runsRustAlgos) rust();
        else if (this.algoRoute === 'unsupported') refuse(this.noAlgosMessage(op));
        else refuse(`${op} was not run: the ${this.name} engine never started`);
      } catch (error) {
        refuse(`${op} failed: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
  }

  /** One worker algo op; a refusal reaches the user as an `error` event. */
  algoOp = (op: string, data: unknown): void => {
    try {
      if (!this.host) throw new Error(`${this.name} engine is not started`);
      if (!this.runsRustAlgos) throw new Error(`${this.name} algos are not running`);
      this.host.algoOp(op, data);
    } catch (error) {
      this.emitter.emit('error', `${op} failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  };

  /** Rust algo replies, under the worker's algo op names. `store` carries the rows
   * frames do not; the worker's TWAP handler persists `twap:stateUpdate`. */
  private receiveAlgoEmit = (op: string, data: unknown) => {
    if (op === 'store') {
      if (!this.isDisposed) this.store.update(data as Partial<AlgoRows>);
      return;
    }
    // Nobody awaits a running algo, so its error is its only outcome: it must beat the
    // gates that drop an unforced error on a stream that looks down, as a dead key's does.
    if (op === 'error') this.emitter.emit(op, data, { forceToast: true });
    else this.emitter.emit(op, data);
  };

  /** As `stopAccountAlgos.ts`, in the engine: TWAPs first, then every chase, waiting
   * (bounded) for their cancels. */
  private stopRustAlgos = async (): Promise<AlgoStopWithPlacements> => {
    const host = this.host!;
    let backstop: ReturnType<typeof setTimeout> | undefined;
    const unanswered = new Promise<AlgoStopAllResult>((resolve) => {
      backstop = setTimeout(
        () => resolve({ stoppedChasers: [], stoppedTwaps: [], settled: false }),
        NUKE_ALGO_STOP_MS + NUKE_ALGO_STOP_GRACE_MS,
      );
    });
    let result: AlgoStopAllResult;
    try {
      result = await Promise.race([host.algoStopAll('Stopped by nuke'), unanswered]);
    } finally {
      clearTimeout(backstop);
    }
    if (!result.settled)
      this.log('Stopped chases did not confirm their cancels before the nuke; orders may rest', LogSeverity.Warning);
    return {
      stoppedChasers: result.stoppedChasers,
      stoppedTwaps: result.stoppedTwaps,
      placedSymbols: result.placedSymbols ?? [],
    };
  };

  // Startup is single-use, including failures; a retry needs a new account instance.
  start = (): Promise<void> => {
    if (this.isDisposed) return Promise.reject(new Error('Exchange disposed during startup'));
    // The worker starts the account; the mirror's store fills from its frames.
    if (this.isWorkerMirror) return Promise.resolve();
    if (!this.startup) {
      this.startupState = 'pending';
      this.startup = this.startEngine().then(
        () => {
          this.startupState = 'ready';
        },
        (error: unknown) => {
          this.startupState = 'failed';
          throw error;
        },
      );
    }
    return this.startup;
  };

  private ensureHost = (): Promise<void> => {
    if (this.isWorkerMirror) return Promise.reject(new Error(`${this.name} worker mirror never runs an engine`));
    return this.ensureSigningHost();
  };

  /** Hosted signing builds the engine without starting it; a mirror still drops its frames. */
  private ensureSigningHost = (): Promise<void> => {
    if (this.isDisposed) return Promise.reject(new Error('Exchange disposed during startup'));
    return (this.hostInitialization ??= this.createHost().catch((error: unknown) => {
      this.signerSetup?.dispose();
      throw error;
    }));
  };

  private createHost = async () => {
    const options = this.engineOptions;
    if (this.setupError) throw this.setupError;
    if (deriveEndpointSetupPolicy(this.name) === 'hyrotrader') {
      const simulator = options.extra?.hyrotraderMode === 'demo';
      const network = options.testnet ? 'testnet' : 'livenet';
      const rest = simulator ? HYROTRADER_SIMULATOR_URL : HYROTRADER_PROD_URL;
      const sockets = simulator ? HYROTRADER_SIMULATOR_WS_URL : HYROTRADER_PROD_WS_URL;
      if (!rest[network] || !sockets.public.linear[network] || !sockets.private[network]) {
        const message = `HyroTrader ${simulator ? 'simulator' : 'production'} URLs are not configured in this build.`;
        const error = new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
        this.reportCommandError(error);
        throw error;
      }
    }
    if (this.warnIgnoredCoinbaseReadOptions) {
      this.warnIgnoredCoinbaseReadOptions = false;
      this.log(
        'Saved read credentials and their proxy are ignored; Coinbase private reads use primary credentials.',
        LogSeverity.Warning,
      );
    }
    if (options.externalPublicData?.type === 'websocket') {
      throw new Error(
        'Rust externalPublicData websocket gateways are unsupported; use publicFeed: { mode: "shared", port } or an in-process source',
      );
    }
    if (options.externalPublicData && options.publicFeed) {
      throw new Error('externalPublicData conflicts with publicFeed policy');
    }
    if (options.privateOnly && options.publicFeed?.mode === 'direct') {
      throw new Error('privateOnly conflicts with direct publicFeed policy');
    }
    assertEngineAccountRouting(options, this.name);
    if (!this.isPublicDataOnly && !this.signerSetup?.primaryComplete && isKeylessRemoteSigned(this.name, options)) {
      throw new ExchangeError('Delegated signing is unsupported in this build', {
        message: 'Delegated signing is unsupported in this build',
        kind: 'unsupported',
        outcome: 'notSent',
      });
    }
    const transportFactory = engineTransport;
    const wasm = await new Promise<WasmEngineModule>((resolve, reject) => {
      const disposed = () => {
        if (!this.isDisposed) return;
        this.startupWaiters.delete(disposed);
        reject(new Error('Exchange disposed during startup'));
      };
      this.startupWaiters.add(disposed);
      try {
        void loadWasmEngine(this.name)
          .then(async (module) => {
            if (this.isDisposed) throw new Error('Exchange disposed during startup');
            if (!transportFactory) await virtualClock.start();
            return module;
          })
          .then(resolve, reject)
          .finally(() => this.startupWaiters.delete(disposed));
      } catch (error) {
        this.startupWaiters.delete(disposed);
        reject(error);
      }
    });
    if (this.isDisposed) throw new Error('Exchange disposed during startup');
    const { WasmEngine } = wasm;
    // Decided with the build, so a signing host built first is the same frame-fed engine.
    this.frameFedEngine = this.isWorkerMirror && !this.isPublicDataOnly && frameFedSupported(wasm, this.name);
    const {
      key,
      secret,
      passphrase,
      omniKeySeed,
      paradexAddress,
      subkeyAddress,
      testnet,
      applicationId,
      vault,
      hyperliquidBuilder,
      enableSpotMarkets,
    } = options;
    const publicDataOnly = this.isPublicDataOnly;
    let pendingProtocolTime: { now: number; utc: number } | undefined;
    const routedTransport = createEngineTransport(
      this.name,
      options,
      transportFactory ? transportFactory() : synchronizedEngineTransport((now, utc) => {
        pendingProtocolTime = { now, utc };
      }),
      this.signerSetup,
      (message) => this.emitAndLogError(message),
      (notice) => {
        // Sentry and the log only: the account keeps working, degraded, so no toast.
        const { message, sentryData } = tunnelProxyFallbackReport(this.name, notice);
        this.emitter.emit('log', `[${this.name}] ${message}`, LogSeverity.Error, { sendToSentry: true, sentryData });
      },
      (notice) => {
        const { message, sentryData } = bybitPublicRateLimitReport(this.name, notice);
        this.emitter.emit('log', `[${this.name}] ${message}`, LogSeverity.Error, { sendToSentry: true, sentryData });
      },
      (event) => this.receiveEvent(event),
    );
    this.httpStartupDiagnostics = routedTransport.startupDiagnostics;
    this.invalidateInfoCatalogue = routedTransport.invalidateCatalogue;
    this.readReconnectDiagnostics = routedTransport.reconnectDiagnostics;
    const transport = withRequestsLog(
      routedTransport,
      this.name,
      (event, message) => this.emitter.emit(event, message),
      (event) => this.emitter.emit('ws-diagnostic', event),
    );
    const engine = constructEngine(
      WasmEngine,
      this.name,
      JSON.stringify({
        key,
        secret,
        passphrase,
        // A ValidateOnly family receives only the result, never the read key.
        ...(this.bridgeConstruction.readCredentials === 'validateOnly'
          ? (() => {
              const valid = readCredentialsValidity(options);
              return valid === undefined ? {} : { readCredentialsValid: valid };
            })()
          : options.secondary_key
            ? {
                readCredentials: {
                  key: options.secondary_key,
                  secret: options.secondary_secret,
                  passphrase: options.secondary_passphrase ?? passphrase,
                },
              }
            : {}),
        omniKeySeed,
        testnet: Boolean(testnet),
        bybitSiteId: options.bybitSiteId,
        brokerAttribution: process.env.NODE_ENV === 'production' || Boolean(options.tealstreetProxy),
        publicDataOnly: Boolean(publicDataOnly),
        publicMode: {
          requested: options.publicDataOnly ?? null,
          remoteSigner: this.delegatedIntent?.remoteSigner ?? Boolean(options.remoteSigner),
          payloadSigner: this.delegatedIntent?.payloadSigner ?? Boolean(options.payloadSigner),
          apiKeyIndexPresent: Boolean(options.apiKeyIndex),
        },
        remoteSigning: this.delegatedIntent !== undefined || Boolean(options.remoteSigner || options.payloadSigner),
        extra: { kucoinBroker: options.extra?.kucoinBroker },
        hyrotrader: {
          mode: options.extra?.hyrotraderMode,
          production: {
            rest: HYROTRADER_PROD_URL,
            publicWs: HYROTRADER_PROD_WS_URL.public.linear,
            privateWs: HYROTRADER_PROD_WS_URL.private,
          },
          simulator: {
            rest: HYROTRADER_SIMULATOR_URL,
            publicWs: HYROTRADER_SIMULATOR_WS_URL.public.linear,
            privateWs: HYROTRADER_SIMULATOR_WS_URL.private,
          },
        },
        recvWindow: options.extra?.recvWindow,
        applicationId,
        subaccount: options.subaccount,
        apiKeyIndex: parseLighterApiKeyIndex(options.apiKeyIndex)?.toString() ?? options.apiKeyIndex,
        lighterL1Address: this.lighterL1Address,
        lighterRemoteSigner: Boolean(options.remoteSigner),
        lighterSignerProvider: (options.extra as { lighterSignerProvider?: string } | undefined)?.lighterSignerProvider,
        lighterIntegratorAccountIndex: options.extra?.lighterIntegratorAccountIndex,
        lighterStopSlippagePct: options.extra?.lighterStopSlippagePct,
        lighterWsRoute: Boolean(options.tealstreetWsRoute),
        vault,
        hyperliquidBuilder,
        hyperliquidFastBook: options.hyperliquidFastBook === true,
        enableSpotMarkets: Boolean(enableSpotMarkets),
        enableInverseMarkets: Boolean(options.enableInverseMarkets && !options.externalPublicData),
        enableOptionMarkets: Boolean(options.enableOptionMarkets),
        grvtTradingAccountId: options.grvtTradingAccountId,
        grvtSignerAddress: options.grvtSignerAddress,
        polymarket: options.polymarket,
        paradexAddress,
        subkeyAddress,
        payloadSigner: this.delegatedUsesPayloadSigner ?? Boolean(options.payloadSigner),
        // Key material reaches the provider only through signer setup.
        asterdex: {
          apiVersion:
            this.asterPublicSetup?.apiVersion ??
            (['v1', 'v3'].includes(options.extra?.asterdexApiVersion ?? '')
              ? options.extra?.asterdexApiVersion
              : undefined),
          userAddress: options.auth?.userAddress,
          signerAddress: this.asterPublicSetup?.signerAddress ?? options.auth?.signerAddress,
          readSignerAddress: this.asterPublicSetup?.readSignerAddress,
          builderAddress: options.extra?.asterBuilderAddress,
          builderFeeRateMaker: options.extra?.asterBuilderFeeRateMaker,
          builderFeeRateTaker: options.extra?.asterBuilderFeeRateTaker,
        },
        ...transport.engineOptions,
        publicFeed: { mode: this.publicFeedMode },
        // The engine reports account sections missing at startup only when untilLoaded waits for them.
        accountStartup: !this.isPublicDataOnly,
        ...(this.frameFedEngine ? { frameFed: true } : {}),
        privateWsHeaders: Boolean(transport.webSocketHeaders),
      }),
      this.signerSetup,
    );
    // Construction transfers allocation ownership only when EngineHost succeeds.
    let host: EngineHost;
    try {
      if (engine.construction) this.constructionRecord = JSON.parse(engine.construction()) as EngineConstruction;
      host = new EngineHost(
        engine,
        transport,
        {
          onFrame: this.receiveFrame,
          onLog: this.receiveLog,
          onEvent: this.receiveEvent,
          onSocketChange: this.syncSocketHealthToStore,
          onDead: this.receiveEngineFailure,
          onCommandProgress: this.receiveCommandProgress,
          onAccountAuth: this.receiveAccountAuth,
          onAlgoEmit: this.receiveAlgoEmit,
          onDeactivate: () => {
            this.noteAuthFailure();
            this.startupFailure ??= new Error(`${this.name} credentials were rejected`);
            for (const wake of this.startupWaiters) wake();
            this.emitter.emit('deactivate', this.startupFailure.message);
          },
          onStartupFailed: (detail) => {
            // Keeps kind/venueCode so start() and validateAccount can branch on them.
            this.startupFailure ??= new ExchangeError(detail.message, detail);
            for (const wake of this.startupWaiters) wake();
          },
        },
        {
          commandTimeoutMs: this.commandTimeoutMs,
          ownsEngine: true,
          ...(!transportFactory ? { protocolTime: () => {
            // Sample only outside the HTTP handoff. now() itself never calls Rust.
            transport.now();
            const correction = pendingProtocolTime;
            pendingProtocolTime = undefined;
            return correction;
          } } : {}),
          ...(this.signerSetup instanceof DelegatedSignerSetup
            ? { signer: this.signerSetup.signer, signerAlgorithms: this.signerSetup.signerAlgorithms }
            : {}),
        },
      );
    } catch (error) {
      try {
        engine.free?.();
      } catch {
        /* Keep the original construction error. */
      }
      throw error;
    }
    try {
      const capabilities = host.capabilities();
      this.adoptEngineCapabilities(capabilities);
      if (!transportFactory) {
        host.syncHostTime(virtualClock.getCurrentTimeMs());
        this.stopTimeSync = virtualClock.subscribeToTimeSync((utc) => host.syncHostTime(utc));
      }
    } catch (error) {
      host.dispose();
      throw error;
    }
    this.engineNow = () => transport.now();
    if (transportFactory) this.utcNow = () => transport.now();
    this.host = host;
  };

  // Nuke and cancel-all run several engine commands in sequence on the worker.
  protected override workerCommandTimeoutMs = (method: CallableMethodsInWorker) =>
    Math.min(
      (MULTI_COMMAND_WORKER_METHODS.has(method) ? MULTI_COMMAND_WAIT_FACTOR : 1) * this.commandTimeoutMs + 1000,
      this.isWorkerReadDuringLoad(method) ? STARTUP_READ_WAIT_MS : 2_147_483_647,
    );

  private isWorkerReadDuringLoad(method: CallableMethodsInWorker) {
    if (WORKER_WRITE_METHODS.has(method) || MULTI_COMMAND_WORKER_METHODS.has(method)) return false;
    const { markets, balance, orders, positions } = this.store.loaded;
    return !markets || (!this.isPublicDataOnly && !(balance && orders && positions));
  }

  protected override workerCommandTimeoutError = (method: CallableMethodsInWorker, args: unknown[]): Error => {
    const clientOrderIds = workerCallClientOrderIds(method, args);
    const message = this.isWorkerReadDuringLoad(method)
      ? `${this.name} ${method} got no answer: the account has not finished loading. Retry once it loads.`
      : WORKER_WRITE_METHODS.has(method)
        ? 'Rust worker command timed out; reconcile interrupted writes before retrying'
        : 'Rust worker read timed out; retry after the account reconnects';
    return Object.assign(new Error(message), {
      code: 'COMMAND_TIMEOUT',
      command: method,
      ...(WORKER_WRITE_METHODS.has(method) ? { outcome: 'unknown' as const } : {}),
      ...(clientOrderIds ? { clientOrderIds } : {}),
    });
  };

  /**
   * The host's worker bridge listens for `callMethod` on a main-thread instance;
   * the worker's instance owns the engine and never has that listener. Trading
   * runs there whole, so preflight, placement scopes and nuke retries see the
   * authoritative state, even if a main-thread read started a local engine.
   */
  private get routesToWorker(): boolean {
    return this.isWorkerMirror || this.emitter.hasListeners('callMethod');
  }

  private get isWorkerMirror(): boolean {
    return this.options.workerMirror === true;
  }

  /** Calls a worker method by name, including Rust-only ones absent from the base `Exchange`. */
  private callWorker = (method: string, ...args: unknown[]): Promise<unknown> =>
    this.callWorkerForOwner(this.captureWorkerAdmission(), method, ...args);

  private workerOwnerChanged = (method: string, args: unknown[]): Error => {
    const message = `${this.name} ${method} was not sent: the account worker changed or its owner is not bound`;
    const clientOrderIds = workerCallClientOrderIds(method, args);
    return Object.assign(new ExchangeError(message, {
      message, kind: 'internal', ...(WORKER_WRITE_METHODS.has(method) ? { outcome: 'notSent' as const } : {}),
    }), { code: 'WORKER_STOPPED', command: method, ...(clientOrderIds ? { clientOrderIds } : {}) });
  };

  private captureWorkerAdmission = (): ((method: string, args: unknown[]) => void) | undefined => {
    const source = this.workerOwnerSource;
    if (!source) return undefined;
    const readOwner = () => {
      try { return source(); } catch { return undefined; }
    };
    const owner = readOwner();
    const ownerId = owner?.ownerId;
    const generation = owner?.generation;
    const bound = typeof ownerId === 'string' && ownerId.length > 0 && typeof generation === 'number' &&
      Number.isSafeInteger(generation) && generation >= 0;
    const bridgeIsCurrent = this.emitter.hasListeners('callMethod')
      ? this.emitter.captureListenerGuard('callMethod') : undefined;
    return (method, args) => {
      // This explicit public read alone may cross replacement bridges.
      if (method === 'fetchOHLCV') return;
      const current = readOwner();
      if (this.isDisposed || source !== this.workerOwnerSource || (bridgeIsCurrent && !bridgeIsCurrent()) ||
        !bound || current?.ownerId !== ownerId || current?.generation !== generation) {
        throw this.workerOwnerChanged(method, args);
      }
    };
  };

  private callWorkerForOwner = async (
    admitted: ((method: string, args: unknown[]) => void) | undefined, method: string, ...args: unknown[]
  ): Promise<unknown> => {
    const timing = TIMED_WRITE_METHODS.has(method) ? new WriteTiming(method) : undefined;
    timing?.mark('called');
    admitted?.(method, args);
    const bridgeIsCurrent = await this.untilWorkerBridge(admitted && (() => admitted(method, args)));
    admitted?.(method, args);
    if (admitted && (this.isDisposed || !bridgeIsCurrent())) {
      throw this.workerOwnerChanged(method, args);
    }
    const call = () => (this.callMethodInWorker as (m: string, ...a: unknown[]) => Promise<unknown>)(method, ...args);
    return timing ? runWithWriteTiming(timing, call) : call();
  };

  private callCandleWorker = async (call: typeof this.callMethodInWorker, args: unknown[]): Promise<unknown> => {
    const read = async () => {
      await this.untilWorkerBridge();
      return (call as (method: string, ...args: unknown[]) => Promise<unknown>)('fetchOHLCV', ...args);
    };
    try {
      return await read();
    } catch (error) {
      if (this.isDisposed || (error as { code?: string })?.code !== 'WORKER_STOPPED') throw error;
      // Only this idempotent public read may cross a replaced bridge, once.
      return read();
    }
  };

  /**
   * A placement clicked while the page engine loads. Past the bound it is refused,
   * and the rejection and the continuation that sends are exclusive: nothing goes
   * out after the user was told it failed.
   */
  private untilFrameFedSettled = (method: string): Promise<void> =>
    new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        const message = `${this.name} ${method} was not sent: the account did not load within ${
          this.startupWriteWaitMs / 1000
        }s. Retry once it loads.`;
        reject(new ExchangeError(message, { message, kind: 'timeout', outcome: 'notSent' }));
      }, this.startupWriteWaitMs);
      const settled = () => {
        clearTimeout(timer);
        resolve();
      };
      this.frameFedReady.then(settled, settled);
    });

  /** A mirror can be used before its host wires `callMethod`; calls wait for that bridge. */
  private untilWorkerBridge = async (assertCurrent?: () => void): Promise<() => boolean> => {
    const deadline = Date.now() + WORKER_BRIDGE_WAIT_MS;
    while (!this.emitter.hasListeners('callMethod')) {
      assertCurrent?.();
      if (this.isDisposed) throw new Error(`${this.name} disposed before its worker bridge was wired`);
      if (Date.now() >= deadline) throw new Error(`${this.name} worker bridge was not wired`);
      await new Promise((resolve) => setTimeout(resolve, WORKER_BRIDGE_POLL_MS));
    }
    return this.emitter.captureListenerGuard('callMethod');
  };

  private refuseOnMirror() {
    const members = this as unknown as Record<string, unknown>;
    for (const method of WORKER_MIRROR_UNSUPPORTED_METHODS) {
      members[method] = () => {
        const message = `${method} is not available on the main-thread exchange: it runs in the account's worker`;
        throw new ExchangeError(message, { message, kind: 'unsupported', outcome: 'notSent' });
      };
    }
  }

  private forwardToWorker() {
    // A branded fallback answers locally; wrapping it would hide the brand `capabilities` reads.
    // The brand alone decides: a mirror learns the record (options included) only later.
    const capabilities = super.capabilities;
    const inert = new Set<string>(
      Object.entries(CAPABILITY_METHODS)
        .filter(([capability]) => !capabilities[capability as keyof ExchangeCapabilities])
        .map(([, method]) => method),
    );
    // Native account reads start an engine, so a mirror asks the worker's instead.
    // Journals are not forwarded: a mirror folds them itself from forwarded raw reads,
    // so the hosted server, which has no TS mappers, only serves those reads.
    type Method =
      | (typeof WORKER_FORWARDED_METHODS)[number]
      | (typeof NATIVE_ACCOUNT_READS)[number]
      | (typeof RUST_FORWARDED_METHODS)[number]
      | 'fetchClosedPnl';
    const methods: Method[] = [
      ...WORKER_FORWARDED_METHODS,
      ...NATIVE_ACCOUNT_READS,
      ...RUST_FORWARDED_METHODS,
      'fetchClosedPnl',
    ];
    type Args = Pick<WorkerMethodArguments, Method>;
    const install = <M extends Method>(
      method: M,
      members: { [K in Method]: (...args: Args[K]) => Promise<unknown> },
    ) => {
      // A capability-defined method this venue lacks stays absent, never a forwarder.
      if (inert.has(method) || typeof members[method] !== 'function') return;
      const local = members[method];
      // An engine-owning instance calls straight through, keeping its own synchronous refusals.
      const route = (...args: Args[M]): Promise<unknown> => {
        // The explicit flag, not isPublicDataOnly: a keyed account's mirror holds no key and derives public.
        if (this.routesToWorker && this.options.publicDataOnly === true && PRIVATE_HISTORY_READS.has(method))
          return Promise.reject(notReadOnPublicData(method));
        if (method === 'fetchOHLCV' && this.routesToWorker && !getExchangeMetadata(this.name).fetchOHLCVInWorker)
          return Reflect.apply(this.fetchPageCandles, this, args);
        if (!this.routesToWorker || (this.frameFed && FRAME_FED_WRITES.has(method))) return local(...args);
        const admitted = this.captureWorkerAdmission();
        const viaWorker = () =>
          this.callWorkerForOwner(admitted, method, ...args).then(
            (result) =>
              // Array properties do not survive every bridge, so the worker sends the replacement ID beside the IDs.
              method === 'updateOrder' ? decodeWorkerOrderIds(result) : rowsFromAccountFrame(method, result, args[0]),
            (error: unknown) => {
              // Never passed through `command` here, and the hosted server marks no failure as
              // reported. A LOCAL worker's report arrives marked, so this says nothing twice.
              if (FRAME_FED_WRITES.has(method)) this.reportCommandError(error);
              throw error;
            },
          );
        if (!this.isWorkerMirror || !PAGE_ENGINE_PLACEMENTS.has(method) || this.frameFedSettled) return viaWorker();
        return this.untilFrameFedSettled(method).then(
          () => {
            admitted?.(method, args);
            return this.frameFed ? local(...args) : viaWorker();
          },
          (error: unknown) => {
            this.reportCommandError(error);
            throw error;
          },
        );
      };
      const forwarded = FRAME_FED_WRITES.has(method)
        ? (...args: Args[M]) => this.trackWrite(() => route(...args))
        : route;
      Object.assign(members, { [method]: forwarded });
    };
    for (const method of methods) install(method, this);
  }

  private writesInFlight = 0;

  private trackWrite = <T>(send: () => Promise<T>): Promise<T> => {
    this.writesInFlight += 1;
    let sent: Promise<T>;
    try {
      sent = Promise.resolve(send());
    } catch (error) {
      this.writesInFlight -= 1;
      throw error;
    }
    return sent.finally(() => {
      this.writesInFlight -= 1;
    });
  };

  /** A user write here has not answered, or an uncertain one still waits for its source's orders. */
  get hasUnsettledWrites(): boolean {
    return (
      this.writesInFlight > 0 ||
      this.sourceOrdersAsk !== undefined ||
      this.sourceCancelChecks.length > 0 ||
      this.sourceOrderReads > 0 ||
      this.sourceOrdersAwaited !== undefined
    );
  }

  /** The startup sections this engine applied from frames itself; carried or seeded rows never set these. */
  get appliedSections(): Record<(typeof LOADED_SECTIONS)[number][1], boolean> {
    const sections = {} as Record<(typeof LOADED_SECTIONS)[number][1], boolean>;
    for (const [flag, name] of LOADED_SECTIONS) sections[name] = (this.nativeLoaded & flag) === flag;
    return sections;
  }

  private receiveCommandProgress = (progress: CommandProgress) => {
    if (this.isDisposed || this.engineFailure) return;
    try {
      this.emitter.emit('command-progress', progress);
    } finally {
      // Existing main-thread log consumers can retain IDs selected after an RPC
      // timed out, even before they adopt the typed progress event.
      if (progress.late && progress.receipts.length)
        this.log(`Late order identity receipt ${progress.receiptKey}: ${JSON.stringify(progress.receipts)}`);
    }
  };

  private receiveEngineFailure = (error: Error) => {
    if (this.engineFailure) return;
    this.engineFailure = error;
    this.startupFailure = error;
    markErrorReported(error);
    for (const wake of this.startupWaiters) wake();
    try {
      this.emitAndLogError(error);
    } finally {
      try {
        this.emitter.emit('engine-dead', error);
      } finally {
        this.dispose();
      }
    }
  };

  /** exchange-server's in-process source speaks the shared-feed protocol over an in-memory port. */
  private openSharedFeedPort = ():
    { port: MessagePort; policy?: { handshakeTimeoutMs: number; inProcess: true } } | undefined => {
    if (this.options.publicFeed?.mode === 'shared') return { port: this.options.publicFeed.port };
    const external = this.options.externalPublicData;
    if (external?.type !== 'in-process' || !this.usesInProcessPublicData) return undefined;
    const [port, sourcePort] = inProcessPortPair();
    this.stopInProcessFeed = serveInProcessPublicData(external.source, sourcePort);
    return { port, policy: { handshakeTimeoutMs: IN_PROCESS_PUBLIC_FEED_TIMEOUT_MS, inProcess: true } };
  };

  private startEngine = async () => {
    await this.ensureHost();
    if (this.isDisposed || !this.host) throw new Error('Exchange disposed during startup');
    const sharedFeed = this.openSharedFeedPort();
    if (sharedFeed) {
      this.sharedPublicFeed = new RustPublicFeedClient(
        sharedFeed.port,
        rustPublicFeedIdentity(this.name, this.options),
        async (update) => {
          await this.host?.command(update);
          const wasReady = this.sharedPublicReady;
          if (
            this.requestSharedSnapshot &&
            update.tickers?.some(
              (ticker) =>
                this.store.marketsBySymbol[ticker.symbol] &&
                TICKER_FIELDS.some((field) => Number.isFinite(ticker[field])),
            )
          )
            this.sharedPublicReady = true;
          this.host?.flush();
          // An unchanged provider snapshot produces no frame or store notification.
          if (!wasReady && this.sharedPublicReady) for (const wake of this.startupWaiters) wake();
        },
        (error) => {
          this.startupFailure = error;
          for (const wake of this.startupWaiters) wake();
          this.emitAndLogError(error);
          markErrorReported(error);
          this.dispose();
        },
        { ...sharedFeed.policy, observations: new Frame(this.host.snapshot()).tickerObservations?.() !== undefined },
      );
      try {
        await this.sharedPublicFeed.connect();
      } catch (error) {
        this.dispose();
        throw error;
      }
      if (this.isDisposed || !this.host) throw new Error('Exchange disposed during startup');
      if (!this.sharedPublicFeed.isReady)
        this.log('In-process public data has not loaded; continuing private startup', LogSeverity.Warning);
    }
    // A fresh engine's full native snapshot clears the previous worker stream's
    // catalog/book baselines before any new deltas reach the main consumer.
    if (this.onFrame) this.applyEngineFrame(this.host.snapshot(), true, true);
    try {
      let applied: Promise<void> | undefined;
      for (;;) {
        const latest = this.syncPollingPolicy();
        if (!latest || latest === applied) break;
        await latest;
        applied = latest;
        this.assertEngineAlive();
      }
    } catch (error) {
      this.startupFailure = error instanceof Error ? error : new Error(String(error));
      this.dispose();
      throw error;
    }
    // An injected signer binds address-bound slots before the venue starts.
    if (this.signerSetup instanceof DelegatedSignerSetup) await this.host?.prepareAddressSetup();
    if (this.isDisposed || !this.host) throw new Error('Exchange disposed during startup');
    this.host?.start();
    if (this.engineFailure) throw this.engineFailure;
    if (this.isDisposed || !this.host) throw new Error('Exchange disposed during startup');
    this.updateTickerSymbols();
    if (this.engineFailure) throw this.engineFailure;
    // Books, candles and fills go out at the venue's book cadence, the rest
    // at the store's, matching what the TS worker posts today.
    const multiplier = this.options.tuning?.dataUpdateThrottleMultiplier || 1;
    const bookMs = (exchangeMetadata[this.name]?.orderbookTickDelay || STORE_FLUSH_MS) * multiplier;
    this.flushTimers.push(
      setInterval(() => {
        this.updatePollingPolicy();
        this.host?.flush();
      }, STORE_FLUSH_MS * multiplier),
    );
    // Staleness has no socket event of its own; re-derive health on a slow clock.
    this.flushTimers.push(setInterval(this.syncSocketHealthToStore, SOCKET_HEALTH_SYNC_MS));
    if (bookMs < STORE_FLUSH_MS * multiplier) {
      this.flushTimers.push(setInterval(() => this.host?.flush(true), bookMs));
    }
    try {
      await this.untilLoaded();
      this.assertEngineAlive();
    } catch (error) {
      this.dispose();
      throw error;
    }
    if (this.fetchAccountTier && !this.isPublicDataOnly) {
      void this.syncLighterTier();
      this.flushTimers.push(setInterval(() => void this.syncLighterTier(), LIGHTER_TIER_SYNC_MS));
    }
    // Interval only: a background read never interleaves with the caller's first commands.
    if (this.capabilityRecord?.balanceInfo && !this.isPublicDataOnly) {
      this.flushTimers.push(setInterval(() => void this.syncBalanceInfo(), BALANCE_INFO_SYNC_MS));
    }
  };

  private balanceInfoJson?: string;

  /**
   * The venue's raw account payload as TS stores it in `balance.info` (Bybit: the wallet
   * account with `accountType` and the `coin` collateral array), read by the web margin UI.
   */
  private syncBalanceInfo = async () => {
    try {
      const info = await this.command<Record<string, unknown> | null>({ op: 'fetchBalanceInfo' }, { report: false });
      if (!info || this.isDisposed) return;
      const json = JSON.stringify(info);
      if (json === this.balanceInfoJson) return;
      this.balanceInfoJson = json;
      this.store.update({ balance: { ...this.store.balance, info } });
    } catch {
      // The engine reports wallet-read failures itself; the last payload stays.
    }
  };

  /** The web tier setting reads TS's `balance.info.lighterAccountLimits`; frames keep `info`. */
  private writeLighterTier = (tier: LighterAccountTier) => {
    if (this.isDisposed || this.store.balance.info?.lighterAccountLimits?.user_tier_name === tier) return;
    this.store.update({
      balance: {
        ...this.store.balance,
        info: { ...this.store.balance.info, lighterAccountLimits: { user_tier_name: tier } },
      },
    });
  };

  private syncLighterTier = async () => {
    try {
      const tier = await this.fetchAccountTier?.();
      if (tier) this.writeLighterTier(tier);
    } catch {
      // The engine reports tier-read failures itself; the last known tier stays.
    }
  };

  /** The whole current state as one frame, for a consumer that starts late. */
  snapshotFrame = () => {
    this.host?.flush();
    return this.host?.snapshot();
  };

  fetchEngineSnapshot = async (): Promise<EngineSnapshot> => this.captureEngineSnapshot();

  private captureEngineSnapshot(): EngineSnapshot {
    const frame = this.snapshotFrame();
    if (!frame) throw new Error('Engine has not started');
    const fetchedLeverage: Record<string, FetchedLeverage> = {};
    for (const symbol of Object.keys(this.store.fetchedLeverage))
      fetchedLeverage[symbol] = { ...this.store.fetchedLeverage[symbol] };
    return { frame, fetchedLeverage };
  }

  /** A late shared subscriber receives the current native book without waiting for a trade. */
  snapshotOrderBook = (symbol: string): OrderBook | undefined => {
    const snapshot = this.snapshotFrame();
    return snapshot
      ? this.applier.books(new Frame(snapshot)).find((entry) => entry.symbol === symbol)?.orderbook
      : undefined;
  };

  updateSymbolMetadata = async (symbol: string) => {
    if (this.isPublicDataOnly) return;
    // A page with cached or kept rows selects before this engine has its markets, and nothing asks again.
    if (this.startupState === 'pending') await this.startup?.catch(() => undefined);
    const capabilities = this.capabilityRecord;
    if (!capabilities) {
      this.log(`Symbol metadata for ${symbol} was not refreshed: the engine has not started`, LogSeverity.Warning);
      return;
    }
    const readsLeverage =
      capabilities.leverageQuery === 'onMetadata' || capabilities.leverageQuery === 'onReadAndMetadata';
    if (capabilities.maxLeverage !== 'query' && !readsLeverage) return;
    // Refreshed on every symbol selection, and a read can refuse routinely (portfolio margin).
    try {
      await this.untilNativeData(LOADED_FLAG.MARKETS, 'markets');
      if (this.isDisposed || !Object.hasOwn(this.store.marketsBySymbol, symbol)) return;
      if (capabilities.maxLeverage === 'query') {
        await this.fetchSymbolMaxLeverage(symbol);
        return;
      }
      const rails = await this.command<FetchedLeverage>({ op: 'fetchLeverage', symbol }, { report: false });
      this.host?.flush();
      this.applier.applyLeverage(symbol, rails);
    } catch (error) {
      this.log(`Error refreshing leverage for ${symbol}: ${error}`, LogSeverity.Warning);
    }
  };

  private frameRecoveryPending = false;

  private receiveFrame = (buffer: ArrayBuffer) => {
    // Only the worker's frames, applied through `applyWorkerFrame`, own a plain mirror's store.
    if (this.isWorkerMirror && !this.frameFed) return;
    this.applyEngineFrame(buffer, true);
  };

  private applyEngineFrame(buffer: ArrayBuffer, recover: boolean, snapshot = false) {
    if (this.isDisposed) return;
    let frame: Frame | undefined;
    let notified: Set<string> | undefined;
    const rejected = (section: 'frame' | 'books' | 'candles', error: unknown) => {
      if (notified?.has(section)) return;
      (notified ??= new Set()).add(section);
      let symbols: readonly string[] = [];
      try {
        if (frame) symbols = this.applier.marketSymbolsFor(frame);
      } catch {
        /* Unknown identities fan out. */
      }
      const signal = new WorkerFrameRecoveryScopes().reject(section, error, frame, symbols);
      if (section === 'frame' || section === 'books')
        this.dispatchFrameEvent(() => this.emitter.emit('workerBookFrameRejected', signal));
      if (this.isDisposed) return;
      if (section === 'frame' || section === 'candles')
        this.dispatchFrameEvent(() => this.emitter.emit('workerCandleFrameRejected', signal));
    };
    try {
      frame = new Frame(buffer);
    } catch (error) {
      this.applier.invalidateMarketIdentity();
      rejected('frame', error);
      diagnosticConsole.error('Invalid engine frame', error);
      if (recover) this.recoverEngineFrame();
      return;
    }
    const decoded = frame;
    const previousMarkets = this.store.markets;
    let needsSnapshot = false;
    try {
      this.applier.apply(frame, !this.frameConsumerDecodesMarketData || this.feedBookListeners.size > 0);
    } catch (error) {
      needsSnapshot = error instanceof FrameApplyError;
      if (error instanceof FrameApplyError && error.bookRejected) rejected('books', error);
      diagnosticConsole.error('Engine frame apply failed', error);
    }
    if (this.isDisposed) return;
    const read = <T>(section: () => T[], stream?: 'books' | 'candles'): T[] => {
      if (this.isDisposed) return [];
      try {
        return section();
      } catch (error) {
        needsSnapshot = true;
        if (stream) rejected(stream, error);
        diagnosticConsole.error('Engine frame section failed', error);
        return [];
      }
    };
    let receivedLoaded = this.nativeLoaded;
    try {
      receivedLoaded = frame.status()?.loaded ?? receivedLoaded;
      this.nativeLoaded = receivedLoaded;
    } catch (error) {
      needsSnapshot = true;
      diagnosticConsole.error('Engine status section failed', error);
    }
    const catalogChanged = previousMarkets !== this.store.markets;
    if (this.publicFeedObservers.size) {
      let publicChanged = catalogChanged;
      try {
        publicChanged ||= Boolean(frame.tickers() || frame.tickerDeltas() || frame.tickerObservations?.());
      } catch (error) {
        needsSnapshot = true;
        diagnosticConsole.error('Engine public feed section failed', error);
      }
      if (publicChanged)
        for (const observer of this.publicFeedObservers) {
          this.dispatchFrameEvent(() => observer.updated(catalogChanged));
          if (this.isDisposed) return;
        }
    }
    let sharedFailure: Error | undefined;
    let sendSharedSnapshot = false;
    if (
      this.sharedPublicFeed &&
      receivedLoaded & LOADED_FLAG.MARKETS &&
      this.applier.hasMarketIdentity &&
      (catalogChanged || !this.requestSharedSnapshot)
    ) {
      try {
        this.sharedPublicFeed.validateMarkets(this.store.markets);
        this.sharedValidatedMarkets = this.store.markets;
        const symbols = new Set(this.store.markets.map((market) => market.symbol));
        const gainedSymbols = [...symbols].some((symbol) => !this.sharedNativeSymbols.has(symbol));
        // Install before dispatch: a synchronous provider reply or recovery can
        // apply another frame with the same catalogue.
        this.sharedNativeSymbols = symbols;
        if (!this.requestSharedSnapshot || gainedSymbols) {
          this.requestSharedSnapshot = true;
          sendSharedSnapshot = true;
        }
      } catch (error) {
        sharedFailure = error instanceof Error ? error : new Error(String(error));
        this.startupFailure = sharedFailure;
      }
    }
    for (const fill of read(() => this.applier.fills(decoded))) {
      this.dispatchFrameEvent(() => this.emitter.emit('fill', fill));
      if (this.isDisposed) return;
    }
    if (!this.frameConsumerDecodesMarketData || this.feedBookListeners.size) {
      for (const { symbol, orderbook } of read(() => this.applier.books(decoded), 'books')) {
        const listeners = this.bookListeners.get(symbol);
        if (!this.capabilityRecord?.isolatedBookSubscriptions)
          listeners?.forEach((listener) => {
            if (this.isDisposed) return;
            if (!this.frameConsumerDecodesMarketData || this.feedBookListeners.has(listener))
              this.dispatchFrameEvent(() => listener(orderbook));
          });
        else if (listeners)
          for (const [id, listener] of [...listeners]) {
            if (this.isDisposed) return;
            if (listeners.has(id) && (!this.frameConsumerDecodesMarketData || this.feedBookListeners.has(listener)))
              this.dispatchFrameEvent(() =>
                listener({
                  bids: orderbook.bids.map((level) => ({ ...level })),
                  asks: orderbook.asks.map((level) => ({ ...level })),
                }),
              );
          }
        if (this.isDisposed) return;
      }
    }
    if (this.isDisposed) return;
    if (!this.frameConsumerDecodesMarketData || this.feedCandleListeners.size) {
      for (const { symbol, interval, candle } of read(() => this.applier.candles(decoded), 'candles')) {
        this.candleListeners.get(`${symbol}:${interval}`)?.forEach((listener) => {
          if (this.isDisposed) return;
          if (!this.frameConsumerDecodesMarketData || this.feedCandleListeners.has(listener))
            this.dispatchFrameEvent(() => listener(candle));
        });
        if (this.isDisposed) return;
      }
    }
    if (this.isDisposed) return;
    this.dispatchFrameEvent(() => this.onFrame?.(buffer, snapshot));
    if (this.isDisposed) return;
    if (sharedFailure) {
      const failure = sharedFailure;
      this.dispatchFrameEvent(() => this.emitAndLogError(failure));
      markErrorReported(failure);
      this.dispose();
    } else {
      // Forward the catalog before a subscription can synchronously produce data.
      if (!this.publicDemandReady && this.nativeLoaded & LOADED_FLAG.MARKETS && !this.isDisposed) {
        this.publicDemandReady = true;
        const books = [...this.bookListeners];
        const candles = [...this.candleListeners];
        for (const [symbol, listeners] of books)
          if (this.bookListeners.get(symbol) === listeners && listeners.size) this.bookDemand(symbol, true);
        for (const [key, listeners] of candles)
          if (this.candleListeners.get(key) === listeners && listeners.size) {
            const separator = key.lastIndexOf(':');
            this.candleDemand(key.slice(0, separator), key.slice(separator + 1) as OHLCVOptions['interval'], true);
          }
      }
      if (sendSharedSnapshot) this.dispatchFrameEvent(() => this.sharedPublicFeed?.send({ op: 'snapshot' }));
      if (needsSnapshot && recover) this.recoverEngineFrame();
    }
    for (const wake of this.startupWaiters) wake();
  }

  private recoverEngineFrame() {
    if (this.frameRecoveryPending || this.isDisposed || !this.host) return;
    this.frameRecoveryPending = true;
    try {
      const snapshot = this.captureEngineSnapshot();
      this.applyEngineFrame(snapshot.frame, false, true);
      this.dispatchFrameEvent(() => this.applier.applyLeverageSnapshot(snapshot.fetchedLeverage));
    } catch (error) {
      diagnosticConsole.error('Engine frame recovery failed', error);
    } finally {
      this.frameRecoveryPending = false;
    }
  }

  private receiveEvent = (event: EngineEvent) => {
    if (event.code === 'http.request_failed') event = { ...event, fields: { ...event.fields, venue: this.name } };

    if (event.code === 'startup.incomplete') {
      this.nativeStartupTags = startupDiagnosticTags(event);
      this.nativeStartupSampleAt = Date.now();
    }
    if (event.code === 'request.failed' || event.code === 'startup.incomplete')
      this.lastFailedPrivateReadKind =
        failedPrivateReadKind(event.fields?.lastFailedPrivateReadKind) ?? this.lastFailedPrivateReadKind;
    if (
      event.code === 'request.failed' &&
      failedPrivateReadKind(event.fields?.lastFailedPrivateReadKind) === 'discovery'
    )
      this.invalidateInfoCatalogue?.();
    if (event.code === 'request.failed' || event.code === 'auth.rejected') this.lastReadFailure = event.message;
    if (event.code === 'auth.rejected') this.noteAuthFailure();
    if (event.code === 'settings.held' && this.frameFed) this.refreshSourceSettings(event.fields?.symbol);
    if (event.code === 'orders.held' && this.frameFed) this.askSourceForOrders(event.fields);
    if (
      event.code === 'orders.cancel_check' &&
      this.frameFed &&
      typeof event.fields?.targets === 'string' &&
      event.fields.targets.length <= 32_768
    ) {
      try {
        const targets: unknown = JSON.parse(event.fields.targets);
        if (Array.isArray(targets) && targets.length <= 64) {
          for (const target of targets) {
            if (
              !target ||
              typeof target.id !== 'string' ||
              !target.id ||
              target.id.length > 128 ||
              typeof target.symbol !== 'string' ||
              !target.symbol ||
              target.symbol.length > 128
            )
              continue;
            this.sourceCancelChecks = this.sourceCancelChecks.filter(
              (c) => c.id !== target.id || c.symbol !== target.symbol,
            );
            this.sourceCancelChecks.push({ id: target.id, symbol: target.symbol });
            if (this.sourceCancelChecks.length > 64) this.sourceCancelChecks.shift();
          }
        }
      } catch {
        // An invalid control grants no confirmation and never suppresses the ordinary read.
      }
    }
    if (event.code === 'orders.refresh' && this.frameFed) {
      const named = typeof event.fields?.symbols === 'string' ? event.fields.symbols.split(',') : [''];
      this.refreshSourceOrders(named.includes('') ? undefined : named);
    }
    this.emitter.emit('engine-event', event);
    // Only an explicit clock-advice listener replaces the generic info log.
    if (event.code === 'bybit.clock.corrected' && event.level === 'info' && this.emitter.hasListeners('clock-advice')) {
      this.emitter.emit('clock-advice', event);
      return;
    }
    if (event.level === 'error')
      this.emitAndLogError(
        event.code === 'ws.public_unavailable'
          ? `${exchangeMetadata[this.name]?.prettyName ?? this.name}: ${event.message}`
          : event.message,
        undefined,
        {
          engineEvent: event,
          classified: engineEventClassified(event),
          sentryTags: startupDiagnosticTags(event),
        },
      );
    else if (event.level !== 'debug') {
      // Browser logging consumes the string, not the optional structured third argument.
      const message =
        event.code === 'bitget.account.mode' || event.code === 'bitget.close.diagnostic'
          ? JSON.stringify(event)
          : event.message;
      this.log(message, event.level === 'warn' ? LogSeverity.Warning : LogSeverity.Info, event);
    }
  };

  get socketHost() {
    return this.host;
  }

  /** Engine sockets replace the TS watchdog: venues own their reconnect policy. */
  override syncSocketHealthToStore = () => {
    if (this.isDisposed) return;
    const shared = this.sharedPublicFeed
      ? { isInitialized: this.sharedPublicReady, isHealthy: this.sharedPublicReady }
      : undefined;
    const diagnostics = this.readReconnectDiagnostics?.();
    const health = {
      ...summarizeSocketHealth(this.getPublicSockets(), this.getPrivateSockets(), shared),
      ...(diagnostics ? { reconnectDiagnostics: diagnostics } : {}),
    };
    const current = this.store.socketHealth;
    // A private stream delivering again proves the key; later rejections still count.
    if (health.privateInitialized && !current?.privateInitialized) this.recordAuthSuccess();
    if (current && (Object.keys(health) as (keyof typeof health)[]).every((key) => current[key] === health[key]))
      return;
    this.store.updateSocketHealth(health);
  };

  private receiveLog = (level: 'info' | 'warning' | 'error', message: string) => {
    if (level === 'error') this.emitAndLogError(message);
    else this.log(message, level === 'warning' ? LogSeverity.Warning : LogSeverity.Info);
  };

  /** Private accounts need markets and account state; public-only sessions also need tickers. */
  private untilLoaded = () =>
    new Promise<void>((resolve, reject) => {
      const ready = () => {
        const loaded = (flags: number) => (this.nativeLoaded & flags) === flags;
        const account =
          this.isPublicDataOnly ||
          (loaded(LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS) && this.applier.hasAccountState);
        const publicReady = !this.isPublicDataOnly || (loaded(LOADED_FLAG.TICKERS) && this.applier.hasTickerState);
        const sharedReady =
          !this.sharedPublicFeed || (this.sharedPublicReady && this.sharedValidatedMarkets === this.store.markets);
        return loaded(LOADED_FLAG.MARKETS) && this.applier.hasMarketIdentity && publicReady && account && sharedReady;
      };
      const wake = () => {
        if (!this.startupFailure && !this.isDisposed && !ready()) return;
        this.startupWaiters.delete(wake);
        if (this.startupFailure || this.isDisposed)
          reject(this.startupFailure ?? new Error('Exchange disposed during startup'));
        else resolve();
      };
      this.startupWaiters.add(wake);
      wake();
    });

  /** Valid once the account's balance loads; the engine's first error says why not. */
  validateAccount = (): Promise<ValidateAccountResult> => {
    if (this.bridgeConstruction.apiKeyIndexRequired) {
      const index = parseLighterApiKeyIndex(this.options.apiKeyIndex);
      if (index === undefined || index < 0 || index > 255) {
        return Promise.resolve({ error: 'Invalid API key index. Expected an integer from 0 to 255.' });
      }
    }
    return new Promise((resolve) => {
      let settled = false;
      let started = false;
      const finish = (result: ValidateAccountResult) => {
        if (settled) return;
        settled = true;
        this.emitter.off('error', onError);
        unsubscribe();
        clearTimeout(timeout);
        resolve(result);
      };
      // Venues log the reason just before StartupFailed in the same batch; let the typed failure win.
      const onError = (message: unknown) =>
        queueMicrotask(() => {
          if (this.startupFailure) return loaded();
          finish({ error: typeof message === 'string' && message ? message : 'Invalid API key or secret' });
        });
      const loaded = () => {
        if (this.startupFailure) {
          const { kind } = this.startupFailure as { kind?: ExchangeErrorKind };
          return finish(kind ? { error: this.startupFailure.message, kind } : { error: this.startupFailure.message });
        }
        if (this.isDisposed) return finish({ error: 'Exchange disposed during account validation' });
        if (
          started &&
          (this.store.loaded.balance ||
            (this.capabilityRecord?.publicCatalogueValidation &&
              this.isPublicDataOnly &&
              this.store.loaded.markets &&
              this.store.loaded.tickers))
        )
          finish({ error: '', balance: this.store.balance });
      };
      this.emitter.on('error', onError);
      const unsubscribe = this.store.subscribe(loaded);
      // This covers the engine's load and a full startup, so it says nothing about the key.
      const timeout = setTimeout(
        () =>
          finish({
            error: 'The exchange did not finish loading in time, so the key was not checked. Please try again.',
            kind: 'timeout',
          }),
        VALIDATE_ACCOUNT_TIMEOUT_MS,
      );
      loaded();
      if (!settled)
        this.start().then(
          () => {
            started = true;
            loaded();
          },
          (error) =>
            error instanceof ExchangeError && error.kind
              ? finish({ error: error.message, kind: error.kind as ExchangeErrorKind })
              : onError(String(error?.message ?? error)),
        );
    });
  };

  private reportCommandError = (error: unknown): void => {
    // The terminal engine notification already reports all interrupted commands.
    if (this.engineFailure) {
      markErrorReported(error);
      return;
    }
    if (this.isDisposed || isErrorReported(error)) return;
    markErrorReported(error);
    try {
      this.emitAndLogError(error instanceof Error ? error : String(error));
    } catch {
      /* Diagnostic listeners cannot replace the original write outcome. */
    }
  };

  /** `report: false` is for background reads whose caller owns the failure's visibility. */
  private command = async <T>(
    command: { op: string; [field: string]: unknown },
    { report = true, timing, timeoutMs }: { report?: boolean; timing?: WriteTiming; timeoutMs?: number } = {},
  ): Promise<T> => {
    // Unreported: the account has nothing to read, which is not an error the user caused.
    if (this.options.publicDataOnly === true && PRIVATE_HISTORY_OPS.has(command.op))
      return Promise.reject(notReadOnPublicData(command.op));
    try {
      if (this.engineFailure) throw this.engineFailure;
      if (this.isDisposed) throw new Error(`${this.name} engine disposed`);
      // A cold wasm fetch: the worker takes calls before its engine is built. Reads wait.
      if (!this.host && this.startupState === 'pending' && COLD_START_READS.test(command.op)) await this.ensureHost();
      this.refuseWriteBeforeEngine();
      if (!this.host) throw new Error(`${this.name} engine is not started`);
      if (this.requireCapabilities('command').finiteCommandNumbers) {
        const invalid = (value: unknown): boolean =>
          typeof value === 'number'
            ? !Number.isFinite(value)
            : Boolean(value && typeof value === 'object' && Object.values(value).some(invalid));
        if (invalid(command)) throw new Error('Coinbase command numbers must be finite');
      }
      const options = {
        ...(timing ? { onStage: timing.mark } : {}),
        ...(timeoutMs !== undefined ? { timeoutMs } : {}),
      };
      // A call admitted before readiness must still belong to this engine at handoff.
      if (!this.algoOwnerCurrent(this.twapOwnerBinding)) {
        const message = `${this.name} ${command.op} was not sent: owner changed; recreate the account worker`;
        const error = new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
        // Retire without algo cleanup cancels, including before startAlgos installs its check.
        this.host.dispose();
        this.receiveEngineFailure(error);
        throw error;
      }
      return (await (Object.keys(options).length
        ? this.host.command(command, options)
        : this.host.command(command))) as T;
    } catch (error) {
      // The web's placement failure names the order; a second raw report would only repeat it.
      const callerReports = command.op === 'placeOrders' && this.options.callerReportsPlacementFailures;
      // A history the venue cannot give (MEXC income other than funding) is no failure to show.
      const unsupportedHistory =
        PRIVATE_HISTORY_OPS.has(command.op) && error instanceof ExchangeError && error.kind === 'unsupported';
      if (report && !callerReports && !unsupportedHistory) this.reportCommandError(error);
      throw error;
    }
  };

  /** A write that waited could go out after its caller timed out; failing lets the user retry. */
  private refuseWriteBeforeEngine() {
    if (this.host || this.startupState !== 'pending') return;
    const message = `${this.name} engine is not started; nothing was sent, retry once it loads`;
    throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
  }

  /** A user write: its stages are stamped, and emitted here unless a caller in another thread owns them. */
  private writeCommand = async <T>(
    claimed: ReturnType<typeof claimWriteTiming>,
    command: { op: string; [field: string]: unknown },
  ): Promise<T> => {
    try {
      const result = await this.command<T>(command, { timing: claimed.timing });
      if (claimed.owned) emitWriteTiming(this, claimed.timing, 'ok');
      return result;
    } catch (error) {
      if (claimed.owned) emitWriteTiming(this, claimed.timing, 'error');
      throw error;
    }
  };

  private subscriptionCommand = (command: { op: string; [field: string]: unknown }): void => {
    if (!this.host || this.isDisposed || this.engineFailure) return;
    void this.command(command).catch(() => {
      /* command() owns error reporting. */
    });
  };

  placeOrders = async (orders: AsterDEXPlaceOrderOpts[]) => {
    const timing = claimWriteTiming('placeOrders');
    const ready = this.writeCapabilitiesFor('placeOrders');
    const capabilities = ready instanceof Promise ? await ready : ready;
    for (const order of orders) {
      if (order.impliedVolatility === undefined) continue;
      if (!this.capabilityRecord?.options) this.rejectPreflight('Options are not enabled on this account');
      if (
        typeof order.impliedVolatility !== 'number' ||
        !Number.isFinite(order.impliedVolatility) ||
        order.impliedVolatility <= 0
      ) {
        this.rejectPreflight('impliedVolatility must be finite and positive');
      }
    }
    if (!capabilities.stpControls && orders.some((order) => order.stpMode !== undefined)) {
      this.rejectPreflight(`${this.name} does not support AsterDEX stpMode`);
    }
    const isBybit = capabilities.bybitExecutionControls;
    if (!isBybit && orders.some((order) => 'bybitBbo' in order || 'bybitSlippageToleranceTicks' in order)) {
      this.rejectPreflight(`${this.name} does not support Bybit BBO or TickSize controls`);
    }
    if (!isBybit) {
      for (const order of orders) {
        for (const field of ['amount', 'price', 'triggerPrice', 'stopLoss', 'takeProfit', 'maxSlippagePct'] as const) {
          const value = order[field];
          if (value !== undefined && (typeof value !== 'number' || !Number.isFinite(value))) {
            this.rejectPreflight(`${this.name} ${field} must be finite`);
          }
        }
      }
    }
    this.assertCallerClientOrderIds(orders);
    if (isBybit) {
      for (const order of orders) {
        for (const field of ['amount', 'price', 'triggerPrice', 'stopLoss', 'takeProfit'] as const) {
          const value = order[field];
          if (value !== undefined && (typeof value !== 'number' || !Number.isFinite(value))) {
            this.rejectPreflight(`Bybit ${field} must be finite`);
          }
        }
        if (
          capabilities.rejectClientSlippage &&
          (order.maxSlippagePct !== undefined || 'bybitBbo' in order || 'bybitSlippageToleranceTicks' in order)
        ) {
          this.rejectPreflight('HyroTrader does not support client-defined slippage or Bybit execution controls');
        }
        const ticks = 'bybitSlippageToleranceTicks' in order ? order.bybitSlippageToleranceTicks : undefined;
        if (ticks !== undefined && (typeof ticks !== 'number' || !Number.isFinite(ticks))) {
          this.rejectPreflight('Bybit bybitSlippageToleranceTicks must be finite');
        }
        if (order.maxSlippagePct !== undefined && !Number.isFinite(order.maxSlippagePct)) {
          this.rejectPreflight('Bybit maxSlippagePct must be finite');
        }
      }
    }
    if (capabilities.stpControls) for (const order of orders) this.validateAsterStp(order.stpMode);
    for (const order of orders) this.attemptedPlacementSymbols.add(order.symbol);
    return this.writeCommand<string[]>(timing, { op: 'placeOrders', orders });
  };

  placeOrder = async (opts: AsterDEXPlaceOrderOpts) => {
    this.resolveCallerClientOrderId(opts);
    return this.placeOrders([opts]);
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const timing = claimWriteTiming('amendOrder');
    const price = 'price' in update ? update.price : undefined;
    const amount = 'amount' in update ? update.amount : undefined;
    if (price === undefined && amount === undefined) throw new Error('Order amendment requires a price or amount');
    if ([price, amount].some((value) => value !== undefined && !Number.isFinite(value))) {
      throw new Error('Order amendment price and amount must be finite numbers');
    }
    const withSymbol = this.capabilityRecord?.amendWithSymbol;
    const result = await this.writeCommand<string[] | { orderIds: string[]; clientOrderId: string }>(timing, {
      op: withSymbol ? 'amendOrderWithSymbol' : 'amendOrder',
      id: order.id,
      ...(withSymbol ? { symbol: order.symbol } : {}),
      ...('price' in update ? { price: update.price } : {}),
      ...('amount' in update ? { amount: update.amount } : {}),
    });
    return Array.isArray(result) ? result : withReplacementClientOrderId(result.orderIds, result.clientOrderId);
  };

  cancelOrders = async (orders: Order[]) => {
    const timing = claimWriteTiming('cancelOrders');
    const descriptors = orders.map(({ id, symbol, type }) => ({ id, symbol, type }));
    if (this.capabilityRecord?.cancelWithSymbols) {
      await this.writeCommand(timing, { op: 'cancelOrdersWithSymbols', orders: descriptors });
    } else
      await this.writeCommand(timing, {
        op: 'cancelOrders',
        ids: orders.map((order) => order.id),
        orders: descriptors,
      });
  };

  cancelSymbolOrders = async (symbol: string) => {
    await this.writeCommand(claimWriteTiming('cancelSymbolOrders'), { op: 'cancelSymbolOrders', symbol });
  };

  nuke = async (_tries = 0) => this.runNativeNuke(true);

  nukePositions = async (_tries = 0) => this.runNativeNuke(false);

  override nukeOrders = async (): Promise<NukeResult> => {
    this.assertEngineAlive();
    if (this.isNuking) this.rejectPreflight('A nuke is already in progress');
    this.isNuking = true;
    let warning: string | undefined;
    try {
      const { placedSymbols, ...stopped } = await this.stopAlgosForNuke();
      warning = unseenOrdersWarning(await this.capabilitiesFor('nuke'));
      await this.cancelNukeOrders(placedSymbols);
      return warning ? { ...stopped, warning } : stopped;
    } catch (error) {
      throw withWarning(error, warning);
    } finally {
      this.isNuking = false;
    }
  };

  /**
   * Every traded symbol's cancel, then, where the venue has one, its account-wide cancel:
   * nuke means everything. Each runs whatever the other did; failures are named together.
   */
  private cancelNukeOrders = async (placedSymbols?: string[]): Promise<void> => {
    const failures: unknown[] = [];
    await this.cancelAllOrders(await this.nukeCancelSymbols(placedSymbols)).catch((error) => failures.push(error));
    if ((await this.capabilitiesFor('nuke')).nukeAccountCancel)
      await this.command({ op: 'cancelAccountOrders' }).catch((error) =>
        failures.push(withWarning(error, NUKE_ORDERS_MAY_REMAIN)),
      );
    if (failures.length === 1) throw failures[0];
    if (failures.length) throw new Error(failures.map(errorMessage).join('; '));
  };

  /** Running chasers and TWAPs would place again behind the nuke: stop them first. */
  private stopAlgosForNuke = async (): Promise<AlgoStopWithPlacements> => {
    const stopped = this.isWorkerMirror
      ? ((await this.callWorker('stopAccountAlgos')) as AlgoStopWithPlacements)
      : await this.stopAccountAlgos();
    const count = stopped.stoppedChasers.length + stopped.stoppedTwaps.length;
    if (count)
      this.log(
        `Nuke stopped ${count} algo(s) first: chasers [${stopped.stoppedChasers.join(', ')}], TWAPs [${stopped.stoppedTwaps.join(', ')}]`,
        LogSeverity.Warning,
      );
    return stopped;
  };

  /** Beyond what cancel-all itself covers: open positions, and the stopping engine's placements. */
  private nukeCancelSymbols = async (placedSymbols: string[] = []): Promise<string[]> => [
    ...placedSymbols,
    ...(await this.fetchPositions().catch(() => this.store.positions))
      .filter((position) => position.contracts !== 0)
      .map((position) => position.symbol),
  ];

  private runNativeNuke = async (cancelOrders: boolean): Promise<NukeResult> => {
    this.assertEngineAlive();
    if (this.isNuking) this.rejectPreflight('A nuke is already in progress');
    this.isNuking = true;
    let failed = false;
    let failure: unknown;
    try {
      const { placedSymbols, ...stopped } = await this.stopAlgosForNuke();
      const leg = async (run: () => Promise<void>) => {
        try {
          await run();
        } catch (error) {
          this.reportCommandError(error);
          // Both legs failed: the final error names each.
          if (failed && failure instanceof Error) {
            if (failure.message !== errorMessage(error)) failure.message += `; ${errorMessage(error)}`;
          } else failure = error;
          failed = true;
        }
      };
      // Orders first: a market close otherwise fills against the user's own resting bid.
      if (cancelOrders) await leg(() => this.cancelNukeOrders(placedSymbols));
      await leg(() => this.closeUntilFlat());
      const warning = cancelOrders ? unseenOrdersWarning(await this.capabilitiesFor('nuke')) : undefined;
      if (failed) {
        this.logNukeRemainder();
        throw withWarning(failure, warning);
      }
      return warning ? { ...stopped, warning } : stopped;
    } finally {
      this.isNuking = false;
    }
  };

  /** What still needs closing; a position that cannot be sized counts as open. */
  private positionsStillOpen = async (): Promise<Position[]> =>
    (await this.fetchPositions()).filter((position) => {
      try {
        return Boolean(this.nukeCloseOrder(position));
      } catch {
        return true;
      }
    });

  /**
   * A positions read issued after this call; resolves with the rows it could not keep and
   * what it left open. On the engine's owner it flushes, so frames precede the answer.
   */
  readPositions = async (): Promise<FreshPositionsRead> => {
    const read = await this.command<FreshPositionsRead>({ op: 'readPositions' }, { report: false });
    this.host?.flush();
    return read;
  };

  /**
   * A close can succeed and leave a position (a fill against a resting order), so flat is
   * read, not assumed: a venue with `freshPositions` is flat only on a read issued at least
   * `NUKE_READ_SETTLE_MS` after the last close. If that read fails, no further close is sent.
   */
  private closeUntilFlat = async (): Promise<void> => {
    const capabilities = await this.capabilitiesFor('nuke');
    for (let round = 0; round < NUKE_FLAT_ROUNDS; round += 1) {
      const closed = await this.closeAllPositions();
      if (closed || !capabilities.freshPositions) await this.settleAfterClose(capabilities.freshPositions);
      if (this.isDisposed) return;
      if (capabilities.freshPositions) {
        const { skipped, open } = await this.freshNukeRead();
        if (!open.length) {
          if (skipped.length) throw new Error(`${NUKE_NOT_CONFIRMED}: ${skipped.join(', ')} on no listed market`);
          return;
        }
        if (round === NUKE_FLAT_ROUNDS - 1)
          throw new Error(`Nuke did not leave the account flat: ${describeOpen(open)} still open`);
        continue;
      }
      const left = await this.positionsStillOpen();
      if (!left.length) return;
      if (round === NUKE_FLAT_ROUNDS - 1)
        throw new Error(`Nuke did not leave the account flat: ${describeOpen(left)} still open`);
    }
  };

  /** Up to the flat settle while the store shows open; before a proving read, at least the read settle. */
  private settleAfterClose = async (provingRead: boolean): Promise<void> => {
    const closedAt = Date.now();
    if (provingRead) await new Promise((resolve) => setTimeout(resolve, NUKE_READ_SETTLE_MS));
    while (
      Date.now() < closedAt + NUKE_FLAT_SETTLE_MS &&
      !this.isDisposed &&
      (await this.positionsStillOpen()).length
    ) {
      await new Promise((resolve) => setTimeout(resolve, NUKE_CLOSE_RETRY_MS));
    }
  };

  private freshNukeRead = async (): Promise<FreshPositionsRead> => {
    try {
      const read: unknown = await this.readPositions();
      if (!isFreshPositionsRead(read)) throw new Error('unexpected positions read result');
      return read;
    } catch (error) {
      const left = await this.positionsStillOpen().catch(() => []);
      const named = left.length ? `${describeOpen(left)} may still be open: ` : '';
      throw Object.assign(new Error(`${NUKE_NOT_CONFIRMED}: ${named}${errorMessage(error)}`), { cause: error });
    }
  };

  /**
   * Closes one position at a time so a failure names its symbol, as legacy, and
   * retries only a failed close whose position still shows open: a close that
   * succeeded behind a lagging store is never sent again. Every close is reduce-only.
   * Resolves whether any close was attempted.
   */
  private closeAllPositions = async (): Promise<boolean> => {
    let attempted = false;
    const unusable: { symbol: string; error: unknown }[] = [];
    let failures = new Map<string, { symbol: string; error: unknown }>();
    for (let attempt = 0; attempt <= NUKE_CLOSE_RETRIES; attempt += 1) {
      if (attempt > 0) await new Promise((resolve) => setTimeout(resolve, NUKE_CLOSE_RETRY_MS));
      const failedLast = failures;
      failures = new Map();
      for (const position of await this.fetchPositions()) {
        const key = `${position.symbol}:${position.side}`;
        if (this.isDisposed || (attempt > 0 && !failedLast.has(key))) continue;
        let order: PlaceOrderOpts | undefined;
        try {
          order = this.nukeCloseOrder(position);
        } catch (error) {
          unusable.push({ symbol: position.symbol, error });
          continue;
        }
        try {
          if (order) {
            attempted = true;
            await this.placeOrders([order]);
          }
        } catch (error) {
          failures.set(key, { symbol: position.symbol, error });
        }
      }
      if (!failures.size) break;
    }
    const open = new Set(
      (await this.fetchPositions())
        .filter((position) => position.contracts !== 0)
        .map((position) => `${position.symbol}:${position.side}`),
    );
    for (const key of failures.keys()) if (!open.has(key)) failures.delete(key);
    const failed = [...failures.values(), ...unusable];
    if (!failed.length) return attempted;
    const unknown = failed.find(({ error }) => (error as { outcome?: unknown })?.outcome === 'unknown');
    const details = (unknown ?? failed[0]).error;
    throw Object.assign(
      new Error(
        `Nuke could not close ${failed.map(({ symbol, error }) => `${symbol}: ${errorMessage(error)}`).join('; ')}`,
      ),
      details instanceof Error ? { ...details } : {},
    );
  };

  /** The reduce-only market close; undefined for a flat position, throws on an unusable one. */
  private nukeCloseOrder = (position: Position): PlaceOrderOpts | undefined => {
    const market = this.store.marketsBySymbol[position.symbol];
    if (!market) {
      if (position.contracts !== 0) throw new Error('market not found');
      return undefined;
    }
    const sized = market.isInverse ? position.notional : position.contracts;
    const native = position.extra?.contractsNative;
    if (
      sized === undefined ||
      !Number.isFinite(sized) ||
      (native !== undefined && !Number.isFinite(native)) ||
      (market.contractSize !== undefined && (!Number.isFinite(market.contractSize) || market.contractSize < 0))
    ) {
      throw new Error('invalid close size');
    }
    const amount = nativePositionSize(position, market);
    if (amount === null || !Number.isFinite(amount) || amount < 0) throw new Error('invalid close size');
    if (amount === 0) return undefined;
    return {
      symbol: position.symbol,
      side: position.side === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy,
      type: OrderType.Market,
      amount,
      reduceOnly: true,
    };
  };

  private logNukeRemainder = (): void => {
    this.syncAccountState();
    const ids = new Map<string, string[]>();
    for (const { symbol, id } of this.store.orders) ids.set(symbol, [...(ids.get(symbol) ?? []), id]);
    const orders = [...ids].map(([symbol, list]) => `${symbol}: ${list.join(', ')}`).join('; ');
    const positions = this.store.positions
      .filter((position) => position.contracts !== 0)
      .map((position) => position.symbol)
      .join(', ');
    // Unloaded rows are not "none": the failure may be that they never loaded.
    const list = (rows: string, loaded: boolean) => rows || (loaded ? 'none' : 'unknown, not loaded');
    this.log(
      `Nuke incomplete. Open orders: ${list(orders, this.store.loaded.orders)}. Open positions: ${list(positions, this.store.loaded.positions)}.`,
      LogSeverity.Error,
    );
  };

  /** Engine health counters; undefined before the engine starts or after disposal. */
  getHealth = (): EngineHealth | undefined => (this.host && !this.isDisposed ? this.host.health() : undefined);

  // Without an engine (a main-thread mirror) the open-order lookup still answers, non-definitively.
  findOrderByClientOrderId = async (symbol: string, clientOrderId: string): Promise<ClientOrderLookup> =>
    this.capabilityRecord?.clientOrderLookup
      ? this.command<ClientOrderLookup>({ op: 'findOrderByClientOrderId', symbol, clientOrderId })
      : this.findOrderFromOpenOrders(symbol, clientOrderId);

  /**
   * `alsoSymbols` reach a nuke's traded symbols the store cannot show. A venue-wide cancel
   * covers them; otherwise each is cancelled with the native orders and attempted placements.
   */
  cancelAllOrders = async (alsoSymbols: string[] = []) => {
    this.refuseWriteBeforeEngine();
    const ready = this.writeCapabilitiesFor('cancelAllOrders');
    const capabilities = ready instanceof Promise ? await ready : ready;
    if (capabilities.cancelAll === 'native') {
      await this.command({ op: 'cancelSymbolOrders', symbol: '' });
      return;
    }
    // Native current orders, not a JS store that can lag the engine's flush; frames can
    // lag placement ACKs, so attempted scopes stay for the account's lifetime.
    const orders = capabilities.cancelAll === 'eachOpenSymbol' ? await this.fetchOrders() : this.store.orders;
    await this.cancelEachSymbol(
      new Set([...orders.map((order) => order.symbol), ...this.attemptedPlacementSymbols, ...alsoSymbols]),
    );
  };

  private cancelEachSymbol = async (symbols: Set<string>) => {
    const failures: string[] = [];
    for (const symbol of symbols) {
      if (this.isDisposed) continue;
      try {
        await this.cancelSymbolOrders(symbol);
      } catch (error) {
        // A rejected scope must not prevent cancellation of later live orders.
        failures.push(`${symbol}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
    if (failures.length) throw new Error(`${this.name} cancel-all failed: ${failures.join('; ')}`);
  };

  /** Identity reads leave failure visibility to their caller. */
  getAccount = async (): Promise<ExchangeAccount> => {
    if (this.isWorkerMirror) return (await this.callWorker('getAccount')) as ExchangeAccount;
    this.assertEngineAlive();
    await this.ensureHost();
    const lookup = this.capabilityRecord?.account;
    if (lookup === 'afterLoad') await this.start();
    else if (lookup !== 'query') throw new Error('Not implemented');
    return this.command<ExchangeAccount>({ op: 'fetchAccount' }, { report: false });
  };

  setAllLeverage = async (leverage: number) => {
    this.refuseWriteBeforeEngine();
    if (this.capabilityRecord?.accountLeverage) await this.command({ op: 'setLeverage', symbol: '', leverage });
    else {
      const positions = await this.fetchPositions();
      if (this.isDisposed) return;
      const liveSymbols = new Set(positions.map((position) => position.symbol));
      const flatSymbols = new Set(
        Object.values(this.store.positionSettings ?? {})
          .map((settings) => settings.symbol)
          .filter((symbol) => !liveSymbols.has(symbol)),
      );
      for (const position of positions) {
        if (this.isDisposed) return;
        await this.setLeverage(position.symbol, leverage, position.side);
      }
      for (const symbol of flatSymbols) {
        if (this.isDisposed) return;
        await this.setLeverage(symbol, leverage);
      }
    }
  };

  setLeverage = async (symbol: string, leverage: number, side?: PositionSide) => {
    await this.command({ op: 'setLeverage', symbol, leverage, ...(side !== undefined ? { side } : {}) });
    // Publish the confirmed rails now rather than at the next store flush.
    this.host?.flush();
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol && !this.capabilityRecord?.accountMarginType) throw new Error('Symbol is required');
    await this.command({ op: 'setMarginType', symbol: symbol ?? '', isolated });
  };

  changePositionMode = async (hedged: boolean, symbol?: string) => {
    await this.command({ op: 'setPositionMode', hedged, ...(symbol !== undefined ? { symbol } : {}) });
  };

  private validateAsterStp = (mode?: AsterDEXStpMode): void => {
    if (mode === undefined) return;
    if (this.asterApiVersion !== 'v3') throw new Error('AsterDEX self-trade prevention requires API v3');
    if (!isAsterDEXStpMode(mode)) throw new Error(`Invalid AsterDEX self-trade-prevention mode: ${String(mode)}`);
  };

  fetchStpMode = async (): Promise<AsterDEXStpMode> => {
    const ready = this.capabilitiesFor('fetchStpMode');
    const capabilities = ready instanceof Promise ? await ready : ready;
    if (!capabilities.stpControls) throw new Error('Self-trade prevention is unavailable for this venue');
    this.validateAsterStp('EXPIRE_TAKER');
    return this.command<AsterDEXStpMode>({ op: 'fetchStpMode' });
  };

  changeStpMode = async (mode: AsterDEXStpMode): Promise<void> => {
    this.refuseWriteBeforeEngine();
    const ready = this.writeCapabilitiesFor('changeStpMode');
    const capabilities = ready instanceof Promise ? await ready : ready;
    if (!capabilities.stpControls) throw new Error('Self-trade prevention is unavailable for this venue');
    this.validateAsterStp('EXPIRE_TAKER');
    if (!isAsterDEXStpMode(mode)) throw new Error(`Invalid AsterDEX self-trade-prevention mode: ${String(mode)}`);
    await this.command({ op: 'setStpMode', mode });
  };

  // The engine reconciles orders and positions itself; reads serve its state.
  /** A mirror's sections load from the worker's frames; it never starts its own engine. */
  private untilWorkerData = (flag: number, label: string): Promise<void> =>
    new Promise<void>((resolve, reject) => {
      const loaded = () => LOADED_SECTIONS.every(([section, key]) => !(flag & section) || this.store.loaded[key]);
      if (loaded()) return resolve();
      const finish = (error?: Error) => {
        clearTimeout(timeout);
        unsubscribe();
        this.startupWaiters.delete(wake);
        if (error) reject(error);
        else resolve();
      };
      // Disposal wakes startup waiters, so an idle mirror does not wait out the timeout.
      const wake = () => {
        if (this.isDisposed) finish(new Error('Exchange disposed during native read'));
        else if (loaded()) finish();
      };
      const unsubscribe = this.store.subscribe(wake);
      this.startupWaiters.add(wake);
      const timeout = setTimeout(() => finish(new Error(`Timed out waiting for worker ${label}`)), 30_000);
    });

  /** Reads wait only for their native sections, independently of unrelated reconciliation. */
  private untilNativeData = (flag: number, label: string): Promise<void> => {
    if (this.isWorkerMirror) return this.untilWorkerData(flag, label);
    const ready = new Promise<void>((resolve, reject) => {
      const finish = (error?: Error) => {
        clearTimeout(timeout);
        this.startupWaiters.delete(wake);
        if (error) reject(error);
        else resolve();
      };
      const wake = () => {
        const failure = this.engineFailure ?? this.startupFailure;
        if (failure || this.isDisposed) finish(failure ?? new Error('Exchange disposed during native read'));
        else if (
          this.isPublicDataOnly &&
          this.nativeLoaded & LOADED_FLAG.MARKETS &&
          (flag & (LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS) & ~this.nativeLoaded) !== 0
        )
          finish(new Error(`Native ${label} is unavailable for a public-data-only account`));
        else if (
          (this.nativeLoaded & flag) === flag &&
          (!(flag & LOADED_FLAG.TICKERS) || !this.sharedPublicFeed || this.sharedPublicReady)
        )
          finish();
      };
      const waitMs = this.pageCandleSource ? PAGE_CANDLE_MARKETS_WAIT_MS : 30_000;
      const timeout = setTimeout(() => finish(new Error(`Timed out waiting for native ${label}`)), waitMs);
      this.startupWaiters.add(wake);
      wake();
    });
    // Own the background startup rejection even when one native section finishes first.
    void this.start().catch((error: unknown) => {
      this.startupFailure ??= error instanceof Error ? error : new Error(String(error));
      for (const wake of this.startupWaiters) wake();
    });
    return ready;
  };

  private marketRefreshPending?: Promise<{ added: Market[]; removed: Market[] }>;

  refreshMarkets = (): Promise<{ added: Market[]; removed: Market[] }> => {
    if (this.marketRefreshPending) return this.marketRefreshPending;
    const pending = this.refreshNativeMarkets().finally(() => {
      if (this.marketRefreshPending === pending) this.marketRefreshPending = undefined;
    });
    this.marketRefreshPending = pending;
    return pending;
  };

  private refreshNativeMarkets = async (): Promise<{ added: Market[]; removed: Market[] }> => {
    await this.untilNativeData(LOADED_FLAG.MARKETS | LOADED_FLAG.TICKERS, 'markets');
    this.assertEngineAlive();
    // Older/other engines retain their existing cached refresh behavior.
    if (!this.capabilityRecord?.marketRefresh) return { added: [], removed: [] };
    // A consumer may not diverge from its authoritative provider catalogue.
    // Provider-side refresh keeps the existing identity-change/restart protocol.
    if (this.publicFeedMode === 'shared')
      throw new Error('Refresh the shared public feed source, then restart provider and accounts');
    const previous = new Map(this.store.markets.map((market) => [market.id, market]));
    await this.command({ op: 'refreshMarkets' });
    this.host?.flush();
    this.assertEngineAlive();
    const current = this.store.markets;
    const currentIds = new Set(current.map((market) => market.id));
    const added = current.filter((market) => !previous.has(market.id));
    const removed = [...previous.values()].filter((market) => !currentIds.has(market.id));
    if (added.length || removed.length) this.emitter.emit('markets:changed', { added, removed });
    return { added, removed };
  };

  fetchMarkets = async () => {
    await this.untilNativeData(LOADED_FLAG.MARKETS, 'markets');
    this.assertEngineAlive();
    return this.store.markets;
  };

  fetchTickers = async () => {
    await this.untilNativeData(LOADED_FLAG.MARKETS | LOADED_FLAG.TICKERS, 'tickers');
    this.assertEngineAlive();
    return this.store.tickers;
  };

  private assertEngineAlive = () => {
    if (this.engineFailure || this.startupFailure) throw this.engineFailure ?? this.startupFailure;
    if (this.isDisposed) throw new Error('Exchange disposed');
  };

  /** Read native account state without consuming deltas or requesting network reconciliation. */
  override syncAccountState = (): void => {
    // A bridged mirror's store is kept current by the worker's frames.
    if (this.routesToWorker) return;
    this.assertEngineAlive();
    if (!this.host) throw new Error('Rust engine is not started');
    if (!(this.nativeLoaded & LOADED_FLAG.MARKETS)) throw new Error('Native market catalog is not ready');
    // Store state commits before notification; nested decision readers can use
    // that state without recursively publishing another account snapshot.
    if (this.syncingAccountState) return;
    this.syncingAccountState = true;
    try {
      this.receiveFrame(this.host.accountSnapshot());
    } finally {
      this.syncingAccountState = false;
    }
    this.assertEngineAlive();
  };

  fetchBalance = async () => {
    await this.untilNativeData(LOADED_FLAG.MARKETS | LOADED_FLAG.BALANCE, 'balance');
    this.syncAccountState();
    return this.store.balance;
  };

  fetchOptionChain = (query: OptionChainQuery): Promise<OptionChainRow[]> => {
    this.assertOptionQueries();
    for (const field of ['expiry', 'strikeMin', 'strikeMax'] as const) {
      const value = query[field];
      if (value !== undefined && (typeof value !== 'number' || !Number.isFinite(value))) {
        this.rejectPreflight(`Option query ${field} must be finite`);
      }
    }
    return this.command({ op: 'fetchOptionChain', query });
  };

  fetchOptionExpiries = (underlying: string): Promise<number[]> => {
    this.assertOptionQueries();
    return this.command({ op: 'fetchOptionExpiries', underlying });
  };

  fetchAccountGreeks = (): Promise<AccountGreeks[]> => {
    this.assertOptionQueries();
    return this.command({ op: 'fetchAccountGreeks' });
  };

  private assertOptionQueries = (): void => {
    if (!this.capabilityRecord?.options) this.rejectPreflight('Options are not enabled on this account');
  };

  fetchOrders = async (symbols?: string[]) => {
    await this.untilNativeData(LOADED_FLAG.MARKETS | LOADED_FLAG.ORDERS, 'orders');
    this.syncAccountState();
    return symbols ? this.store.orders.filter((order) => symbols.includes(order.symbol)) : this.store.orders;
  };

  fetchPositions = async (symbols?: string[]) => {
    await this.untilNativeData(LOADED_FLAG.MARKETS | LOADED_FLAG.POSITIONS, 'positions');
    this.syncAccountState();
    return symbols
      ? this.store.positions.filter((position) => symbols.includes(position.symbol))
      : this.store.positions;
  };

  fetchPositionMode = async () => {
    await this.untilNativeData(LOADED_FLAG.MARKETS | LOADED_FLAG.POSITIONS, 'position mode');
    this.syncAccountState();
    return Boolean(this.store.options.isHedged);
  };

  fetchSymbolMaxLeverage = async (symbol: string): Promise<number> => {
    const ready = this.capabilitiesFor('fetchSymbolMaxLeverage');
    const capabilities = ready instanceof Promise ? await ready : ready;
    const source = capabilities.maxLeverage;
    if (source === 'unsupported') throw new Error('Not implemented');
    if (source === 'rails') {
      if (this.routesToWorker) return (await this.callWorker('fetchSymbolMaxLeverage', symbol)) as number;
      const rails = await this.command<FetchedLeverage>({ op: 'fetchLeverage', symbol });
      return Math.max(rails.buyLeverage, rails.sellLeverage);
    }
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market not found for symbol: ${symbol}`);
    if (source === 'query' && !this.isPublicDataOnly) {
      const leverage = await this.command<number>({ op: 'fetchMaxLeverage', symbol });
      if (!Number.isFinite(leverage) || leverage < 1) throw new Error('Invalid Bitrue maximum leverage');
      this.host?.flush();
      return leverage;
    }
    return market.limits.leverage.max;
  };

  override scheduleCancel = async (time?: number | null): Promise<void> => {
    // Validate before either bridge serializes: JSON would turn NaN/Infinity into a clear.
    if (time != null) {
      if (typeof time !== 'number' || !Number.isSafeInteger(time))
        throw new Error('Hyperliquid scheduleCancel time must be a safe integer UTC timestamp in milliseconds');
    }
    if (this.routesToWorker && !this.frameFed) {
      await this.callWorker('scheduleCancel', time);
      return;
    }
    this.refuseWriteBeforeEngine();
    const ready = this.writeCapabilitiesFor('scheduleCancel');
    const capabilities = ready instanceof Promise ? await ready : ready;
    if (!capabilities.scheduleCancel) throw new Error('scheduleCancel is unavailable for this venue');
    if (time != null && time < this.utcNow() + 5_000)
      throw new Error('Hyperliquid scheduleCancel time must be at least 5 seconds in the future');
    await this.command({ op: 'scheduleCancel', ...(time == null ? {} : { time }) });
  };

  fetchOrderBook = async (symbol: string, limit?: number): Promise<OrderBook> => {
    const ready = this.capabilitiesFor('fetchOrderBook');
    const capabilities = ready instanceof Promise ? await ready : ready;
    if (!capabilities.orderBookQuery) throw new Error('Orderbook snapshot queries are unavailable for this venue');
    return this.command<OrderBook>({ op: 'fetchOrderBook', symbol, limit });
  };

  private candleSource?: RustEngineExchange;
  private readonly constructedAt = Date.now();
  private candleSourceRebuilds = 0;
  private candleSourceRebuildAt = 0;

  /**
   * A source that failed to start or died cannot serve a later read; one whose markets are only
   * slow (a big catalogue, a tunnel round trip, an instruments 429) keeps loading for the next read.
   */
  private get candleSourceBroken(): boolean {
    return Boolean(
      this.isDisposed ||
        this.engineFailure ||
        this.startupFailure ||
        (!(this.nativeLoaded & LOADED_FLAG.MARKETS) && Date.now() - this.constructedAt >= PAGE_CANDLE_MARKETS_STUCK_MS),
    );
  }

  private fetchPageCandles = async (opts: OHLCVOptions) => {
    if (this.isDisposed) throw new Error('Exchange disposed before candle read');
    if (!this.candleSource) {
      // A persistently failing venue is rebuilt on a backoff, never spun; meanwhile the
      // chart sees a visible failure and retries.
      if (Date.now() < this.candleSourceRebuildAt) throw new Error(`${this.name} candle source is recovering`);
      // Frame-fed engines execute only writes; candles always own a credential-free public source.
      const source = new RustEngineExchange(
        this.name,
        {
          ...withoutDelegatedSigning(this.engineOptions),
          workerMirror: false,
          publicDataOnly: true,
          privateOnly: false,
          publicFeed: undefined,
          externalPublicData: undefined,
        },
        new DefaultStore(),
      );
      source.pageCandleSource = true;
      this.candleSource = source;
    }
    const source = this.candleSource;
    try {
      const candles = await source.fetchOHLCV(opts);
      this.candleSourceRebuilds = 0;
      return candles;
    } catch (error) {
      if (this.candleSource === source && source.candleSourceBroken) {
        this.candleSource = undefined;
        source.dispose();
        this.candleSourceRebuildAt =
          Date.now() +
          Math.min(PAGE_CANDLE_REBUILD_MIN_MS * 2 ** this.candleSourceRebuilds, PAGE_CANDLE_REBUILD_MAX_MS);
        this.candleSourceRebuilds += 1;
      }
      throw error;
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    // Before its market list an engine refuses the symbol or sends an unmapped id.
    if (!(this.nativeLoaded & LOADED_FLAG.MARKETS)) await this.untilNativeData(LOADED_FLAG.MARKETS, 'markets');
    return this.command<Candle[]>(
      {
        op: 'fetchCandles',
        symbol: opts.symbol,
        interval: opts.interval,
        limit: opts.limit,
        from: opts.from,
        to: opts.to,
      },
      this.pageCandleSource ? { timeoutMs: PAGE_CANDLE_COMMAND_TIMEOUT_MS } : {},
    );
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions) => {
    const ready = this.capabilitiesFor('fetchHistoricalFills');
    const capabilities = ready instanceof Promise ? await ready : ready;
    const nonnegative = capabilities.fillHistoryTime === 'nonnegativeSecondsOrMilliseconds';
    if (
      capabilities.finiteHistoryBounds &&
      [opts.from, opts.to].some((value) => value !== undefined && !Number.isFinite(value))
    ) {
      return Promise.reject(new Error('Coinbase history bounds must be finite'));
    }
    const milliseconds = (value?: number) => {
      if (value === undefined || !Number.isFinite(value) || (nonnegative && value < 0)) return undefined;
      const alreadyMilliseconds = nonnegative ? value >= 1_000_000_000_000 : value > 1_000_000_000_000;
      return Math.floor(alreadyMilliseconds ? value : value * 1000);
    };
    return this.command<Execution[]>({
      op: 'fetchFills',
      ...opts,
      ...(capabilities.fillHistoryTime !== 'asProvided'
        ? { from: milliseconds(opts.from), to: milliseconds(opts.to) }
        : {}),
    });
  };

  resolveLeverageContext?: (symbol: string) => Promise<
    | {
        marginType: MarginType;
        positionMode: PositionMode;
        longLeverage: number;
        shortLeverage: number;
      }
    | undefined
  >;

  private resolvePhemexLeverageContext = async (
    symbol: string,
  ): ReturnType<NonNullable<RustEngineExchange['resolveLeverageContext']>> => {
    // Both hosted and browser workers already serve this raw leverage read.
    const mirrored = this.routesToWorker;
    const admitted = mirrored ? this.captureWorkerAdmission() : undefined;
    const args: [string, boolean] = [symbol, true];
    try {
      const rails = mirrored
        ? ((await this.callWorkerForOwner(admitted, 'fetchLeverageForSymbol', ...args)) as FetchedLeverage | null)
        : await this.command<FetchedLeverage | null>({ op: 'fetchLeverage', symbol }, { report: false });
      admitted?.('fetchLeverageForSymbol', args);
      if (this.isDisposed) return undefined;
      if (rails && rails.marginType !== undefined && rails.positionMode !== undefined) {
        return {
          marginType: rails.marginType,
          positionMode: rails.positionMode,
          longLeverage: rails.buyLeverage,
          shortLeverage: rails.sellLeverage,
        };
      }
    } catch (error) {
      admitted?.('fetchLeverageForSymbol', args);
      if ((error as { code?: string })?.code === 'WORKER_STOPPED') throw error;
      this.emitAndLogError(error instanceof Error ? error : String(error));
    }
    if (this.isDisposed) return undefined;
    const stored = this.store.positions.filter((position) => position.symbol === symbol);
    const marginType = stored[0]?.marginType;
    const positionMode = stored[0]?.positionMode;
    if (marginType === undefined || positionMode === undefined) return undefined;
    const long = stored.find((position) => position.side === PositionSide.Long) ?? stored[0];
    const short = stored.find((position) => position.side === PositionSide.Short) ?? stored[0];
    return { marginType, positionMode, longLeverage: Math.abs(long.leverage), shortLeverage: Math.abs(short.leverage) };
  };

  fetchLeverageForSymbol = async (symbol: string, fresh = false): Promise<FetchedLeverage | null> => {
    // Phemex metadata confirmation must bypass cached rails, including while flat.
    if (fresh && this.helperCapabilities.resolveLeverageContext === true) {
      if (this.routesToWorker) {
        const admitted = this.captureWorkerAdmission();
        const args: [string, boolean] = [symbol, true];
        const rails = await this.callWorkerForOwner(admitted, 'fetchLeverageForSymbol', ...args);
        admitted?.('fetchLeverageForSymbol', args);
        return rails as FetchedLeverage | null;
      }
      return this.command<FetchedLeverage | null>({ op: 'fetchLeverage', symbol }, { report: false });
    }
    const ready = this.capabilitiesFor('fetchLeverageForSymbol');
    const capabilities = ready instanceof Promise ? await ready : ready;
    try {
      this.host?.flush();
      const query = capabilities.leverageQuery;
      const cached = this.store.fetchedLeverage[symbol];
      const usable =
        cached &&
        Number.isFinite(cached.buyLeverage) &&
        cached.buyLeverage > 0 &&
        Number.isFinite(cached.sellLeverage) &&
        cached.sellLeverage > 0;
      if (query === 'settings') return usable ? cached : null;
      // Metadata-refreshed rails answer from settings once loaded; before the
      // first refresh (a flat, freshly selected symbol) the venue is read.
      if (query === 'onMetadata' && usable) return cached;
      // The venue read needs the engine; the worker's frames bring the result back.
      if (this.routesToWorker)
        return (await this.callWorker('fetchLeverageForSymbol', symbol)) as FetchedLeverage | null;
      const leverage = await this.command<FetchedLeverage>({ op: 'fetchLeverage', symbol }, { report: false });
      this.host?.flush();
      this.applier.applyLeverage(symbol, leverage);
      return leverage;
    } catch (error) {
      this.log(`Error fetching leverage for ${symbol}: ${error}`, LogSeverity.Warning);
      return null;
    }
  };

  fetchHistoricalOrders = async (opts?: OrderHistoryOptions) => {
    const ready = this.capabilitiesFor('fetchHistoricalOrders');
    const capabilities = ready instanceof Promise ? await ready : ready;
    if (capabilities.rejectOrderHistoryFilters && (opts?.orderId !== undefined || opts?.status !== undefined)) {
      return Promise.reject(
        new Error(
          `${this.clientOrderIdRule?.label ?? this.name} order history does not support orderId or status filters`,
        ),
      );
    }
    return this.command<HistoricalOrder[]>({
      op: 'fetchOrderHistory',
      symbol: opts?.symbol,
      from: opts?.from,
      to: opts?.to,
      limit: opts?.limit,
    });
  };

  fetchIncomeHistory = (opts?: IncomeHistoryOptions) =>
    this.command<IncomeRecord[]>({
      op: 'fetchIncome',
      symbol: opts?.symbol,
      incomeType: opts?.incomeType,
      from: opts?.startTime,
      to: opts?.endTime,
      limit: opts?.limit,
    });

  fetchIncomeHistoryRange = (startTime: number, endTime: number, incomeType?: string) =>
    this.command<IncomeRecord[]>({ op: 'fetchIncome', incomeType, from: startTime, to: endTime });

  /** The venue's raw closed-position rows, for the same TS mappers the TS venue uses. */
  private fetchClosedPositionRows = (opts: { symbol?: string; from?: number; to?: number }, report = true) =>
    this.routesToWorker
      ? (this.callWorker('fetchClosedPositions', opts) as Promise<Array<Record<string, any>>>)
      : this.command<Array<Record<string, any>>>(
          { op: 'fetchClosedPositions', symbol: opts.symbol, from: opts.from, to: opts.to },
          { report },
        );

  /** Worker side of a mirror's journal read: the raw rows, which the mirror maps. */
  fetchClosedPositions = (opts: { symbol?: string; from?: number; to?: number }) => this.fetchClosedPositionRows(opts);

  /**
   * Journal paths read the reported closed axis and THROW on a failed read, as TS does: an
   * under-counted history presented as complete is the failure the journal refuses.
   */
  private bybitJournal = async (opts: { account: string; from?: number; to?: number }) => {
    const to = opts.to ?? this.utcNow();
    const from = opts.from ?? to - 7 * 24 * 60 * 60 * 1000;
    if (this.isPublicDataOnly) return [];
    const rows = await this.fetchClosedPositionRows({ from, to }, false);
    // Closed-PnL rows carry no hedge leg; the order axis does (see bybit.exchange.ts).
    const legs = new Map<string, PositionSide | 'both'>();
    let orders: HistoricalOrder[];
    try {
      orders = await this.fetchHistoricalOrders({ from, to });
      for (const order of orders) {
        if (order?.id && order.positionSide) legs.set(String(order.id), order.positionSide);
      }
    } catch {
      throw new Error('bybit: could not read the order axis to resolve hedge legs');
    }
    const records = aggregateBybitClosedPnl(rows, {
      account: opts.account,
      resolveSettleCurrency: (symbol: string) => this.store.marketsBySymbol[symbol]?.quote || 'USDT',
      resolveLeg: legs.size ? (orderId: string) => legs.get(orderId) : undefined,
    });
    return copyHistoryCompleteness(orders, copyHistoryCompleteness(rows, records));
  };

  private okxJournal = async (opts: { account: string; from?: number; to?: number }) => {
    if (this.isPublicDataOnly) return [];
    const rows = await this.fetchClosedPositionRows({ from: opts.from, to: opts.to }, false);
    const records = [];
    for (const row of rows) {
      const market = this.store.marketsById[String(row.instId ?? '')];
      // As TS: inverse rows need a reference price the adapter lacks, so they are skipped.
      if (!market || market.isInverse) continue;
      const record = mapOkxPositionHistory(row, {
        account: opts.account,
        toBase: (contracts: number, instId: string) =>
          this.okxContractsToBase(contracts, this.store.marketsById[instId] ?? market),
        resolveSymbol: (instId: string) => this.store.marketsById[instId]?.symbol,
        resolveSettleCurrency: (instId: string) => this.store.marketsById[instId]?.quote,
      });
      if (record) records.push(record);
    }
    return copyHistoryCompleteness(rows, records);
  };

  private blofinJournal = async (opts: { account: string; from?: number; to?: number }) => {
    if (this.isPublicDataOnly) return [];
    const rows = await this.fetchClosedPositionRows({ from: opts.from, to: opts.to }, false);
    const records = [];
    for (const row of rows) {
      const record = mapBlofinPositionHistory(row, {
        account: opts.account,
        resolveSymbol: (instId: string) => this.store.marketsById[instId]?.symbol,
        resolveSettleCurrency: (instId: string) => this.store.marketsById[instId]?.quote,
      });
      if (record) records.push(record);
    }
    return copyHistoryCompleteness(rows, records);
  };

  /**
   * The reported closed axis TS `BitunixExchange.fetchHistoricalPositions` reads (fills fold to nothing).
   * Unlike TS, which returned [] or partial rows, a failed read throws under the journal rule above.
   */
  private bitunixJournal = async (opts: { account: string; from?: number; to?: number }) => {
    if (this.isPublicDataOnly) return [];
    const rows = await this.fetchClosedPositionRows({ from: opts.from, to: opts.to }, false);
    const records = [];
    for (const row of rows) {
      const record = mapBitunixPositionHistory(row, {
        account: opts.account,
        resolveSettleCurrency: (symbol: string) => this.store.marketsBySymbol[symbol]?.quote || 'USDT',
      });
      if (record) records.push(record);
    }
    return copyHistoryCompleteness(rows, records);
  };

  /** Reported rows only, newest close first; failures propagate so refreshes keep their last result. */
  private fetchReportedClosedPnl = async (opts: HistoricalTradeOptions): Promise<ClosedPnlEntry[]> => {
    if (this.routesToWorker) {
      return (await this.callWorker('fetchClosedPnl', opts)) as ClosedPnlEntry[];
    }
    if (this.isPublicDataOnly) return [];
    const ready = this.capabilitiesFor('fetchClosedPnl');
    const capabilities = ready instanceof Promise ? await ready : ready;
    const format = capabilities.closedPositionFormat;
    // OKX/Blofin treat an empty symbol as account-wide; Bybit and Bitunix send it as given.
    const symbol = format === 'okx' || format === 'blofin' ? opts.symbol || undefined : opts.symbol;
    return this.command<ClosedPnlEntry[]>({
      op: 'fetchClosedPnl', symbol, from: opts.from, to: opts.to, account: opts.accountName || '',
    });
  };

  /**
   * OKX `contractsToBase`. Engine markets carry no `ctValCcy`; an OKX inverse swap's
   * contract value is always quote-denominated, which is the case TS converts by price.
   */
  private okxContractsToBase = (contractsNative: number, market: Market, referencePrice?: number) => {
    const contractSize = market.contractSize || 1;
    if (!market.isInverse) return multiply(contractsNative, contractSize);
    const ticker = this.store.tickersById[market.id] ?? this.store.tickersBySymbol[market.symbol];
    const price = [referencePrice, ticker?.mark, ticker?.last].find(
      (value): value is number => typeof value === 'number' && Number.isFinite(value) && value > 0,
    );
    return price ? (contractsNative * contractSize) / price : 0;
  };

  fetchPublicTrades = (symbol: string, limit?: number) =>
    this.command<PublicTrade[]>({ op: 'fetchPublicTrades', symbol, limit });

  listenOrderBook = (
    symbol: string,
    callback: (book: OrderBook) => void,
    onRejection?: (signal: OrderBookFrameRejection) => void,
  ) => {
    const stopRejections = this.observeOrderBookFrameRejections(symbol, onRejection);
    if (onRejection) {
      const original = callback;
      callback = (book) => original(book);
    }
    let listeners = this.bookListeners.get(symbol);
    const first = !listeners;
    if (!listeners) {
      listeners = new Map();
      this.bookListeners.set(symbol, listeners);
    }
    const listenerId =
      (!this.bridgeConstruction.isolatedBookSubscriptions
        ? [...listeners].find(([, listener]) => listener === callback)?.[0]
        : undefined) ?? ++this.nextBookListenerId;
    listeners.set(listenerId, callback);
    const callbacks = listeners;
    if (first) this.bookDemand(symbol, true);
    return () => {
      stopRejections();
      if (!callbacks.delete(listenerId)) return;
      if (callbacks.size === 0 && this.bookListeners.get(symbol) === callbacks) {
        this.bookListeners.delete(symbol);
        this.bookDemand(symbol, false);
      }
    };
  };

  private bookDemand = (symbol: string, subscribe: boolean) => {
    if (!this.publicDemandReady || this.isDisposed) return;
    this.sharedPublicFeed?.send({ op: 'book', symbol, subscribe });
    this.subscriptionCommand({ op: subscribe ? 'subscribeOrderBook' : 'unsubscribeOrderBook', symbol });
  };

  /** Provider listeners still receive data when normal callbacks are handled by the frame consumer. */
  listenOrderBookForFeed = (symbol: string, callback: (book: OrderBook) => void) => {
    const listener = (book: OrderBook) => callback(book);
    this.feedBookListeners.add(listener);
    const stop = this.listenOrderBook(symbol, listener);
    return () => {
      this.feedBookListeners.delete(listener);
      stop();
    };
  };

  listenOHLCV = (
    opts: OHLCVOptions,
    callback: (candle: Candle) => void,
    onRejection?: (signal: OHLCVFrameRejection) => void,
  ) => {
    const stopRejections = this.observeOHLCVFrameRejections(opts, onRejection);
    if (onRejection) {
      const original = callback;
      callback = (candle) => original(candle);
    }
    const { interval } = opts;
    const key = `${opts.symbol}:${interval}`;
    let listeners = this.candleListeners.get(key);
    const first = !listeners;
    if (!listeners) {
      listeners = new Set();
      this.candleListeners.set(key, listeners);
    }
    listeners.add(callback);
    const callbacks = listeners;
    if (first) this.candleDemand(opts.symbol, interval, true);
    return () => {
      stopRejections();
      callbacks.delete(callback);
      if (callbacks.size === 0 && this.candleListeners.get(key) === callbacks) {
        this.candleListeners.delete(key);
        this.candleDemand(opts.symbol, interval, false);
      }
    };
  };

  private candleDemand = (symbol: string, interval: OHLCVOptions['interval'], subscribe: boolean) => {
    if (!this.publicDemandReady || this.isDisposed) return;
    this.sharedPublicFeed?.send({ op: 'candle', symbol, interval, subscribe });
    this.subscriptionCommand({ op: subscribe ? 'subscribeCandles' : 'unsubscribeCandles', symbol, interval });
  };

  /** The engine re-sends the venue's subscribe for this live stream; a venue that cannot logs why. */
  repairOHLCV = (opts: Pick<OHLCVOptions, 'symbol' | 'interval'>) => {
    const unsent = !this.publicDemandReady
      ? 'public data is not ready'
      : !this.candleListeners.has(`${opts.symbol}:${opts.interval}`) && 'no candle listener';
    if (unsent) {
      this.log(`${this.name} candles ${opts.symbol} ${opts.interval} repair not sent: ${unsent}`, LogSeverity.Warning);
      return;
    }
    // A shared-feed account holds no public topics; its source's socket carries the stream.
    if (this.sharedPublicFeed) {
      this.sharedPublicFeed.send({ op: 'repair', symbol: opts.symbol, interval: opts.interval });
      return;
    }
    this.subscriptionCommand({
      op: 'repairSubscription',
      topic: { kind: 'candles', symbol: opts.symbol, interval: opts.interval },
    });
  };

  listenOHLCVForFeed = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const listener = (candle: Candle) => callback(candle);
    this.feedCandleListeners.add(listener);
    const stop = this.listenOHLCV(opts, listener);
    return () => {
      this.feedCandleListeners.delete(listener);
      stop();
    };
  };

  dispose() {
    this.stopTimeSync?.();
    this.stopTimeSync = undefined;
    clearTimeout(this.sourceOrdersTimer);
    this.sourceOrdersTimer = undefined;
    this.sourceOrdersAsk = undefined;
    this.settleSourceOrders();
    this.candleSource?.dispose();
    this.candleSource = undefined;
    this.algoControlWatch?.dispose();
    this.signerSetup?.dispose();
    this.stopWorkerCapabilitySync?.();
    this.stopWorkerCapabilitySync = undefined;
    this.sharedPublicFeed?.dispose();
    this.sharedPublicFeed = undefined;
    this.stopInProcessFeed?.();
    this.stopInProcessFeed = undefined;
    for (const observer of this.publicFeedObservers) this.dispatchFrameEvent(() => observer.closed());
    this.publicFeedObservers.clear();
    this.feedBookListeners.clear();
    this.feedCandleListeners.clear();
    this.stopPollingConfig?.();
    this.stopPollingConfig = undefined;
    this.flushTimers.forEach(clearInterval);
    this.flushTimers = [];
    const host = this.host;
    this.host = undefined;
    // Running chases cancel their resting orders first (bounded by the engine).
    if (host?.algosRunning) {
      let stopping: Promise<void>;
      try {
        stopping = host.algoDispose('account disposed');
      } catch {
        stopping = Promise.resolve();
      }
      void stopping.finally(() => host.dispose());
    } else host?.dispose();
    // Ops still waiting for the engine report it before the error listeners go.
    this.replayQueuedAlgoOps();
    super.dispose();
    for (const wake of this.startupWaiters) wake();
  }
}
