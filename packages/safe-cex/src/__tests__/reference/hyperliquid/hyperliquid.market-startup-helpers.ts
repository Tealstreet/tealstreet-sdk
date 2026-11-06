import type { Axios } from 'axios';

import { LogSeverity } from '../../../types';
import { AssetCtx, ENDPOINTS, INFO_TYPES, Meta } from '../../../exchanges/hyperliquid/hyperliquid.types';

type LogFn = (message: string, severity?: LogSeverity) => void;

const INFO_REQUEST_CONFIG = {
  headers: {
    'Content-Type': 'application/json',
  },
};

/**
 * Validate and parse Hyperliquid [meta, assetCtxs] response tuples.
 */
const parseMetaAndAssetCtxsResponse = (
  data: unknown,
  sourceLabel: string,
  contextLabel: string,
  log: LogFn,
): [Meta, AssetCtx[]] | null => {
  if (!Array.isArray(data)) {
    log(
      `Invalid ${contextLabel} response from ${sourceLabel}: expected tuple array, got ${typeof data}`,
      LogSeverity.Warning,
    );
    return null;
  }

  const [metaCandidate, assetCtxsCandidate] = data as [unknown, unknown];
  const hasUniverse = Array.isArray((metaCandidate as { universe?: unknown[] } | null | undefined)?.universe);
  const hasAssetCtxs = Array.isArray(assetCtxsCandidate);

  if (!hasUniverse || !hasAssetCtxs) {
    log(
      `Invalid ${contextLabel} shape from ${sourceLabel}: tupleLength=${data.length}, hasUniverse=${hasUniverse}, hasAssetCtxs=${hasAssetCtxs}`,
      LogSeverity.Warning,
    );
    return null;
  }

  return [metaCandidate as Meta, assetCtxsCandidate as AssetCtx[]];
};

export const fetchCoreMetaAndAssetCtxsWithFallback = async ({
  infoXhr,
  xhr,
  log,
}: {
  infoXhr: Axios;
  xhr: Axios;
  log: LogFn;
}): Promise<[Meta, AssetCtx[]]> => {
  const requestBody = { type: INFO_TYPES.PERPS_META_AND_ASSET_CTXS };

  const fetchFromSource = async (
    client: Axios,
    sourceLabel: string,
    shouldLogRetryHint = false,
  ): Promise<[Meta, AssetCtx[]] | null> => {
    try {
      const { data } = await client.post<[Meta, AssetCtx[]]>(ENDPOINTS.INFO, requestBody, INFO_REQUEST_CONFIG);
      const parsed = parseMetaAndAssetCtxsResponse(data, sourceLabel, 'core markets', log);
      if (!parsed && shouldLogRetryHint) {
        log(`Retrying core markets fetch via fallback due to malformed response`, LogSeverity.Warning);
      }
      return parsed;
    } catch (error) {
      log(`Failed to fetch core markets via ${sourceLabel}: ${error}`, LogSeverity.Warning);
      return null;
    }
  };

  const primary = await fetchFromSource(infoXhr, 'primary', true);
  if (primary) {
    return primary;
  }

  const fallback = await fetchFromSource(xhr, 'fallback');
  if (fallback) {
    return fallback;
  }

  throw new Error('Failed to fetch valid core markets response from primary and fallback sources');
};

export const fetchHip3MetaAndAssetCtxsWithRetry = async ({
  xhr,
  dexName,
  log,
}: {
  xhr: Axios;
  dexName: string;
  log: LogFn;
}): Promise<[Meta, AssetCtx[]] | null> => {
  const requestBody = { type: INFO_TYPES.PERPS_META_AND_ASSET_CTXS, dex: dexName };

  const fetchDexMetaAndAssetCtxs = async (attempt: number): Promise<[Meta, AssetCtx[]] | null> => {
    try {
      const { data } = await xhr.post<[Meta, AssetCtx[]]>(ENDPOINTS.INFO, requestBody, INFO_REQUEST_CONFIG);
      const parsed = parseMetaAndAssetCtxsResponse(
        data,
        `official API (attempt ${attempt})`,
        `HIP-3 markets (${dexName})`,
        log,
      );
      if (!parsed && attempt === 1) {
        log(`Retrying HIP-3 markets fetch for ${dexName} due to malformed response`, LogSeverity.Warning);
      }
      return parsed;
    } catch (error) {
      const attemptLabel = attempt === 1 ? 'initial attempt' : 'retry';
      log(`Failed to fetch HIP-3 markets for ${dexName} (${attemptLabel}): ${error}`, LogSeverity.Warning);
      return null;
    }
  };

  const parsedResponse = (await fetchDexMetaAndAssetCtxs(1)) || (await fetchDexMetaAndAssetCtxs(2));
  if (!parsedResponse) {
    log(`Failed to fetch valid HIP-3 markets for ${dexName} after retry`, LogSeverity.Warning);
    return null;
  }

  return parsedResponse;
};
