use super::rt::Host;
use crate::session_control::SessionHost;
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
impl SessionHost for Host {
    type Value = Js;
    type Error = Thrown;
    fn base_dispose(&mut self) -> Result<(), Thrown> {
        self.rt.base().dispose().map_err(Thrown::from)
    }
    fn socket_dispose(&mut self, k: &'static str) -> Result<(), Thrown> {
        self.get(k)?.call_method("dispose", vec![])?;
        Ok(())
    }
    fn connect_private(&mut self) -> Result<(), Thrown> {
        self.get("privateWebsocket")?
            .call_method("connectAndSubscribe", vec![])?;
        Ok(())
    }
    fn set_setting(&mut self, k: &'static str, v: bool) -> Result<(), Thrown> {
        self.get("store")?
            .call_method("setSetting", vec![Js::str(k), Js::Bool(v)])?;
        Ok(())
    }
    async fn initial_tick(&mut self) -> Result<(), Thrown> {
        self.invoke("tick", vec![]).await?;
        Ok(())
    }
    fn disposed(&mut self) -> Result<bool, Thrown> {
        Ok(self.get("isDisposed")?.truthy())
    }
    fn log(&mut self, s: &'static str) -> Result<(), Thrown> {
        self.call("log", vec![Js::str(s)])?;
        Ok(())
    }
    fn empty_array(&mut self) -> Result<Js, Thrown> {
        Ok(Js::array(vec![]))
    }
    fn zero_balance(&mut self, f: [(&'static str, f64); 4]) -> Result<Js, Thrown> {
        Ok(Js::object(
            f.into_iter().map(|(k, v)| (k, Js::Number(v))).collect(),
        ))
    }
    fn loaded(&mut self) -> Result<Js, Thrown> {
        self.store("loaded")
    }
    fn update(
        &mut self,
        mut fields: Vec<(&'static str, Js)>,
        loaded: Js,
        flags: Vec<&'static str>,
    ) -> Result<(), Thrown> {
        let loaded = loaded.spread_object()?;
        for k in flags {
            loaded.define(k, Js::Bool(true))?;
        }
        fields.push(("loaded", loaded));
        self.get("store")?
            .call_method("update", vec![Js::object(fields)])?;
        Ok(())
    }
    async fn fetch_positions_balance(&mut self) -> Result<(Js, Js), Thrown> {
        let v = self.invoke("fetchPositionsAndBalance", vec![]).await?;
        Ok((v.get("balance")?, v.get("positions")?))
    }
    fn add_positions(&mut self, p: Js) -> Result<(), Thrown> {
        self.get("store")?
            .call_method("addOrUpdatePositionsFull", vec![p])?;
        Ok(())
    }
    fn report_message(&mut self, e: &Thrown) -> Result<(), Thrown> {
        self.emit(e.value.get("message")?)
    }
    fn tick_interval(&mut self) -> Result<Js, Thrown> {
        self.call("getTickInterval", vec![])
    }
    fn schedule_tick(&mut self, interval: Js, method: &'static str) -> Result<(), Thrown> {
        let this = self.rt.this.clone();
        self.global(
            "loop",
            vec![
                Js::function(method, move |_, _| this.call_method(method, vec![])),
                interval,
            ],
        )?;
        Ok(())
    }
}
