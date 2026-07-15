import { inflateRaw } from 'pako';

export type HyperliquidFastAssetCtx = {
  markPx?: string;
  midPx?: string;
};

export type HyperliquidFastAssetCtxs = Record<string, HyperliquidFastAssetCtx>;

/**
 * `fastAssetCtxs` is a base64-encoded raw-DEFLATE JSON object. The initial
 * frame is a full snapshot; later frames contain only changed coins/fields.
 */
export const decodeHyperliquidFastAssetCtxs = (payload: string): HyperliquidFastAssetCtxs => {
  if (typeof payload !== 'string' || payload.length === 0) {
    throw new Error('Invalid Hyperliquid fastAssetCtxs payload');
  }

  const compressed = Uint8Array.from(atob(payload), (character) => character.charCodeAt(0));
  const decoded = JSON.parse(new TextDecoder().decode(inflateRaw(compressed))) as unknown;

  if (!decoded || Array.isArray(decoded) || typeof decoded !== 'object') {
    throw new Error('Invalid Hyperliquid fastAssetCtxs data');
  }

  return decoded as HyperliquidFastAssetCtxs;
};
