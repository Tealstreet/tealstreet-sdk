import { canonicalUrl, subscriptionKeys } from './requests.mjs';

// Known TS defect: only the first 100 detailed feeds are subscribed. Expect
// the documented all-contract mini feed as well, without changing any prices,
// detailed subscription, command, request or Rust result.
export function withMexcMiniTickerSubscription(subscriptions) {
  const expected = new Set(subscriptions);
  const url = 'wss://contract.mexc.com/edge';
  if (subscriptions.some((key) => key.startsWith(`WS ${canonicalUrl(url)} `) && key.includes('"method":"sub.ticker"'))) {
    for (const key of subscriptionKeys(url, { method: 'sub.tickers', param: {}, gzip: false })) expected.add(key);
  }
  return [...expected].sort();
}
