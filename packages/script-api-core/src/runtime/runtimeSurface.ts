import type { ReadMethodName, TargetName, WriteMethodName } from '../api/capabilities.types';

export type RunnableTargetName = Exclude<TargetName, 'core'>;
export type ApiPath = `api.${string}`;

export interface RuntimeSurfaceDescriptor {
  readonly target: TargetName;
  readonly label: string;
  readonly apiPaths: readonly ApiPath[];
}

export interface ApiPathAvailability {
  readonly path: ApiPath;
  readonly allowed: boolean;
  readonly excludedBy: readonly RunnableTargetName[];
  readonly alternatives: readonly ApiPath[];
}

const readPath = (method: ReadMethodName): ApiPath => `api.${method}`;
const writePath = (method: WriteMethodName): ApiPath => `api.exchange.${method}`;

export const CORE_READ_API_PATHS = Object.freeze([
  readPath('getActiveAccount'),
  readPath('getActiveAccounts'),
  readPath('getActiveSymbol'),
  readPath('getActiveExchange'),
  readPath('getAccounts'),
  readPath('getMarkets'),
  readPath('getMarket'),
  readPath('getTicker'),
  readPath('getTickers'),
  readPath('getPositions'),
  readPath('getHistoricalPosition'),
  readPath('getOrders'),
  readPath('getBalance'),
  readPath('getCombinedBalance'),
  readPath('getOrderbook'),
  readPath('getCandles'),
] as const);

export const CORE_WRITE_API_PATHS = Object.freeze([
  writePath('placeOrder'),
  writePath('placeOrders'),
  writePath('cancelOrder'),
  writePath('cancelOrders'),
  writePath('updateOrder'),
  writePath('closePosition'),
  writePath('setLeverage'),
  writePath('setPositionMode'),
  writePath('setMarginType'),
  writePath('runCommand'),
] as const);

/**
 * The OEMS slice — the order-flow subset an agent-driven caller may use when
 * it is ACTING on the user's behalf rather than rendering a panel.
 *
 * This is a slice of the surface above, not a fourth API. It is deliberately
 * NOT an entry in `RUNTIME_SURFACES`: that record is keyed by `TargetName`,
 * whose members feed exhaustive `Record<TargetName, …>` tables in
 * `targetCheck.ts` and the `RunnableTargetName` exclusion. OEMS is a
 * CAPABILITY narrowing, not a place code runs, and conflating the two would
 * make every target-shaped table grow a member that can never be a target.
 *
 * Three writes are excluded and the exclusions are the point:
 * `setPositionMode` and `setMarginType` reconfigure an ACCOUNT rather than
 * work an order, and `runCommand` is the REPL escape hatch — a planner that
 * can reach it can reach every verb this slice was drawn to exclude.
 */
export const OEMS_WRITE_API_PATHS = Object.freeze([
  writePath('placeOrder'),
  writePath('placeOrders'),
  writePath('cancelOrder'),
  writePath('cancelOrders'),
  writePath('updateOrder'),
  writePath('closePosition'),
  writePath('setLeverage'),
] as const satisfies readonly ApiPath[]);

/**
 * What an OEMS caller may read. Candles, orderbooks and historical positions
 * are absent: they are analysis inputs, and a planner that has to SPEND
 * CONTEXT describing its own surface pays for every path it never uses.
 */
export const OEMS_READ_API_PATHS = Object.freeze([
  readPath('getActiveAccount'),
  readPath('getActiveAccounts'),
  readPath('getActiveSymbol'),
  readPath('getAccounts'),
  readPath('getMarkets'),
  readPath('getMarket'),
  readPath('getTicker'),
  readPath('getPositions'),
  readPath('getOrders'),
  readPath('getBalance'),
] as const satisfies readonly ApiPath[]);

export const OEMS_API_PATHS = Object.freeze([
  ...OEMS_READ_API_PATHS,
  ...OEMS_WRITE_API_PATHS,
] as const satisfies readonly ApiPath[]);

const PORTABLE_UTILITY_API_PATHS = Object.freeze([
  'api.io',
  'api.io.print',
  'api.io.return',
  'api.utils',
  'api.utils.sleep',
  'api.utils.uuid',
  'api.utils.adjust',
  'api.utils.formatPnl',
  'api.utils.calculateReturns',
  'api.utils.getMarginCurrency',
  'api.utils.convertSymbol',
  'api.utils.getTickerPredictedBestBid',
  'api.utils.getTickerPredictedBestAsk',
  'api.utils.getBBOMid',
  'api.utils.getNearTouchPrice',
  'api.utils.getFarTouchPrice',
  'api.utils.getCandleStartTime',
  'api.utils.getCandleEndTime',
  'api.utils.getNumCandlesInRange',
  'api.utils.normalizeEpochToSeconds',
  'api.constants',
  'api.constants.ExchangeName',
  'api.constants.OrderSide',
  'api.constants.OrderType',
  'api.constants.PositionSide',
  'api.constants.PositionMode',
  'api.constants.OrderStatus',
  'api.constants.OrderTimeInForce',
  'api.constants.OrderStopTriggerType',
  'api.constants.MarginType',
  'api.version',
  'api.meta',
  'api.meta.scriptId',
  'api.meta.scriptName',
  'api.meta.invokedFrom',
  'api.meta.targets',
  'api.meta.capabilities',
  'api.meta.loopSymbol',
] as const satisfies readonly ApiPath[]);

