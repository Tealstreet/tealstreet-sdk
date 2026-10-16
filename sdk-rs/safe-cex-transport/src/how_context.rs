use crate::{RestFailure, RestRequest, RestResponse};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct TradeTunnelTimeoutError {
    pub name: &'static str,
    pub message: String,
    pub config: RestRequest,
    pub method: String,
    pub trade_tunnel_route: Value,
    pub url: String,
}
impl TradeTunnelTimeoutError {
    pub fn new(
        message: String,
        config: RestRequest,
        method: String,
        trade_tunnel_route: Value,
        url: String,
    ) -> Self {
        Self {
            name: "TradeTunnelTimeoutError",
            message,
            config,
            method,
            trade_tunnel_route,
            url,
        }
    }
}
impl std::fmt::Display for TradeTunnelTimeoutError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for TradeTunnelTimeoutError {}

#[derive(Debug, Clone, PartialEq)]
pub struct HOWResponse {
    pub response: RestResponse,
    pub config: RestRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HOWRequestError {
    Timeout(Box<TradeTunnelTimeoutError>),
    Transport {
        error: Box<RestFailure>,
        config: Box<RestRequest>,
        route: Value,
        method: String,
        url: String,
    },
}

impl std::fmt::Display for HOWRequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout(error) => error.fmt(formatter),
            Self::Transport { error, .. } => error.fmt(formatter),
        }
    }
}
impl std::error::Error for HOWRequestError {}
