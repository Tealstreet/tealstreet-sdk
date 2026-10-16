//! Opaque SDK records and borrowed JWT cache handles preserve host identity.

// ts: ParadexSDKAccount
pub struct ParadexSdkAccount<A, C, P> {
    pub account: A,
    pub config: C,
    pub provider: P,
}

// ts: ParadexRequestSigner.getAccountAddress
pub fn get_account_address<A, T, E>(
    read_account: impl FnOnce() -> Result<A, E>,
    mut read_address: impl FnMut(A) -> Result<T, E>,
) -> Result<T, E> {
    read_address(read_account()?)
}

// ts: ParadexJWTManager.clearToken
pub struct JwtCache<'a, T: ?Sized, F: ?Sized> {
    pub token: Option<&'a T>,
    pub token_expiry: f64,
    pub refresh_promise: Option<&'a F>,
}

// ts: ParadexJWTManager.clearToken
pub fn clear_token<T: ?Sized, F: ?Sized>(cache: &mut JwtCache<'_, T, F>) {
    cache.token = None;
    cache.token_expiry = 0.0;
    cache.refresh_promise = None;
}
