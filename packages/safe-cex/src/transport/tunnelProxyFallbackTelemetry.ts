import type { ExchangeName } from '../types';
import type { TunnelProxyFallbackNotice } from './engine';

const bucket = (value: number, edges: readonly number[]): string => {
  for (let index = 0; index < edges.length; index += 1) {
    if (value < edges[index]) return index === 0 ? `<${edges[0]}` : `${edges[index - 1]}-${edges[index]}`;
  }
  return `${edges[edges.length - 1]}+`;
};

/**
 * One static Sentry message per account and outage, closed-set tags only, so every
 * activation counts and a tunnel problem stays visible behind the fallback.
 */
export const tunnelProxyFallbackReport = (exchange: ExchangeName, notice: TunnelProxyFallbackNotice) => {
  const message = notice.skipped
    ? 'Tunnel unreachable; REST stays on the tunnel'
    : 'Tunnel unreachable; REST on the HTTP proxy';
  return {
    message,
    sentryData: {
      error: { name: 'TunnelProxyFallback', message },
      tags: {
        exchange,
        tunnel_proxy_fallback: notice.skipped ?? 'proxy',
        tunnel_ports_tried: notice.outage.portsTried,
        tunnel_failures: bucket(notice.outage.failures, [3, 5, 9]),
        tunnel_first_failure_s: bucket(Math.floor(notice.outage.sinceFirstFailureMs / 1000), [10, 20, 40, 80]),
      },
    },
  };
};
