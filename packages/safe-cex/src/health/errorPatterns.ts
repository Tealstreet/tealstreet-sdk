/**
 * Error pattern definitions for fuzzy exchange health detection
 *
 * Categories detect common exchange issues from error/log messages.
 * Exchanges can override defaults by providing exchange-specific patterns.
 */

export type ErrorCategory =
  | 'auth_invalid'
  | 'auth_expired'
  | 'auth_permissions'
  | 'rate_limit'
  | 'network'
  | 'exchange_down'
  | 'time_sync'
  | 'order_rejected'
  | 'order_warning'
  | 'unsupported_backend'
  | 'suppressed'
  | 'unknown';

export type ErrorSeverity = 'critical' | 'warning' | 'info';

export interface ErrorPattern {
  category: ErrorCategory;
  patterns: (string | RegExp)[];
  severity: ErrorSeverity;
}

/**
 * Default error patterns that apply to all exchanges
 */
export const DEFAULT_ERROR_PATTERNS: ErrorPattern[] = [
  // Suppressed — errors that should be silently ignored (e.g., market not found during symbol switching)
  // Must come before other patterns to prevent false matches
  {
    category: 'suppressed',
    severity: 'info',
    patterns: [/market.*not found/i, /not found.*market/i],
  },
  // Unsupported backend — exchange not configured in tradetunnel
  // Must come before auth_permissions (which has 'not allowed') to avoid false matches
  {
    category: 'unsupported_backend',
    severity: 'warning',
    patterns: ['url is not allowed', 'url not allowed'],
  },
  // Authentication - Invalid credentials
  {
    category: 'auth_invalid',
    severity: 'critical',
    patterns: [
      'invalid api key',
      'apikey is missing',
      'api key is missing',
      'invalid signature',
      'error sign',
      'signature mismatch',
      'api-key format invalid',
      'api key rejected',
      'invalid key',
      'authentication failed',
      'auth failed',
      'unauthorized',
      'invalid credentials',
      'wrong api key or secret',
      'api_key_invalid',
      /api[-_]?key.*invalid/i,
      /invalid.*api[-_]?key/i,
      /(?:wrong|invalid).*(?:api[-_ ]?key|secret)/i,
      /secret.*invalid/i,
    ],
  },
  // Authentication - Expired
  {
    category: 'auth_expired',
    severity: 'critical',
    patterns: [
      'token expired',
      'session expired',
      'key expired',
      'signature expired',
      /expired.*key/i,
      /key.*expired/i,
      /token.*expired/i,
      /session.*expired/i,
      /signature.*expired/i,
    ],
  },
  // Authentication - Permission issues
  {
    category: 'auth_permissions',
    severity: 'critical',
    patterns: [
      'permission denied',
      'insufficient permissions',
      'not allowed',
      'forbidden',
      'no permission',
      'access denied',
      'ip whitelist',
      'ip not allowed',
      'ip restriction',
      'unmatched ip',
      /permission.*denied/i,
      /insufficient.*permission/i,
    ],
  },
  // Rate limiting
  {
    category: 'rate_limit',
    severity: 'warning',
    patterns: [
      'rate limit',
      'too many requests',
      '429',
      'request limit',
      'throttle',
      'exceed.*limit',
      /rate.*limit/i,
      /too.*many.*request/i,
    ],
  },
  // Network issues
  {
    category: 'network',
    severity: 'warning',
    patterns: [
      'timeout',
      'ECONNREFUSED',
      'ECONNRESET',
      'ETIMEDOUT',
      'ENOTFOUND',
      'connection reset',
      'network error',
      'socket hang up',
      'socket closed',
      /connect.*refused/i,
      /connection.*failed/i,
    ],
  },
  // Exchange down/maintenance
  {
    category: 'exchange_down',
    severity: 'warning',
    patterns: [
      'service unavailable',
      'maintenance',
      '503',
      '502',
      'temporarily unavailable',
      'server error',
      'internal error',
      'system busy',
      /service.*unavailable/i,
      /under.*maintenance/i,
    ],
  },
  // Time sync issues
  {
    category: 'time_sync',
    severity: 'warning',
    patterns: [
      'recv window',
      'recvwindow',
      'time sync',
      'out of sync',
      /timestamp.*(?:ahead|behind|recv[ _-]?window|server time|outside)/i,
      /(?:ahead|behind|recv[ _-]?window|server time|outside).*timestamp/i,
      /clock.*(?:sync|skew|drift|ahead|behind)/i,
    ],
  },
];

