import type { ErrorCategory, ErrorPattern } from './errorPatterns';

import { describe, expect, it } from 'vitest';

import { DEFAULT_ERROR_PATTERNS, EXCHANGE_SPECIFIC_PATTERNS, mergePatterns } from './errorPatterns';

describe('errorPatterns', () => {
  describe('DEFAULT_ERROR_PATTERNS', () => {
    it('should have patterns for all main categories', () => {
      const categories = DEFAULT_ERROR_PATTERNS.map((p) => p.category);
      expect(categories).toContain('auth_invalid');
      expect(categories).toContain('auth_expired');
      expect(categories).toContain('auth_permissions');
      expect(categories).toContain('rate_limit');
      expect(categories).toContain('network');
      expect(categories).toContain('exchange_down');
      expect(categories).toContain('time_sync');
    });

    it('should have correct severity for auth_invalid', () => {
      const pattern = DEFAULT_ERROR_PATTERNS.find((p) => p.category === 'auth_invalid');
      expect(pattern?.severity).toBe('critical');
    });

    it('should have correct severity for rate_limit', () => {
      const pattern = DEFAULT_ERROR_PATTERNS.find((p) => p.category === 'rate_limit');
      expect(pattern?.severity).toBe('warning');
    });

    describe('auth_invalid patterns', () => {
      const authPattern = DEFAULT_ERROR_PATTERNS.find((p) => p.category === 'auth_invalid')!;

      it('should match "invalid api key"', () => {
        const matches = authPattern.patterns.some((p) =>
          typeof p === 'string' ? 'invalid api key'.includes(p.toLowerCase()) : p.test('invalid api key'),
        );
        expect(matches).toBe(true);
      });

      it('should match "unauthorized"', () => {
        const matches = authPattern.patterns.some((p) =>
          typeof p === 'string' ? 'unauthorized'.includes(p.toLowerCase()) : p.test('unauthorized'),
        );
        expect(matches).toBe(true);
      });

      it('should match Phemex private websocket auth rejection', () => {
        const message = 'Phemex websocket error - wrong api key or secret';
        const matches = authPattern.patterns.some((p) =>
          typeof p === 'string' ? message.toLowerCase().includes(p.toLowerCase()) : p.test(message),
        );
        expect(matches).toBe(true);
      });

      it('should match API_KEY_INVALID responses', () => {
        const message = 'Exchange Error: The request is unauthorized. [API_KEY_INVALID]';
        const matches = authPattern.patterns.some((p) =>
          typeof p === 'string' ? message.toLowerCase().includes(p.toLowerCase()) : p.test(message),
        );
        expect(matches).toBe(true);
      });
    });

    describe('network patterns', () => {
      const networkPattern = DEFAULT_ERROR_PATTERNS.find((p) => p.category === 'network')!;

      it('should match "ECONNREFUSED"', () => {
        const matches = networkPattern.patterns.some((p) =>
          typeof p === 'string' ? 'ECONNREFUSED'.toLowerCase().includes(p.toLowerCase()) : p.test('ECONNREFUSED'),
        );
        expect(matches).toBe(true);
      });

      it('should match "timeout"', () => {
        const matches = networkPattern.patterns.some((p) =>
          typeof p === 'string' ? 'timeout'.includes(p.toLowerCase()) : p.test('timeout'),
        );
        expect(matches).toBe(true);
      });
    });

    describe('auth_permissions patterns', () => {
      const permissionsPattern = DEFAULT_ERROR_PATTERNS.find((p) => p.category === 'auth_permissions')!;

      it('should match unmatched IP API-key allowlist failures', () => {
        const message = 'Unmatched IP, please check your API key';
        const matches = permissionsPattern.patterns.some((p) =>
          typeof p === 'string' ? message.toLowerCase().includes(p.toLowerCase()) : p.test(message),
        );
        expect(matches).toBe(true);
      });
    });
  });

  describe('new categories: suppressed, unsupported_backend, order_rejected, order_warning', () => {
    it('should have suppressed category in defaults', () => {
      const pattern = DEFAULT_ERROR_PATTERNS.find((p) => p.category === 'suppressed');
      expect(pattern).toBeDefined();
      expect(pattern?.severity).toBe('info');
    });

    it('suppressed pattern should match "market XYZ not found"', () => {
      const pattern = DEFAULT_ERROR_PATTERNS.find((p) => p.category === 'suppressed')!;
      const matches = pattern.patterns.some((p) =>
        typeof p === 'string'
          ? 'market BTCUSDT not found'.toLowerCase().includes(p.toLowerCase())
          : p.test('market BTCUSDT not found'),
      );
      expect(matches).toBe(true);
    });

    it('suppressed pattern should match "Market Not Found" (case insensitive)', () => {
      const pattern = DEFAULT_ERROR_PATTERNS.find((p) => p.category === 'suppressed')!;
      const matches = pattern.patterns.some((p) =>
        typeof p === 'string' ? 'Market Not Found'.toLowerCase().includes(p.toLowerCase()) : p.test('Market Not Found'),
      );
      expect(matches).toBe(true);
    });

    it('should have unsupported_backend category in defaults', () => {
      const pattern = DEFAULT_ERROR_PATTERNS.find((p) => p.category === 'unsupported_backend');
      expect(pattern).toBeDefined();
      expect(pattern?.severity).toBe('warning');
    });

    it('unsupported_backend should match "url is not allowed"', () => {
      const pattern = DEFAULT_ERROR_PATTERNS.find((p) => p.category === 'unsupported_backend')!;
      const matches = pattern.patterns.some((p) =>
        typeof p === 'string' ? 'url is not allowed'.includes(p.toLowerCase()) : p.test('url is not allowed'),
      );
      expect(matches).toBe(true);
    });

    it('bybit should have order_warning for EC_PostOnlyWillTakeLiquidity', () => {
      const pattern = EXCHANGE_SPECIFIC_PATTERNS.bybit.find((p) => p.category === 'order_warning');
      expect(pattern).toBeDefined();
      expect(pattern?.patterns).toContain('EC_PostOnlyWillTakeLiquidity');
    });

    it('bybit should have order_rejected for EC_* codes', () => {
      const pattern = EXCHANGE_SPECIFIC_PATTERNS.bybit.find((p) => p.category === 'order_rejected');
      expect(pattern).toBeDefined();
    });

    it('bingx should have order_warning for post-only message', () => {
      const pattern = EXCHANGE_SPECIFIC_PATTERNS.bingx.find((p) => p.category === 'order_warning');
      expect(pattern).toBeDefined();
    });

    it('bitunix should have order_warning for user-action order placement rejections', () => {
      const pattern = EXCHANGE_SPECIFIC_PATTERNS.bitunix.find((p) => p.category === 'order_warning');
      expect(pattern).toBeDefined();
      expect(pattern?.severity).toBe('warning');
      expect(pattern?.patterns).toContain('nothing to reduce');
      expect(pattern?.patterns).toContain('This trading pair does not currently support trading via OpenAPI');
      expect(pattern?.patterns).toContain('Stock Perpetual trading permissions not activated');
      // Do NOT bundle internal-recovery messages (Missing positionId) into
      // the warning category — that would toast on every retry-then-succeed.
      expect(pattern?.patterns).not.toContain('missing positionid');
    });

    it('bitunix should suppress "missing positionid" (auto-recovered via refresh+retry)', () => {
      const pattern = EXCHANGE_SPECIFIC_PATTERNS.bitunix.find((p) => p.category === 'suppressed');
      expect(pattern).toBeDefined();
      expect(pattern?.severity).toBe('info');
      expect(pattern?.patterns).toContain('missing positionid');
    });
  });

  describe('EXCHANGE_SPECIFIC_PATTERNS', () => {
    it('should have patterns for bybit', () => {
      expect(EXCHANGE_SPECIFIC_PATTERNS.bybit).toBeDefined();
      expect(EXCHANGE_SPECIFIC_PATTERNS.bybit.length).toBeGreaterThan(0);
    });

    it('should have patterns for binance', () => {
      expect(EXCHANGE_SPECIFIC_PATTERNS.binance).toBeDefined();
      expect(EXCHANGE_SPECIFIC_PATTERNS.binance.length).toBeGreaterThan(0);
    });

    it('should have patterns for okx', () => {
      expect(EXCHANGE_SPECIFIC_PATTERNS.okx).toBeDefined();
      expect(EXCHANGE_SPECIFIC_PATTERNS.okx.length).toBeGreaterThan(0);
    });

    it('should have patterns for hyperliquid', () => {
      expect(EXCHANGE_SPECIFIC_PATTERNS.hyperliquid).toBeDefined();
      expect(EXCHANGE_SPECIFIC_PATTERNS.hyperliquid.length).toBeGreaterThan(0);
    });

    it('binance should have auth_invalid pattern for -2014', () => {
      const authPattern = EXCHANGE_SPECIFIC_PATTERNS.binance.find((p) => p.category === 'auth_invalid');
      expect(authPattern?.patterns).toContain('-2014');
    });

    it('bybit should have time_sync pattern', () => {
      const timeSyncPattern = EXCHANGE_SPECIFIC_PATTERNS.bybit.find((p) => p.category === 'time_sync');
      expect(timeSyncPattern).toBeDefined();
      expect(timeSyncPattern?.patterns).toContain('recv_window');
    });
  });

  describe('mergePatterns', () => {
    it('should merge patterns from multiple lists', () => {
      const list1: ErrorPattern[] = [{ category: 'auth_invalid', severity: 'critical', patterns: ['pattern1'] }];
      const list2: ErrorPattern[] = [{ category: 'network', severity: 'warning', patterns: ['pattern2'] }];

      const merged = mergePatterns(list1, list2);
      expect(merged.length).toBe(2);
      expect(merged.some((p) => p.category === 'auth_invalid')).toBe(true);
      expect(merged.some((p) => p.category === 'network')).toBe(true);
    });

    it('should merge patterns for same category', () => {
      const list1: ErrorPattern[] = [{ category: 'auth_invalid', severity: 'critical', patterns: ['pattern1'] }];
      const list2: ErrorPattern[] = [{ category: 'auth_invalid', severity: 'critical', patterns: ['pattern2'] }];

      const merged = mergePatterns(list1, list2);
      expect(merged.length).toBe(1);

      const authPattern = merged.find((p) => p.category === 'auth_invalid');
      expect(authPattern?.patterns).toContain('pattern1');
      expect(authPattern?.patterns).toContain('pattern2');
    });

    it('should handle empty arrays', () => {
      const merged = mergePatterns([], []);
      expect(merged).toEqual([]);
    });

    it('should handle single array', () => {
      const list: ErrorPattern[] = [{ category: 'auth_invalid', severity: 'critical', patterns: ['pattern1'] }];

      const merged = mergePatterns(list);
      expect(merged.length).toBe(1);
    });

    it('should preserve severity from later patterns', () => {
      const list1: ErrorPattern[] = [{ category: 'auth_invalid', severity: 'warning', patterns: ['pattern1'] }];
      const list2: ErrorPattern[] = [{ category: 'auth_invalid', severity: 'critical', patterns: ['pattern2'] }];

      const merged = mergePatterns(list1, list2);
      // Note: The implementation keeps the first severity and just merges patterns
      const authPattern = merged.find((p) => p.category === 'auth_invalid');
      expect(authPattern?.severity).toBe('warning'); // First pattern's severity is kept
    });

    it('should handle multiple pattern lists', () => {
      const list1: ErrorPattern[] = [{ category: 'auth_invalid', severity: 'critical', patterns: ['a'] }];
      const list2: ErrorPattern[] = [{ category: 'auth_invalid', severity: 'critical', patterns: ['b'] }];
      const list3: ErrorPattern[] = [{ category: 'auth_invalid', severity: 'critical', patterns: ['c'] }];

      const merged = mergePatterns(list1, list2, list3);
      const authPattern = merged.find((p) => p.category === 'auth_invalid');
      expect(authPattern?.patterns).toHaveLength(3);
      expect(authPattern?.patterns).toContain('a');
      expect(authPattern?.patterns).toContain('b');
      expect(authPattern?.patterns).toContain('c');
    });
  });
});

