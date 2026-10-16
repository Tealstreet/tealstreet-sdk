//! Runtime-scoped accessToken wrapper. The contextual provider owns its credential.
use crate::proxy_token::{ProxyTokenClient, is_token_expired};
use safe_cex_store::virtual_clock::VirtualClock;
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};

#[derive(Debug, Clone)]
pub struct ProxyTokenUnavailableError {
    pub name: String,
    pub message: String,
}
impl ProxyTokenUnavailableError {
    pub fn new(message: Option<&str>) -> Self {
        Self {
            name: "ProxyTokenUnavailableError".into(),
            message: message
                .unwrap_or("Tealstreet proxy token unavailable")
                .into(),
        }
    }
}
type TokenFuture = Pin<Box<dyn Future<Output = Result<String, Rc<ProxyTokenUnavailableError>>>>>;
pub type TokenRefreshFunction = Rc<dyn Fn() -> TokenFuture>;
pub type ContextualTokenProvider =
    Rc<dyn Fn() -> Result<Option<TokenRefreshFunction>, Rc<ProxyTokenUnavailableError>>>;
#[derive(Default)]
struct State {
    current_proxy_token: String,
    custom_refresh_fn: Option<TokenRefreshFunction>,
    contextual_token_provider_fn: Option<ContextualTokenProvider>,
}
#[derive(Clone)]
pub struct AccessToken {
    state: Rc<RefCell<State>>,
    client: Option<ProxyTokenClient>,
    clock: VirtualClock,
    browser_context: bool,
}
impl AccessToken {
    pub fn new(
        client: Option<ProxyTokenClient>,
        clock: VirtualClock,
        browser_context: bool,
    ) -> Self {
        Self {
            state: Rc::new(RefCell::new(State::default())),
            client,
            clock,
            browser_context,
        }
    }
    pub fn current_proxy_token(&self) -> String {
        self.state.borrow().current_proxy_token.clone()
    }
    pub fn sync_token(&self) {
        if let Some(client) = &self.client {
            self.state.borrow_mut().current_proxy_token = client.get_current_token();
        }
    }
    pub async fn get_contextual_proxy_token(
        &self,
    ) -> Result<Option<String>, Rc<ProxyTokenUnavailableError>> {
        let lookup = self.state.borrow().contextual_token_provider_fn.clone();
        let Some(provider) = (match lookup {
            Some(lookup) => lookup()?,
            None => None,
        }) else {
            return Ok(None);
        };
        match provider().await {
            Ok(token) if !token.is_empty() && !is_token_expired(&token, &self.clock) => {
                Ok(Some(token))
            }
            Ok(_) => Err(Rc::new(ProxyTokenUnavailableError::new(Some(
                "proxy token missing or expired for this request context",
            )))),
            Err(error) if error.name == "ProxyTokenUnavailableError" => Err(error),
            Err(_) => Err(Rc::new(ProxyTokenUnavailableError::new(Some(
                "proxy token provider failed",
            )))),
        }
    }
    pub async fn refresh_proxy_token(&self) -> Result<String, Rc<ProxyTokenUnavailableError>> {
        if let Some(token) = self.get_contextual_proxy_token().await? {
            return Ok(token);
        }
        Ok(self.refresh_module_proxy_token().await)
    }
    pub async fn refresh_module_proxy_token(&self) -> String {
        let custom = self.state.borrow().custom_refresh_fn.clone();
        if let Some(custom) = custom {
            if let Ok(token) = custom().await
                && !token.is_empty()
            {
                self.set_proxy_token(token);
            }
            return self.current_proxy_token();
        }
        if self.browser_context
            && let Some(client) = &self.client
        {
            let token = client.refresh_proxy_token().await;
            self.state.borrow_mut().current_proxy_token = token.clone();
            return token;
        }
        self.current_proxy_token()
    }
    pub async fn get_proxy_token(&self) -> Result<String, Rc<ProxyTokenUnavailableError>> {
        if let Some(token) = self.get_contextual_proxy_token().await? {
            return Ok(token);
        }
        if self.state.borrow().custom_refresh_fn.is_some() {
            if is_token_expired(&self.current_proxy_token(), &self.clock) {
                return Ok(self.refresh_module_proxy_token().await);
            }
            return Ok(self.current_proxy_token());
        }
        if self.browser_context
            && let Some(client) = &self.client
        {
            let token = client.get_proxy_token().await;
            self.state.borrow_mut().current_proxy_token = token.clone();
            return Ok(token);
        }
        Ok(self.current_proxy_token())
    }
    pub fn set_proxy_token(&self, token: String) {
        self.state.borrow_mut().current_proxy_token = token.clone();
        if let Some(client) = &self.client {
            client.set_token(token);
        }
    }
    pub fn set_token_refresh_function(&self, refresh: Option<TokenRefreshFunction>) {
        self.state.borrow_mut().custom_refresh_fn = refresh;
    }
    pub fn set_contextual_proxy_token_provider(&self, lookup: Option<ContextualTokenProvider>) {
        self.state.borrow_mut().contextual_token_provider_fn = lookup;
    }
    pub async fn initialize_proxy_token(&self) {
        if self.browser_context
            && let Some(client) = &self.client
        {
            client.initialize_proxy_token_client().await;
            self.sync_token();
        }
    }
}