/**
 * Exchange-specific pattern overrides/additions
 * Key is the exchange name (lowercase)
 */
export const EXCHANGE_SPECIFIC_PATTERNS: Record<string, ErrorPattern[]> = {
  bybit: [
    {
      category: 'order_warning',
      severity: 'warning',
      patterns: ['EC_PostOnlyWillTakeLiquidity'],
    },
    {
      category: 'order_rejected',
      severity: 'warning',
      patterns: [
        /\bEC_\w+/i, // Bybit EC_* error codes (generic catch-all)
      ],
    },
    {
      category: 'auth_invalid',
      severity: 'critical',
      patterns: ['apiKey is missing', 'error sign!', 'invalid api_key', 'Api key invalid'],
    },
    {
      category: 'time_sync',
      severity: 'warning',
      patterns: ['recv_window', 'Timestamp for this request'],
    },
  ],
  binance: [
    {
      category: 'auth_invalid',
      severity: 'critical',
      patterns: ['-2014', '-2015', 'API-key format invalid'],
    },
    {
      category: 'time_sync',
      severity: 'warning',
      patterns: ['-1021', '-1022', 'Timestamp for this request'],
    },
    {
      category: 'rate_limit',
      severity: 'warning',
      patterns: ['-1015', 'Way too many requests', 'Request weight'],
    },
  ],
  bingx: [
    {
      category: 'auth_invalid',
      severity: 'critical',
      patterns: ['Incorrect apiKey'],
    },
    {
      category: 'order_warning',
      severity: 'warning',
      patterns: ['The Maker (Post Only) order ensures'],
    },
  ],
  okx: [
    {
      category: 'auth_invalid',
      severity: 'critical',
      patterns: ['50111', '50112', '50113', 'Invalid sign', 'Invalid OK-ACCESS-KEY', "API key doesn't exist"],
    },
  ],
  hyperliquid: [
    {
      category: 'auth_invalid',
      severity: 'critical',
      patterns: ['Invalid signature', 'wallet address'],
    },
  ],
  blofin: [
    {
      category: 'auth_invalid',
      severity: 'critical',
      patterns: ['Access key does not exist'],
    },
  ],
  coinbase: [
    {
      category: 'auth_permissions',
      severity: 'critical',
      patterns: ['No portfolio with permissions found'],
    },
  ],
  phemex: [
    {
      category: 'auth_invalid',
      severity: 'critical',
      patterns: ["Cannot read properties of undefined (reading 'account')"],
    },
  ],
  bitunix: [
    {
      category: 'auth_invalid',
      severity: 'critical',
      patterns: ['Token invalid', 'login failed, double check your key'],
    },
    {
      // `Missing positionId` is an INTERNAL recovery path — placeOrder's
      // catch refreshes positions and retries automatically. The user
      // almost never needs to see this; suppressing it keeps the happy
      // retry-then-succeed path silent. We still emit (so it lands in
      // console + Sentry + requestsLog for debugging if it actually
      // persists after refresh). Listed BEFORE order_warning so the
      // suppress match wins. Pattern lowercase-matched against
      // `formatCreateOrder`'s emit string.
      category: 'suppressed',
      severity: 'info',
      patterns: ['missing positionid'],
    },
    {
      // `nothing to reduce` is a real user-action error — they enabled
      // Reduce-Only and clicked Buy/Sell against a flat account. Render
      // as yellow `warningToast` (not red `errorToast`) so they get a
      // clear nudge without the alarm signal. The exact string is
      // emitted by Bitunix's `formatCreateOrder` via `emitAndLogError`
      // — keep in sync if you reword it.
      category: 'order_warning',
      severity: 'warning',
      patterns: ['nothing to reduce'],
    },
  ],
};

/**
 * Merge multiple pattern lists, with later lists taking priority
 * for the same category
 */
export function mergePatterns(...patternLists: ErrorPattern[][]): ErrorPattern[] {
  const categoryMap = new Map<ErrorCategory, ErrorPattern>();

  for (const patterns of patternLists) {
    for (const pattern of patterns) {
      const existing = categoryMap.get(pattern.category);
      if (existing) {
        // Merge patterns for same category
        categoryMap.set(pattern.category, {
          ...existing,
          patterns: [...existing.patterns, ...pattern.patterns],
        });
      } else {
        categoryMap.set(pattern.category, { ...pattern });
      }
    }
  }

  return Array.from(categoryMap.values());
}