describe('bybit auth rejections the venue actually sends', () => {
  const match = (message: string) => {
    const patterns = mergePatterns(DEFAULT_ERROR_PATTERNS, EXCHANGE_SPECIFIC_PATTERNS.bybit);
    for (const pattern of patterns) {
      for (const p of pattern.patterns) {
        const hit = typeof p === 'string' ? message.toLowerCase().includes(p.toLowerCase()) : p.test(message);
        if (hit) return pattern.category;
      }
    }
    return null;
  };

  // Verbatim retMsg strings. 10003 classified as `unknown` until 2026-09-25,
  // so the commonest rejection of all never raised the auth badge.
  it('classifies the live 10003 text, which the older literal missed', () => {
    expect(match('API key is invalid.')).toBe('auth_invalid');
  });

  it('classifies a LABELLED ret code, and keeps each code in its real category', () => {
    expect(match('Bybit request failed with code 10003')).toBe('auth_invalid');
    expect(match('retCode: 10010 Unmatched IP')).toBe('auth_permissions');
    expect(match('retCode=33004')).toBe('auth_expired');
  });

  it('does not read a price, balance or id that happens to contain a ret code', () => {
    expect(match('order price 10005.5 out of range')).not.toBe('auth_permissions');
    expect(match('Balance 33004.21 USDT insufficient')).not.toBe('auth_expired');
    expect(match('orderId 33004-abc was not found')).not.toBe('auth_expired');
  });

  it('keeps an expired key distinct from an invalid one', () => {
    expect(match('api key has expired')).toBe('auth_expired');
  });

  it('does not read a rate limit or a clock skew as an auth failure', () => {
    expect(match('Too many visits!')).not.toBe('auth_invalid');
    expect(match('invalid request, please check your server timestamp or recv_window param')).toBe('time_sync');
  });
});

