import { describe, expect, it } from 'vitest';

import {
  exchangesRequireWsIp,
  privateProxyIpsSingapore,
  privateProxyIpsTokyo,
  privateProxyIpsTokyoSmallPP,
  privateProxyIpsVultrTokyo,
  tokyoK8sNatGatewayIp,
  wsProxyIps,
} from './index';

// Regex for valid IPv4 address (no CIDR notation expected for this package)
const ipv4Regex = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/;

function isValidIPv4(ip: string): boolean {
  const match = ip.match(ipv4Regex);
  if (!match) return false;

  // Check each octet is between 0-255
  for (let i = 1; i <= 4; i++) {
    const octet = parseInt(match[i], 10);
    if (octet < 0 || octet > 255) return false;
  }
  return true;
}

describe('privateProxyIpsVultrTokyo', () => {
  it('holds the nine fleet IPs, valid and unique', () => {
    expect(privateProxyIpsVultrTokyo).toHaveLength(9);
    privateProxyIpsVultrTokyo.forEach((ip) => expect(isValidIPv4(ip)).toBe(true));
    expect(new Set(privateProxyIpsVultrTokyo).size).toBe(privateProxyIpsVultrTokyo.length);
  });

  it('shares no address with the AWS pools it replaces', () => {
    const aws = [...privateProxyIpsSingapore, ...privateProxyIpsTokyo, tokyoK8sNatGatewayIp, ...wsProxyIps];
    expect(privateProxyIpsVultrTokyo.filter((ip) => aws.includes(ip))).toEqual([]);
  });
});

describe('privateProxyIpsSingapore', () => {
  it('should export a non-empty array', () => {
    expect(Array.isArray(privateProxyIpsSingapore)).toBe(true);
    expect(privateProxyIpsSingapore.length).toBeGreaterThan(0);
  });

  it('should contain valid IPv4 addresses', () => {
    privateProxyIpsSingapore.forEach((ip) => {
      expect(isValidIPv4(ip)).toBe(true);
    });
  });

  it('should contain unique IP addresses', () => {
    const uniqueIps = new Set(privateProxyIpsSingapore);
    expect(uniqueIps.size).toBe(privateProxyIpsSingapore.length);
  });
});

describe('privateProxyIpsTokyo', () => {
  it('should export a non-empty array', () => {
    expect(Array.isArray(privateProxyIpsTokyo)).toBe(true);
    expect(privateProxyIpsTokyo.length).toBeGreaterThan(0);
  });

  it('should contain valid IPv4 addresses', () => {
    privateProxyIpsTokyo.forEach((ip) => {
      expect(isValidIPv4(ip)).toBe(true);
    });
  });

  it('should contain unique IP addresses', () => {
    const uniqueIps = new Set(privateProxyIpsTokyo);
    expect(uniqueIps.size).toBe(privateProxyIpsTokyo.length);
  });
});

describe('privateProxyIpsTokyoSmallPP', () => {
  it('should export a non-empty array', () => {
    expect(Array.isArray(privateProxyIpsTokyoSmallPP)).toBe(true);
    expect(privateProxyIpsTokyoSmallPP.length).toBeGreaterThan(0);
  });

  it('should contain valid IPv4 addresses', () => {
    privateProxyIpsTokyoSmallPP.forEach((ip) => {
      expect(isValidIPv4(ip)).toBe(true);
    });
  });

  it('should be a subset of privateProxyIpsTokyo', () => {
    privateProxyIpsTokyoSmallPP.forEach((ip) => {
      expect(privateProxyIpsTokyo).toContain(ip);
    });
  });
});

describe('tokyoK8sNatGatewayIp', () => {
  it('should be a valid IPv4 address', () => {
    expect(isValidIPv4(tokyoK8sNatGatewayIp)).toBe(true);
  });

  it('should be the Tokyo K8s NAT gateway IP', () => {
    expect(tokyoK8sNatGatewayIp).toBe('18.176.98.246');
  });
});

describe('wsProxyIps', () => {
  it('should export a non-empty array', () => {
    expect(Array.isArray(wsProxyIps)).toBe(true);
    expect(wsProxyIps.length).toBeGreaterThan(0);
  });

  it('should contain valid IPv4 addresses', () => {
    wsProxyIps.forEach((ip) => {
      expect(isValidIPv4(ip)).toBe(true);
    });
  });
});

describe('exchangesRequireWsIp', () => {
  // Was 'should export a non-empty array': the last entry, coinbase, moved to the
  // tunnel relay (AWS_EXIT.md 5a), so no browser socket needs the ws-proxy IP.
  it('is empty now that every private socket uses the relay', () => {
    expect(exchangesRequireWsIp).toEqual([]);
  });

  it('should contain only strings', () => {
    exchangesRequireWsIp.forEach((exchange) => {
      expect(typeof exchange).toBe('string');
      expect(exchange.length).toBeGreaterThan(0);
    });
  });

  it('should contain unique exchange names', () => {
    const uniqueExchanges = new Set(exchangesRequireWsIp);
    expect(uniqueExchanges.size).toBe(exchangesRequireWsIp.length);
  });
});
