/**
 * Post-processor registry.
 *
 * Add new post-processors to ONE of these two lists:
 *
 * - independentPostprocessors: Only need (accountName, exchangeName, options).
 *   Run client-side in hosted mode, alongside everything else in non-hosted mode.
 *
 * - exchangeDependentPostprocessors: Need the live Exchange instance or
 *   process-local singletons (e.g. twapManager, chaserOrderManager).
 *   Run server-side in hosted mode, alongside everything else in non-hosted mode.
 */
import { BasePostprocessor } from './BasePostprocessor';
import { ChaserOrderEnrichmentPostprocessor } from './ChaserOrderEnrichmentPostprocessor';
import { ChaserOrderPostprocessor } from './ChaserOrderPostprocessor';
import { DisplayedPositionsPostprocessor } from './DisplayedPositionsPostprocessor';
import { ExchangeDependentPostprocessor } from './ExchangeDependentPostprocessor';
import { DisplayedMarginPostprocessor } from './DisplayedMarginPostprocessor';
import { MessageHandlerStatsPostprocessor } from './MessageHandlerStatsPostprocessor';
import { OrdersRiskProfilePostprocessor } from './OrdersRiskProfilePostprocessor';
import { TwapOrderEnrichmentPostprocessor } from './TwapOrderEnrichmentPostprocessor';
import { TwapPostprocessor } from './TwapPostprocessor';

export const independentPostprocessors: (typeof BasePostprocessor)[] = [
  // DisplayedMarginPostprocessor reads displayedPositions, so it must run after
  // DisplayedPositionsPostprocessor.
  DisplayedPositionsPostprocessor,
  DisplayedMarginPostprocessor,
  ChaserOrderEnrichmentPostprocessor,
  TwapOrderEnrichmentPostprocessor,
  OrdersRiskProfilePostprocessor,
];

// Hosts add their own independent post-processors (Tealstreet's account
// snapshot upload lives in apps/web). Appended, so built-in order is kept.
export const registerIndependentPostprocessor = (Cls: typeof BasePostprocessor): void => {
  if (!independentPostprocessors.includes(Cls)) independentPostprocessors.push(Cls);
};

export const exchangeDependentPostprocessors: (typeof ExchangeDependentPostprocessor)[] = [
  ChaserOrderPostprocessor,
  TwapPostprocessor,
  MessageHandlerStatsPostprocessor,
];