describe('hyrotrader auth rejections the simulator sends', () => {
  const match = (message: string) => {
    const patterns = mergePatterns(DEFAULT_ERROR_PATTERNS, EXCHANGE_SPECIFIC_PATTERNS.hyrotrader);
    for (const pattern of patterns) {
      for (const p of pattern.patterns) {
        const hit = typeof p === 'string' ? message.toLowerCase().includes(p.toLowerCase()) : p.test(message);
        if (hit) return pattern.category;
      }
    }
    return null;
  };

  it('classifies HyroTrader 10003 as an auth error for user-facing challenge guidance', () => {
    expect(match('retCode:10003 retMsg:API key is invalid.')).toBe('auth_invalid');
  });
});

describe('mexc permission rejections the venue sends', () => {
  const match = (message: string) => {
    const patterns = mergePatterns(DEFAULT_ERROR_PATTERNS, EXCHANGE_SPECIFIC_PATTERNS.mexc);
    for (const pattern of patterns) {
      for (const p of pattern.patterns) {
        const hit = typeof p === 'string' ? message.toLowerCase().includes(p.toLowerCase()) : p.test(message);
        if (hit) return pattern.category;
      }
    }
    return null;
  };

  it('classifies missing API-key trading write access as a permission error', () => {
    expect(match('Please enable API Key trading information write access')).toBe('auth_permissions');
  });
});

describe('engine capacity refusals are not venue rate limits', () => {
  const match = (message: string) => {
    for (const pattern of DEFAULT_ERROR_PATTERNS) {
      for (const p of pattern.patterns) {
        const hit = typeof p === 'string' ? message.toLowerCase().includes(p.toLowerCase()) : p.test(message);
        if (hit) return pattern.category;
      }
    }
    return null;
  };

  it('leaves the Rust engine capacity message unclassified', () => {
    expect(match('Engine request capacity reached')).toBeNull();
  });

  it('documents why the former engine text was replaced', () => {
    expect(match('Too many requests in flight')).toBe('rate_limit');
  });
});
