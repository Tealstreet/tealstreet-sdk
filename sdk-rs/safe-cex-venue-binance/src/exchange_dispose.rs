// ts: BinanceExchange.dispose
/// Ordered, synchronous disposal body. The operations invoke configured base,
/// public-socket and private-socket disposers; concrete class wiring is separate.
/// Callback failure escapes unchanged and prevents subsequent disposal calls.
pub fn dispose_binance_exchange<E>(
    dispose_base: impl FnOnce() -> Result<(), E>,
    dispose_public: impl FnOnce() -> Result<(), E>,
    dispose_private: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    dispose_base()?;
    dispose_public()?;
    dispose_private()
}

// ts: BinancePMExchange.dispose
/// Configured disposers run in source order; no catch, retry or finally step.
pub fn dispose_binance_pm_exchange<E>(
    dispose_base: impl FnOnce() -> Result<(), E>,
    dispose_public: impl FnOnce() -> Result<(), E>,
    dispose_private: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    dispose_base()?;
    dispose_public()?;
    dispose_private()
}

// ts: BinanceSpotExchange.dispose
/// Configured disposers run in source order; no catch, retry or finally step.
pub fn dispose_binance_spot_exchange<E>(
    dispose_base: impl FnOnce() -> Result<(), E>,
    dispose_public: impl FnOnce() -> Result<(), E>,
    dispose_private: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    dispose_base()?;
    dispose_public()?;
    dispose_private()
}
