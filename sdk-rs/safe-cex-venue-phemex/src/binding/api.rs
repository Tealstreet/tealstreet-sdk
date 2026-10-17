use super::rt::{Host, text};
use crate::api_transport::{ApiHost, RequestHost};
use safe_cex_exchange::js_runtime::{
    client,
    value::{Js, Thrown, arg},
};

#[derive(Clone)]
struct Api {
    host: Host,
    options: Js,
}
pub(crate) fn create(host: &Host, options: Js) -> Result<Js, Thrown> {
    crate::api_transport::create_api(&mut Api {
        host: host.clone(),
        options,
    })
}
impl RequestHost for Api {
    type Config = Js;
    type Value = Js;
    type Hmac = Js;
    type Error = Thrown;
    fn copy(&mut self, c: &Js) -> Result<Js, Thrown> {
        c.spread_object()
    }
    fn remote(&mut self) -> Result<bool, Thrown> {
        Ok(self.options.get("remoteSigner")?.truthy())
    }
    fn key(&mut self) -> Result<Js, Thrown> {
        self.options.get("key")
    }
    fn secret(&mut self) -> Result<Js, Thrown> {
        self.options.get("secret")
    }
    fn truthy(&mut self, v: &Js) -> Result<bool, Thrown> {
        Ok(v.truthy())
    }
    fn params(&mut self, c: &Js) -> Result<Option<Vec<u16>>, Thrown> {
        let p = c.get("params")?;
        if !p.truthy() {
            return Ok(None);
        }
        let encoded = self.host.global("qs.stringify", vec![p])?;
        Ok(Some(
            self.host
                .global("decodeURIComponent", vec![encoded])?
                .to_js_string()?
                .to_vec(),
        ))
    }
    fn data(&mut self, c: &Js) -> Result<Option<Vec<u16>>, Thrown> {
        let d = c.get("data")?;
        if !d.truthy() {
            return Ok(None);
        }
        let s = self.host.global("JSON.stringify", vec![d])?;
        Ok(if s.is_undefined() {
            None
        } else {
            Some(s.to_js_string()?.to_vec())
        })
    }
    fn clock(&mut self) -> Result<f64, Thrown> {
        self.host
            .global("virtualClock.getCurrentTime", vec![])?
            .to_number()
    }
    fn url(&mut self, c: &Js) -> Result<Js, Thrown> {
        c.get("url")
    }
    fn create_hmac(&mut self, secret: Js) -> Result<Js, Thrown> {
        self.host
            .global("createHmac", vec![Js::str("sha256"), secret])
    }
    fn join(
        &mut self,
        url: Js,
        params: Vec<u16>,
        expiry: &[u16],
        data: Option<Vec<u16>>,
    ) -> Result<Vec<u16>, Thrown> {
        let mut out = if url.is_nullish() {
            vec![]
        } else {
            url.to_js_string()?.to_vec()
        };
        out.extend(params);
        out.extend(expiry);
        out.extend(data.unwrap_or_default());
        Ok(out)
    }
    fn hmac(&mut self, hmac: Js, payload: Vec<u16>) -> Result<Js, Thrown> {
        hmac.call_method("update", vec![Js::utf16(&payload)])?
            .call_method("digest", vec![Js::str("hex")])
    }
    fn headers(
        &mut self,
        config: Js,
        key: Js,
        expiry: Vec<u16>,
        signature: Js,
    ) -> Result<Js, Thrown> {
        let headers = config.get("headers")?.spread_object()?;
        for (k, v) in [
            ("Content-Type", Js::str("application/json")),
            ("X-PHEMEX-ACCESS-TOKEN", key),
            ("X-PHEMEX-REQUEST-EXPIRY", Js::utf16(&expiry)),
            ("X-PHEMEX-REQUEST-SIGNATURE", signature),
        ] {
            headers.define(k, v)?;
        }
        config.define("headers", headers)?;
        Ok(config)
    }
}
impl ApiHost for Api {
    type Client = Js;
    type RetryCondition = Js;
    fn testnet(&mut self) -> Result<bool, Thrown> {
        Ok(self.options.get("testnet")?.truthy())
    }
    fn create(
        &mut self,
        base: &'static str,
        timeout: f64,
        credentials: bool,
    ) -> Result<Js, Thrown> {
        client::factory::create(
            self.host.rt.shared.http.clone(),
            &*self.host.rt.shared.runtime.transport(),
            Js::object(vec![
                ("baseURL", Js::str(base)),
                ("timeout", Js::Number(timeout)),
                ("withCredentials", Js::Bool(credentials)),
            ]),
            &self.options,
            self.host.rt.base(),
        )
    }
    fn retry_condition(&mut self) -> Result<Js, Thrown> {
        Ok(Js::function("networkErrorOnRead", |_, a| {
            let error = arg(&a, 0);
            if !error.is_object() {
                return Ok(Js::Bool(false));
            }
            let config = error.get("config")?;
            if config.is_undefined() {
                return Ok(Js::Bool(false));
            }
            let method = text(&config.get("method")?)?.to_ascii_lowercase();
            Ok(Js::Bool(
                matches!(method.as_str(), "get" | "head")
                    && error.get("response")?.is_nullish()
                    && error.get("code")?.truthy()
                    && text(&error.get("code")?)? != "ECONNABORTED",
            ))
        }))
    }
    fn retry(&mut self, c: &Js, n: usize, cond: Js) -> Result<(), Thrown> {
        c.call_method(
            "__configureRetry",
            vec![Js::object(vec![
                ("retries", Js::Number(n as f64)),
                ("retryCondition", cond),
            ])],
        )?;
        Ok(())
    }
    fn tunnel(&mut self) -> Result<bool, Thrown> {
        Ok(self.options.get("tealstreetTunnel")?.truthy())
    }
    fn install(&mut self, c: &Js) -> Result<(), Thrown> {
        let callback = self
            .host
            .rt
            .shared
            .extra
            .install_transport
            .clone()
            .ok_or_else(|| {
                Thrown::error(
                    "Error",
                    "Phemex tunnel requires an installTransportAdapter provider",
                )
            })?;
        callback(
            c.clone(),
            self.options.clone(),
            self.host.rt.shared.extra.name.clone(),
        )
    }
    fn interceptor(
        &mut self,
        c: &Js,
        callback: fn(&mut Self, Js) -> Result<Js, Thrown>,
    ) -> Result<(), Thrown> {
        let weak = std::rc::Rc::downgrade(&self.host.rt.shared);
        let options = self.options.clone();
        c.get("interceptors")?.get("request")?.call_method(
            "use",
            vec![Js::function("signPhemexRequest", move |_, a| {
                callback(
                    &mut Api {
                        host: Host {
                            rt: safe_cex_exchange::js_runtime::scope::scope_of(&weak)?,
                        },
                        options: options.clone(),
                    },
                    arg(&a, 0),
                )
            })],
        )?;
        Ok(())
    }
}
