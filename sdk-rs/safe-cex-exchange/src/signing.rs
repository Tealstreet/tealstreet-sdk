use crate::runtime::ExchangeFuture;
use safe_cex_transport::signing::SigningConfig;
/// Per-request capture runs the venue interceptors and terminates before network I/O.
pub trait SigningHttpClient {
    fn capture(&self, config: SigningConfig) -> ExchangeFuture<'_, SigningConfig>;
}
