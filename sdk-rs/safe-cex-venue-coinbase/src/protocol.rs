//! Inert Starbase protocol constants; TS remains the behavioral reference.

// ts: coinbase.starbase-cutover.ts::COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE

pub const COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE: &[(&str, &str)] = &[
    ("credentialedHttpAuthentication", "unverified"),
    ("credentialedHttpReadParity", "unverified"),
    ("credentialedWebsocketSessionLifecycle", "unverified"),
    ("credentialedWriteCanary", "unverified"),
    ("cutoverRunbook", "unverified"),
    ("httpAuthenticationContract", "implemented"),
    ("privateHttpResponseContracts", "implemented"),
    ("privateNotificationEnvelope", "unverified"),
    ("publicHttpResponseContracts", "implemented"),
    ("unitParity", "unverified"),
    ("websocketSessionContract", "implemented"),
];

// ts: coinbase.starbase-cutover.ts::COINBASE_STARBASE_HTTP_SHADOW_ACTIVATION_ENABLED

pub const COINBASE_STARBASE_HTTP_SHADOW_ACTIVATION_ENABLED: bool = false;

// ts: coinbase.starbase-cutover.ts::COINBASE_STARBASE_WEBSOCKET_ACTIVATION_ENABLED

pub const COINBASE_STARBASE_WEBSOCKET_ACTIVATION_ENABLED: bool = false;

// ts: coinbase.starbase-cutover.ts::COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED

pub const COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED: bool = false;

// ts: coinbase.starbase-orders.ts::COINBASE_STARBASE_BASE_AMOUNT_SOURCE

pub const COINBASE_STARBASE_BASE_AMOUNT_SOURCE: &str = "coinbase-technical-migration-guide";

// ts: coinbase.starbase-public-frames.ts::COINBASE_STARBASE_PUBLIC_FRAME_CLASSIFIER_ACTIVATION_ENABLED

pub const COINBASE_STARBASE_PUBLIC_FRAME_CLASSIFIER_ACTIVATION_ENABLED: bool = false;

// ts: coinbase.starbase-public-frames.ts::COINBASE_STARBASE_PUBLIC_FRAME_CAPTURE_SHA256

pub const COINBASE_STARBASE_PUBLIC_FRAME_CAPTURE_SHA256: &[(&str, &str)] = &[
    (
        "heartbeat",
        "7a63af8955274769a006d4dd7ab448ff7f2c3a255a11871b2a95e9bb9e00e503",
    ),
    (
        "notification",
        "a18e355e8c29e6a5699166e5dd5e67cc797345d6fda6f6e30676a06d91dba692",
    ),
    (
        "testRequest",
        "01bee32b135c20dea365212880829f7c34ad4eabe00bd1f13a8d0dcec544280a",
    ),
];

// ts: coinbase.starbase-public.ts::COINBASE_STARBASE_CHART_RESOLUTIONS

pub const COINBASE_STARBASE_CHART_RESOLUTIONS: &[&str] =
    &["1", "3", "5", "15", "30", "60", "120", "360", "720", "1D"];

// ts: coinbase.starbase-public.ts::COINBASE_STARBASE_TIMEFRAME_RESOLUTION

pub const COINBASE_STARBASE_TIMEFRAME_RESOLUTION: &[(&str, &str)] = &[
    ("1m", "1"),
    ("3m", "3"),
    ("5m", "5"),
    ("15m", "15"),
    ("30m", "30"),
    ("1h", "60"),
    ("2h", "120"),
    ("6h", "360"),
    ("12h", "720"),
    ("1d", "1D"),
];

// ts: coinbase.starbase-session.ts::COINBASE_STARBASE_WEBSOCKET_ONLY_SESSION_METHODS

pub const COINBASE_STARBASE_WEBSOCKET_ONLY_SESSION_METHODS: &[&str] = &[
    "private/enable_cancel_on_disconnect",
    "private/disable_cancel_on_disconnect",
];

// ts: coinbase.starbase.ts::COINBASE_STARBASE_ENDPOINTS

pub const COINBASE_STARBASE_ENDPOINTS: &[(&str, &str)] = &[
    ("http", "https://drb.coinbase.com/api/v2"),
    ("websocket", "wss://drb.coinbase.com/ws/api/v2"),
];

// ts: coinbase.starbase.ts::COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS

pub const COINBASE_STARBASE_AUTH_REFRESH_AFTER_MS: &[(&str, u64)] =
    &[("http", 900000), ("websocket", 2940000)];
