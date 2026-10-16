//! Public socket setup and routing over opaque frames, callbacks and transports.
#[derive(Clone)]
pub enum Parameter {
    Text(Vec<u16>),
    Number(f64),
    Undefined,
}
pub struct Request {
    pub method: Vec<u16>,
    pub params: Vec<Parameter>,
    pub id: f64,
    pub id_first: bool,
}
pub trait RoutingHost {
    type Value: Clone;
    type Socket;
    type Call;
    type Callbacks;
    type Iterator;
    type Callback;
    type Error;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    fn testnet(&mut self) -> Result<bool, Self::Error>;
    fn create(&mut self, url: &'static str) -> Result<Self::Socket, Self::Error>;
    fn socket(&mut self, socket: Self::Socket) -> Result<(), Self::Error>;
    fn listener(&mut self, event: &'static str, member: &'static str) -> Result<(), Self::Error>;
    fn send_target(&mut self) -> Result<Self::Call, Self::Error>;
    fn serialize(&mut self, request: Request) -> Result<Vec<u16>, Self::Error>;
    fn send(&mut self, call: Self::Call, text: Vec<u16>) -> Result<(), Self::Error>;
    fn reset(&mut self) -> Result<(), Self::Error>;
    fn subscribe(&mut self) -> Result<(), Self::Error>;
    fn ping(&mut self) -> Result<(), Self::Error>;
    fn topic_keys(&mut self) -> Result<Vec<Vec<u16>>, Self::Error>;
    fn number_text(&mut self, text: Option<&[u16]>) -> Result<f64, Self::Error>;
    fn topic_values(&mut self) -> Result<Vec<f64>, Self::Error>;
    fn warned(&mut self) -> Result<bool, Self::Error>;
    fn set_warned(&mut self, warned: bool) -> Result<(), Self::Error>;
    fn warning(&mut self, message: Vec<u16>) -> Result<(), Self::Error>;
    fn now(&mut self) -> Result<f64, Self::Error>;
    fn set_ping_at(&mut self, time: f64) -> Result<(), Self::Error>;
    fn next_request_id(&mut self) -> Result<f64, Self::Error>;
    fn number_string(&mut self, number: f64) -> Result<Vec<u16>, Self::Error>;
    fn callbacks(&mut self, topic: &[u16]) -> Result<Option<Self::Callbacks>, Self::Error>;
    fn nonempty(&mut self, callbacks: &Self::Callbacks) -> Result<bool, Self::Error>;
    fn stats(&mut self, topic: &[u16]) -> Result<(), Self::Error>;
    fn iterate_callbacks(
        &mut self,
        callbacks: Self::Callbacks,
    ) -> Result<Self::Iterator, Self::Error>;
    fn next_callback(
        &mut self,
        iterator: &mut Self::Iterator,
    ) -> Result<Option<Self::Callback>, Self::Error>;
    fn invoke(
        &mut self,
        callback: Self::Callback,
        payload: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn number(&mut self, value: Self::Value) -> Result<f64, Self::Error>;
    fn interval_cache_exists(&mut self) -> Result<bool, Self::Error>;
    fn create_interval_cache(&mut self) -> Result<(), Self::Error>;
    fn timeframes(&mut self) -> Result<Self::Value, Self::Error>;
    fn timeframe_keys(&mut self, frames: &Self::Value) -> Result<Vec<Vec<u16>>, Self::Error>;
    fn timeframe(&mut self, frames: &Self::Value, key: &[u16]) -> Result<Self::Value, Self::Error>;
    fn cache_interval(&mut self, number: f64, interval: Vec<u16>) -> Result<(), Self::Error>;
    fn cached_interval(&mut self, number: f64) -> Result<Option<Vec<u16>>, Self::Error>;
    fn received(&mut self) -> Result<(), Self::Error>;
    fn parse(&mut self, data: Self::Value) -> Result<Self::Value, Self::Error>;
    fn property(
        &mut self,
        value: &Self::Value,
        name: &'static str,
    ) -> Result<Self::Value, Self::Error>;
    fn truthy(&mut self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn text(&mut self, value: &Self::Value) -> Result<Option<Vec<u16>>, Self::Error>;
    fn array(&mut self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn iterate_values(&mut self, value: Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next_value(
        &mut self,
        iterator: &mut Self::Iterator,
    ) -> Result<Option<Self::Value>, Self::Error>;
    fn index(&mut self, value: &Self::Value, index: usize) -> Result<Self::Value, Self::Error>;
    fn wrap(&mut self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn dispatch(
        &mut self,
        topic: Vec<u16>,
        value: Self::Value,
        each: Option<bool>,
    ) -> Result<(), Self::Error>;
    fn pong(&mut self) -> Result<(), Self::Error>;
}
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn send_request<H: RoutingHost>(host: &mut H, request: Request) -> Result<(), H::Error> {
    let call = host.send_target()?;
    let text = host.serialize(request)?;
    host.send(call, text)
}
pub fn connect_and_subscribe<H: RoutingHost>(host: &mut H) -> Result<(), H::Error> {
    if !host.disposed()? {
        let network = if host.testnet()? {
            "testnet"
        } else {
            "livenet"
        };
        let url = crate::constants::BASE_WS_URL
            .iter()
            .find(|(name, _)| *name == "public")
            .unwrap()
            .1
            .iter()
            .find(|(name, _)| *name == network)
            .unwrap()
            .1;
        let socket = host.create(url)?;
        host.socket(socket)?;
        for (event, member) in [
            ("open", "onOpen"),
            ("message", "onMessage"),
            ("close", "onClose"),
        ] {
            host.listener(event, member)?;
        }
    }
    Ok(())
}
pub fn on_open<H: RoutingHost>(host: &mut H) -> Result<(), H::Error> {
    if !host.disposed()? {
        send_request(
            host,
            Request {
                method: units("perp_market24h_pack_p.subscribe"),
                params: Vec::new(),
                id: 1.0,
                id_first: true,
            },
        )?;
        host.reset()?;
        host.subscribe()?;
        host.ping()?;
    }
    Ok(())
}
fn split_topic(topic: &[u16]) -> Vec<Vec<u16>> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i + 3 <= topic.len() {
        if topic[i..i + 3] == [124, 124, 124] {
            parts.push(topic[start..i].to_vec());
            i += 3;
            start = i;
        } else {
            i += 1;
        }
    }
    parts.push(topic[start..].to_vec());
    parts
}
pub fn subscribe<H: RoutingHost>(host: &mut H) -> Result<(), H::Error> {
    let topics = host.topic_keys()?;
    let mut id = 10.0;
    for topic in topics {
        let parts = split_topic(&topic);
        let method = parts[0].clone();
        let call = host.send_target()?;
        let params = if method == units("kline_p.subscribe") {
            vec![
                parts
                    .get(1)
                    .cloned()
                    .map(Parameter::Text)
                    .unwrap_or(Parameter::Undefined),
                Parameter::Number(host.number_text(parts.get(2).map(Vec::as_slice))?),
            ]
        } else {
            parts[1..].iter().cloned().map(Parameter::Text).collect()
        };
        let text = host.serialize(Request {
            method,
            params,
            id,
            id_first: false,
        })?;
        host.send(call, text)?;
        id += 1.0;
    }
    Ok(())
}
pub fn update_subscription_limit_warning<H: RoutingHost>(host: &mut H) -> Result<(), H::Error> {
    let active = 1 + host
        .topic_values()?
        .into_iter()
        .filter(|n| *n > 0.0)
        .count();
    if active >= 18 && !host.warned()? {
        host.set_warned(true)?;
        host.warning(units(&format!("[Phemex] Public WebSocket has {active}/20 active subscriptions; additional charts or orderbooks may be rejected.")))?;
    } else if active < 18 {
        host.set_warned(false)?;
    }
    Ok(())
}
pub fn ping<H: RoutingHost>(host: &mut H) -> Result<(), H::Error> {
    if !host.disposed()? {
        let now = host.now()?;
        host.set_ping_at(now)?;
        let call = host.send_target()?;
        let id = host.next_request_id()?;
        let text = units("{\"id\":")
            .into_iter()
            .chain(host.number_string(id)?)
            .chain(units(",\"method\":\"server.ping\",\"params\":[]}"))
            .collect();
        host.send(call, text)?;
    }
    Ok(())
}
pub fn dispatch_topic<H: RoutingHost>(
    host: &mut H,
    topic: &[u16],
    payload: H::Value,
    each: bool,
) -> Result<(), H::Error> {
    let Some(callbacks) = host.callbacks(topic)? else {
        return Ok(());
    };
    if !host.nonempty(&callbacks)? {
        return Ok(());
    }
    if !each {
        host.stats(topic)?;
    }
    let mut iterator = host.iterate_callbacks(callbacks)?;
    while let Some(callback) = host.next_callback(&mut iterator)? {
        if each {
            host.stats(topic)?;
        }
        host.invoke(callback, &payload)?;
    }
    Ok(())
}
pub fn get_kline_interval<H: RoutingHost>(
    host: &mut H,
    resolution: H::Value,
) -> Result<Option<Vec<u16>>, H::Error> {
    let numeric = host.number(resolution)?;
    if !numeric.is_finite() {
        return Ok(None);
    }
    if !host.interval_cache_exists()? {
        let frames = host.timeframes()?;
        host.create_interval_cache()?;
        for key in host.timeframe_keys(&frames)? {
            let value = host.timeframe(&frames, &key)?;
            let number = host.number(value)?;
            host.cache_interval(number, key)?;
        }
    }
    host.cached_interval(numeric)
}
pub fn on_message<H: RoutingHost>(host: &mut H, data: H::Value) -> Result<(), H::Error> {
    if host.disposed()? {
        return Ok(());
    }
    host.received()?;
    let json = host.parse(data)?;
    let method = host.property(&json, "method")?;
    if host.text(&method)? == Some(units("server.ping")) {
        return host.pong();
    }
    let result = host.property(&json, "result")?;
    if host.text(&result)? == Some(units("pong")) {
        return host.pong();
    }
    let symbol = host.property(&json, "symbol")?;
    let kline = host.property(&json, "kline_p")?;
    if host.truthy(&kline)? {
        if let Some(symbol) = host.text(&symbol)? {
            let mut seen = Vec::<Vec<u16>>::new();
            let rows = host.property(&json, "kline_p")?;
            let mut iterator = host.iterate_values(rows)?;
            while let Some(row) = host.next_value(&mut iterator)? {
                let interval = if host.array(&row)? {
                    let res = host.index(&row, 1)?;
                    get_kline_interval(host, res)?
                } else {
                    None
                };
                let Some(interval) = interval.filter(|v| !v.is_empty()) else {
                    continue;
                };
                let topic = units("kline_p.")
                    .into_iter()
                    .chain(interval)
                    .chain([46])
                    .chain(symbol.iter().copied())
                    .collect::<Vec<_>>();
                if !seen.contains(&topic) {
                    seen.push(topic.clone());
                    let payload = host.property(&json, "kline_p")?;
                    host.dispatch(topic, payload, None)?;
                }
            }
            return Ok(());
        }
    }
    let book = host.property(&json, "orderbook_p")?;
    if host.truthy(&book)? {
        if let Some(symbol) = host.text(&symbol)? {
            let topic = units("orderbook_p.").into_iter().chain(symbol).collect();
            let payload = host.property(&json, "orderbook_p")?;
            return host.dispatch(topic, payload, None);
        }
    }
    let method = host.property(&json, "method")?;
    if let Some(method) = host.text(&method)? {
        let payload = if host.array(&json)? {
            json
        } else {
            host.wrap(json)?
        };
        host.dispatch(method, payload, Some(true))?;
    }
    Ok(())
}

#[cfg(test)]
mod accepted_native_contract {
    use super::*;

    #[test]
    fn utf16_encoder_preserves_native_units() {
        assert_eq!(units(""), Vec::<u16>::new());
        assert_eq!(units("A🚀\0"), vec![65, 55357, 56960, 0]);
        assert_eq!(units("é雪"), vec![233, 38634]);
    }
}
