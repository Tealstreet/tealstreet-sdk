/**
 * Resolve the margin currency for a market. Re-export of safe-cex's vetted
 * helper. The script-api narrows the signature so the market arg is
 * required (matches SPEC §2.1) — safe-cex's underlying form accepts
 * `market?` with a fallback string.
 */
import type { Market } from '@tealstreet/safe-cex/src/types';
import { getMarginCurrency as safeGetMarginCurrency } from '@tealstreet/safe-cex/src/utils/sizing';

export const getMarginCurrency = (market: Market): string => safeGetMarginCurrency(market);
