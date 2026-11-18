import type { EngineEvent } from '@tealstreet/safe-cex-wasm/host';

const REQUEST_PATH_PARTS = new Set(
  'api v1 v2 v3 v4 v5 fapi dapi sapi openApi swap spot market public private account trade orders order positions position balance balances wallet instruments-info tickers realtime list info all wallet-balance fee-rate config funds assets time order-history cancel-all account-info batch instruments pending-algo asset orders-pending orders-tpsl-pending orders-algo-pending margin-mode position-mode batch-leverage-info :segment :unknown :more'.split(
    ' ',
  ),
);

export type FailedPrivateReadKind = 'state' | 'spot' | 'orders' | 'role' | 'mode' | 'discovery';

export function failedPrivateReadKind(value: unknown): FailedPrivateReadKind | undefined {
  return typeof value === 'string' && ['state', 'spot', 'orders', 'role', 'mode', 'discovery'].includes(value)
    ? (value as FailedPrivateReadKind)
    : undefined;
}

const STAGES = new Set([
  'metaAndAssetCtxs',
  'perpDexs',
  'spotMetaAndAssetCtxs',
  'userRole',
  'userAbstraction',
  'frontendOpenOrders',
  'clearinghouseState',
  'spotClearinghouseState',
  'tickers',
]);

/** Closed stage names and counters only; request bodies and wallet identities never become tags. */
export function startupDiagnosticTags(event: EngineEvent): Record<string, string> {
  if (event.code === 'request.failed' && typeof event.fields?.httpPath === 'string') {
    const { httpMethod, httpPath, httpPhase } = event.fields;
    if (
      !['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS', 'OTHER'].includes(String(httpMethod)) ||
      !['preparing', 'prepared', 'bound', 'released'].includes(String(httpPhase)) ||
      httpPath.length > 160 ||
      !httpPath.startsWith('/') ||
      !httpPath
        .slice(1)
        .split('/')
        .every((part) => REQUEST_PATH_PARTS.has(part))
    )
      return {};
    return {
      http_request_method: String(httpMethod),
      http_request_path: httpPath,
      http_request_phase: String(httpPhase),
    };
  }
  if (event.code !== 'startup.incomplete') return {};
  const fields = event.fields ?? {};
  const tags: Record<string, string> = {};
  for (const section of ['Markets', 'Tickers', 'Balance', 'Orders', 'Positions']) {
    const prefix = `startup${section}`;
    const tag = `startup_${section.toLowerCase()}`;
    const state = fields[`${prefix}State`];
    if (typeof state !== 'string' || !['neverStarted', 'started', 'timedOut', 'refused', 'failed'].includes(state))
      continue;
    tags[`${tag}_state`] = state;
    const kind = fields[`${prefix}Kind`];
    if (
      typeof kind === 'string' &&
      [
        'rejected',
        'invalidInput',
        'insufficientBalance',
        'authInvalid',
        'authExpired',
        'authPermission',
        'rateLimited',
        'network',
        'timeout',
        'venueUnavailable',
        'unsupported',
        'internal',
      ].includes(kind)
    )
      tags[`${tag}_kind`] = kind;
    const code = fields[`${prefix}VenueCode`];
    if (typeof code === 'string' && /^[A-Za-z0-9_:.\-]{1,32}$/.test(code)) tags[`${tag}_venue_code`] = code;
    const status = fields[`${prefix}HttpStatus`];
    if (typeof status === 'number' && Number.isInteger(status) && status >= 100 && status <= 599)
      tags[`${tag}_http_status`] = String(status);
  }

  if (typeof fields.stage === 'string' && STAGES.has(fields.stage)) tags.startup_stage = fields.stage;
  const failedRead = failedPrivateReadKind(fields.lastFailedPrivateReadKind);
  if (failedRead) tags.startup_last_failed_private_read_kind = failedRead;
  for (const [field, tag] of [
    ['elapsedMs', 'startup_elapsed_ms'],
    ['deadlineMs', 'startup_deadline_ms'],
    ['readTimeoutMs', 'startup_read_timeout_ms'],
    ['http429Count', 'startup_http_429_count'],
  ]) {
    const value = fields[field];
    if (typeof value === 'number' && Number.isFinite(value) && value >= 0 && value <= Number.MAX_SAFE_INTEGER)
      tags[tag] = String(Math.floor(value));
  }
  return tags;
}
