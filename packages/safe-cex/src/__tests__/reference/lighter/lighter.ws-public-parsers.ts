export function shouldDispatchLighterPublicMessage(topic: string, data: string): boolean {
  const channelVariants = new Set<string>([topic, topic.replace(/:/g, '/'), topic.replace(/\//g, ':')]);

  const hasMatchingChannel = Array.from(channelVariants).some((channel) => data.includes(`"channel":"${channel}"`));
  const isTickerPerMarketChannel =
    topic === 'ticker' && (data.includes('"channel":"ticker:') || data.includes('"channel":"ticker/'));

  return hasMatchingChannel || isTickerPerMarketChannel || (topic === 'pong' && data.includes('pong'));
}

export function parseLighterWsNumber(...values: any[]): number | undefined {
  for (const value of values) {
    if (typeof value === 'number' && Number.isFinite(value)) {
      return value;
    }
    if (typeof value === 'string' && value.length > 0) {
      const parsed = parseFloat(value);
      if (Number.isFinite(parsed)) {
        return parsed;
      }
    }
  }
  return undefined;
}

export function parseLighterWsBookSidePrice(value: any): number | undefined {
  if (!value) {
    return undefined;
  }

  // Compact payloads can encode side as [price, size] tuple.
  if (Array.isArray(value) && value.length > 0) {
    return parseLighterWsNumber(value[0]);
  }

  return parseLighterWsNumber(value, value.price, value.p, value.bestPrice, value.best_price);
}

export function extractLighterTickerMarketIdFromChannel(channel: string): string | undefined {
  return String(channel || '').match(/ticker[:/](\d+)/)?.[1];
}