const CORE_READ_ONLY_API_PATHS = Object.freeze([
  ...CORE_READ_API_PATHS,
  ...PORTABLE_UTILITY_API_PATHS,
] as const satisfies readonly ApiPath[]);

export const CORE_SHARED_API_PATHS = Object.freeze([
  ...CORE_READ_ONLY_API_PATHS,
  ...CORE_WRITE_API_PATHS,
  'api.exchange',
  'api.storage',
  'api.storage.get',
  'api.storage.set',
  'api.storage.delete',
  'api.storage.list',
] as const satisfies readonly ApiPath[]);

export const MOBILE_READ_API_PATHS = CORE_READ_API_PATHS;
export const CLI_READ_API_PATHS = Object.freeze([
  readPath('getActiveAccount'),
  readPath('getActiveAccounts'),
  readPath('getActiveSymbol'),
  readPath('getActiveExchange'),
  readPath('getAccounts'),
  readPath('getMarkets'),
  readPath('getMarket'),
  readPath('getTicker'),
  readPath('getTickers'),
  readPath('getPositions'),
  readPath('getOrders'),
  readPath('getBalance'),
  readPath('getCombinedBalance'),
  readPath('getOrderbook'),
  readPath('getCandles'),
] as const);

export const PORTABLE_READ_ONLY_API_PATHS = Object.freeze([
  ...CLI_READ_API_PATHS,
  ...PORTABLE_UTILITY_API_PATHS,
] as const satisfies readonly ApiPath[]);

const MOBILE_SHARED_API_PATHS = Object.freeze([
  ...MOBILE_READ_API_PATHS,
  ...PORTABLE_UTILITY_API_PATHS,
] as const satisfies readonly ApiPath[]);

const CLI_SHARED_API_PATHS = Object.freeze([
  ...CLI_READ_API_PATHS,
  ...PORTABLE_UTILITY_API_PATHS,
] as const satisfies readonly ApiPath[]);

const WEB_EXTENSION_API_PATHS = Object.freeze([
  'api.react',
  'api.components',
  'api.ui',
  'api.ui.getClick',
  'api.ui.getOrderSize',
  'api.ui.getQuickSize',
  'api.domHotkey',
  'api.domHotkey.register',
] as const satisfies readonly ApiPath[]);

/**
 * The version of the `cli` SURFACE, which is what `validateTargets` compares a
 * script's `cliApiVersion` stamp against.
 *
 * It lived in `@tealstreet/script-api-cli` until that package was deleted. The
 * package was scaffolding and stayed scaffolding: three of its four files were
 * `export {}` placeholders for a phase that never landed, and this constant —
 * still `0.0.0` — was the only thing it exported. `api.io.print` / `api.io.return`,
 * the one surface the SPEC earmarked for it, were moved into core early on
 * because web inline invocation needs them too, and nothing CLI-specific ever
 * replaced them.
 *
 * The TARGET is not going anywhere: `RUNTIME_SURFACES.cli` below is real,
 * `apps/cli` mounts it, and a script may declare `targets: ['cli']`. Only its
 * empty package is gone, which leaves `cli` shaped exactly like `mobile` —
 * a target described entirely from core.
 *
 * **`validateTargets` REQUIRES a non-empty string here** for any target it
 * checks, so deleting this rather than moving it would make every
 * `targets: ['cli']` script fail to load with "runtime provides no 'cli' API
 * version". Bump it when a genuinely CLI-only script API lands; at that point
 * it also needs a `check:api-versions` entry naming its new surface files.
 */
export const CLI_API_VERSION = '0.0.0';

export const RUNTIME_SURFACES = Object.freeze({
  core: Object.freeze({
    target: 'core',
    label: 'core',
    apiPaths: PORTABLE_READ_ONLY_API_PATHS,
  } satisfies RuntimeSurfaceDescriptor),
  web: Object.freeze({
    target: 'web',
    label: 'web',
    apiPaths: Object.freeze([...CORE_READ_ONLY_API_PATHS, ...WEB_EXTENSION_API_PATHS]),
  } satisfies RuntimeSurfaceDescriptor),
  cli: Object.freeze({
    target: 'cli',
    label: 'CLI',
    apiPaths: CLI_SHARED_API_PATHS,
  } satisfies RuntimeSurfaceDescriptor),
  mobile: Object.freeze({
    target: 'mobile',
    label: 'mobile',
    apiPaths: MOBILE_SHARED_API_PATHS,
  } satisfies RuntimeSurfaceDescriptor),
});

