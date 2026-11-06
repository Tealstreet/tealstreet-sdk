import { describe, expect, it } from 'vitest';

import { tunnelProxyFallbackReport } from './tunnelProxyFallbackTelemetry';

const outage = { episode: 3, portsTried: '8443+443' as const, failures: 4, sinceFirstFailureMs: 14_500 };

describe('tunnelProxyFallbackReport', () => {
  it('reports an activation with a static message and closed-set tags', () => {
    expect(tunnelProxyFallbackReport('bybit', { outage })).toEqual({
      message: 'Tunnel unreachable; REST on the HTTP proxy',
      sentryData: {
        error: { name: 'TunnelProxyFallback', message: 'Tunnel unreachable; REST on the HTTP proxy' },
        tags: {
          exchange: 'bybit',
          tunnel_proxy_fallback: 'proxy',
          tunnel_ports_tried: '8443+443',
          tunnel_failures: '3-5',
          tunnel_first_failure_s: '10-20',
        },
      },
    });
  });

  it('tags a venue the proxy fleet refuses with its own reason', () => {
    const report = tunnelProxyFallbackReport('kraken', { outage, skipped: 'proxy_not_allowlisted' });
    expect(report.message).toBe('Tunnel unreachable; REST stays on the tunnel');
    expect(report.sentryData.tags.tunnel_proxy_fallback).toBe('proxy_not_allowlisted');
  });
});
