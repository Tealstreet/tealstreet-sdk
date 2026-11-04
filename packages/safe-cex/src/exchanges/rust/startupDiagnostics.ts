import type { EngineEvent } from '@tealstreet/safe-cex-wasm/host';

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
  if (event.code !== 'startup.incomplete') return {};
  const fields = event.fields ?? {};
  const tags: Record<string, string> = {};
  if (typeof fields.stage === 'string' && STAGES.has(fields.stage)) tags.startup_stage = fields.stage;
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