export const RUNNABLE_TARGETS = Object.freeze(['web', 'mobile', 'cli'] as const);

const RUNTIME_TARGET_SET = new Set<TargetName>(RUNNABLE_TARGETS);
const ALTERNATIVES: Readonly<Record<string, readonly ApiPath[]>> = Object.freeze({
  'api.getHistoricalPosition': ['api.getPositions'],
  'api.getOrderbook': ['api.getTicker'],
  'api.getCandles': ['api.getTicker', 'api.getOrderbook'],
  'api.storage': ['api.io.return'],
  'api.storage.get': ['api.io.return'],
  'api.storage.set': ['api.io.return'],
  'api.storage.delete': ['api.io.return'],
  'api.storage.list': ['api.io.return'],
  'api.exchange': ['api.getOrders', 'api.getPositions', 'api.getTicker'],
  'api.exchange.placeOrder': ['api.getOrders', 'api.getPositions', 'api.getTicker'],
  'api.exchange.placeOrders': ['api.getOrders', 'api.getPositions', 'api.getTicker'],
  'api.exchange.cancelOrder': ['api.getOrders'],
  'api.exchange.cancelOrders': ['api.getOrders'],
  'api.exchange.updateOrder': ['api.getOrders'],
  'api.exchange.closePosition': ['api.getPositions'],
  'api.exchange.setLeverage': ['api.getPositions'],
  'api.exchange.setPositionMode': ['api.getPositions'],
  'api.exchange.setMarginType': ['api.getPositions'],
  'api.exchange.runCommand': ['api.getOrders', 'api.getPositions', 'api.getTicker'],
  'api.react': ['api.io.return'],
  'api.components': ['api.io.return'],
  'api.ui': ['api.getTicker', 'api.getOrderbook'],
  'api.ui.getClick': ['api.getTicker', 'api.getOrderbook'],
  'api.ui.getOrderSize': ['api.getBalance', 'api.getPositions'],
  'api.ui.getQuickSize': ['api.getBalance', 'api.getPositions'],
  'api.domHotkey': ['api.io.return'],
  'api.domHotkey.register': ['api.io.return'],
});

const KNOWN_API_PATHS = new Set<ApiPath>([
  ...Object.values(RUNTIME_SURFACES).flatMap((surface) => surface.apiPaths),
  ...(Object.keys(ALTERNATIVES) as ApiPath[]),
]);

/**
 * Narrow a target list to the ones a thing can actually RUN on, treating an
 * empty result as "all of them" rather than "none".
 *
 * Exported because `@tealstreet/module-api-runtime` needs the identical rule
 * for the module vocabulary and had a byte-identical copy of it. The two
 * vocabularies describe different api objects — `api.exchange.placeOrder` is
 * not `api.orders.placeMarketOrder` — so their PATH SETS stay separate, but
 * "which targets does this list mean" is one question with one answer, and two
 * copies of it can only drift apart.
 */
export const narrowToRunnableTargets = (targets: readonly TargetName[]): readonly RunnableTargetName[] => {
  const runnable = targets.filter((target): target is RunnableTargetName => RUNTIME_TARGET_SET.has(target));
  return runnable.length > 0 ? runnable : RUNNABLE_TARGETS;
};

export const getAllowedApiPathsForTargets = (targets: readonly TargetName[]): ReadonlySet<ApiPath> => {
  const runnable = narrowToRunnableTargets(targets);
  const [first, ...rest] = runnable;
  let allowed = new Set<ApiPath>(RUNTIME_SURFACES[first].apiPaths);
  for (const target of rest) {
    const next = new Set<ApiPath>(RUNTIME_SURFACES[target].apiPaths);
    allowed = new Set([...allowed].filter((path) => next.has(path)));
  }
  return allowed;
};

export const isKnownApiPath = (path: string): path is ApiPath => KNOWN_API_PATHS.has(path as ApiPath);

export const getApiPathAvailability = (path: ApiPath, targets: readonly TargetName[]): ApiPathAvailability => {
  const runnable = narrowToRunnableTargets(targets);
  const excludedBy = runnable.filter((target) => !RUNTIME_SURFACES[target].apiPaths.includes(path));
  return {
    path,
    allowed: excludedBy.length === 0,
    excludedBy,
    alternatives: ALTERNATIVES[path] ?? [],
  };
};
