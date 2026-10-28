/**
 * Error pattern definitions for fuzzy exchange health detection
 *
 * Categories detect common exchange issues from error/log messages.
 * Exchanges can override defaults by providing exchange-specific patterns.
 *
 * `auth_invalid` is deliberately NARROW on the signer-failure dimension: it
 * matches "auth failed" and friends because a venue-sent credential rejection
 * reads that way, but transient browser-signer failures must NOT match it —
 * the web controller treats `auth_invalid` as terminal and suppresses
 * automatic account reloads for five minutes (`noteTerminalExchangeFailure`),
 * which is the very reload a signer-less tab needs to re-pair. Every
 * `<venue>.ws-private.ts` therefore emits `Remote WS signing unavailable:` on
 * the `remoteSigner.sign` failure branch rather than `Remote WS auth failed:`
 * (the former string is explicitly covered by the `private-ws-auth-failure-
 * consequences.test.ts` guard). Do NOT widen `auth_invalid` here as a fix for
 * a misclassified signer failure — fix the emit site instead.
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

const BYBIT_TRADING_AGREEMENT_PATTERNS: (string | RegExp)[] = [
  'Bybit Trading Terms not accepted',
  'You must agree to the Trading Terms',
  'You must sign the required agreement before trading this contract',
  /\b(?:ret)?code[=:\s]+(?:110123|110125)\b/i,
];

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
      /api[-_ ]?key.*invalid/i,
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
      // A bare status number only, never digits inside an id, price or amount:
      // a substring '429' read client order 4294967295 as a rate limit.
      /(?<![\w.])429(?![\w.])/,
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
      /(?<![\w.])50[23](?![\w.])/,
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
      patterns: ['EC_PostOnlyWillTakeLiquidity', ...BYBIT_TRADING_AGREEMENT_PATTERNS],
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
      // The live retMsg for 10003 is "API key is invalid." — with an "is" the
      // older literal lacks, so the commonest bybit rejection classified as
      // `unknown` and never raised the auth badge.
      patterns: [
        'apiKey is missing',
        'error sign!',
        'invalid api_key',
        'Api key invalid',
        /api\s*key\s+is\s+invalid/i,
        // Codes must be anchored to a `code`/`retCode` label. A bare /\b10005\b/
        // also matches "order price 10005.5 out of range" and a balance of
        // 33004.21, and a false auth_invalid is latched critical: red badge plus
        // five minutes of suppressed reloads on a perfectly good key.
        /\b(?:ret)?code[=:\s]+(?:10003|10004)\b/i,
      ],
    },
    {
      // Split by what the code MEANS: 10005/10010 are permission and IP-allowlist
      // failures, not a bad key, and 33004 is expiry. Folding all five into
      // auth_invalid mislabels a working key whose IP moved.
      category: 'auth_permissions',
      severity: 'critical',
      patterns: [/\b(?:ret)?code[=:\s]+(?:10005|10010)\b/i],
    },
    {
      category: 'auth_expired',
      severity: 'critical',
      patterns: [/api\s*key\s+has\s+expired/i, /\b(?:ret)?code[=:\s]+33004\b/i],
    },
    {
      category: 'time_sync',
      severity: 'warning',
      patterns: ['recv_window', 'Timestamp for this request'],
    },
  ],
  bybitdemo: [
    {
      category: 'order_warning',
      severity: 'warning',
      patterns: BYBIT_TRADING_AGREEMENT_PATTERNS,
    },
  ],
  bybitinverse: [
    {
      category: 'order_warning',
      severity: 'warning',
      patterns: BYBIT_TRADING_AGREEMENT_PATTERNS,
    },
  ],
  bybitspot: [
    {
      category: 'order_warning',
      severity: 'warning',
      patterns: BYBIT_TRADING_AGREEMENT_PATTERNS,
    },
  ],
  hyrotrader: [
    {
      category: 'auth_invalid',
      severity: 'critical',
      patterns: [
        /api\s*key\s+is\s+invalid/i,
        /\b(?:ret)?code[=:\s]+10003\b/i,
      ],
    },
    {
      category: 'order_warning',
      severity: 'warning',
      patterns: BYBIT_TRADING_AGREEMENT_PATTERNS,
    },
  ],
  binance: [
    {
      category: 'auth_invalid',
      severity: 'critical',
      // -1022 is an invalid signature, not a clock error (-1021): the engine
      // classifies it authInvalid after its one re-signed resend.
      patterns: ['-2014', '-2015', '-1022', 'API-key format invalid'],
    },
    {
      category: 'time_sync',
      severity: 'warning',
      patterns: ['-1021', 'Timestamp for this request'],
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
      // Codes as the engine words them ("OKX 50111: ...") or labelled in a raw
      // body ("code":"50111"), never a bare number a BTC price such as 50111.5
      // contains. Each code sits in the category of the OKX CodeTable.
      category: 'auth_invalid',
      severity: 'critical',
      patterns: [
        /\b(?:OKX|code"?)[=:\s]+"?(?:50101|50103|50104|50105|50111|50113|50114|50119)\b/i,
        'Invalid sign',
        'Invalid OK-ACCESS-KEY',
        "API key doesn't exist",
      ],
    },
    {
      // 50030 (no permission for one API) gets no code row: like Bybit 10005 it
      // is one endpoint's scope, which Rust startup does not treat as a bad key.
      category: 'auth_permissions',
      severity: 'critical',
      patterns: [/\b(?:OKX|code"?)[=:\s]+"?(?:50100|50110|50120)\b/i],
    },
    {
      // 50102 (expired) and 50112 (invalid) OK-ACCESS-TIMESTAMP: a clock to
      // resync, not a bad key.
      category: 'time_sync',
      severity: 'warning',
      patterns: [/\b(?:OKX|code"?)[=:\s]+"?(?:50102|50112)\b/i],
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
  mexc: [
    {
      category: 'auth_permissions',
      severity: 'critical',
      patterns: ['Please enable API Key trading information write access'],
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
      patterns: [
        'nothing to reduce',
        'This trading pair does not currently support trading via OpenAPI',
        'Stock Perpetual trading permissions not activated',
      ],
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
