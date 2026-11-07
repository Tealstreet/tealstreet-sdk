//! Opaque dispatch recipe and request continuation data.
pub mod dispatch;
use safe_cex_engine::{Method, json::ArrayAt};
#[derive(Clone)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub body: Option<String>,
    pub at: Option<ArrayAt>,
    pub floor: Option<safe_cex_engine::StampFloor>,
}
