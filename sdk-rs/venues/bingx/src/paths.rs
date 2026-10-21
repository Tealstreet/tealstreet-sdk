//! BingX perpetual swap endpoints.
pub const REST: &str = "https://open-api.bingx.com";
pub const PUBLIC_WS: &str = "wss://ws-market-swap.qq-os.com/ws";
pub const PRIVATE_WS: &str = "wss://open-api-swap.bingx.com/swap-market";
pub const MARKETS: &str = "/openApi/swap/v2/quote/contracts";
pub const DAILY: &str = "/openApi/swap/v2/quote/ticker";
pub const PREMIUM: &str = "/openApi/swap/v2/quote/premiumIndex";
pub const KLINES: &str = "/openApi/swap/v3/quote/klines";
pub const LEVERAGE: &str = "/openApi/swap/v2/trade/leverage";
