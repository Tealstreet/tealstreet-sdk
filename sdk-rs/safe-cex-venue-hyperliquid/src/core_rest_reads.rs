//! REST metadata, candles and history query bodies and owned callbacks.
//! Source async exports enter eager owned tasks through the venue-local runtime binding.
use crate::{
    core_position_order_state::PositionOrderHost,
    helper_tasks::{HelperObserver, HelperTaskHost},
    spot_json::JsString,
};
#[derive(Clone, Debug)]
pub enum RestCallback<V> {
    FetchHip3Dexs0([V; 0]),
    FetchHip3Dexs1([V; 0]),
    FetchHip3Dexs2([V; 0]),
    FetchHip3Markets0([V; 1]),
    FetchOHLCV0([V; 2]),
    FetchOHLCV1([V; 0]),
    FetchIncomeHistory0([V; 1]),
    FetchIncomeHistory1([V; 1]),
    FetchIncomeHistory2([V; 2]),
    FetchIncomeHistory3([V; 0]),
    FetchIncomeHistory4([V; 0]),
    FetchIncomeHistory5([V; 1]),
    FetchIncomeHistory6([V; 0]),
    FetchHistoricalOrders0([V; 1]),
    FetchHistoricalOrders1([V; 1]),
    FetchHistoricalOrders2([V; 0]),
    FetchHistoricalOrders3([V; 1]),
    FetchHistoricalOrders4([V; 0]),
    DeriveClosedPositions0([V; 1]),
    DeriveClosedPositions1([V; 2]),
    DeriveClosedPositions2([V; 0]),
    DeriveClosedPositions3([V; 1]),
    FetchHistoricalFills0([V; 0]),
    FetchHistoricalFills1([V; 0]),
    FetchHistoricalFills2([V; 0]),
    FetchHistoricalFills3([V; 0]),
    FetchOHLCV2([V; 1]),
}
impl<V> RestCallback<V> {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::FetchHip3Dexs0(_) => "fetchHip3Dexs_0",
            Self::FetchHip3Dexs1(_) => "fetchHip3Dexs_1",
            Self::FetchHip3Dexs2(_) => "fetchHip3Dexs_2",
            Self::FetchHip3Markets0(_) => "fetchHip3Markets_0",
            Self::FetchOHLCV0(_) => "fetchOHLCV_0",
            Self::FetchOHLCV1(_) => "fetchOHLCV_1",
            Self::FetchIncomeHistory0(_) => "fetchIncomeHistory_0",
            Self::FetchIncomeHistory1(_) => "fetchIncomeHistory_1",
            Self::FetchIncomeHistory2(_) => "fetchIncomeHistory_2",
            Self::FetchIncomeHistory3(_) => "fetchIncomeHistory_3",
            Self::FetchIncomeHistory4(_) => "fetchIncomeHistory_4",
            Self::FetchIncomeHistory5(_) => "fetchIncomeHistory_5",
            Self::FetchIncomeHistory6(_) => "fetchIncomeHistory_6",
            Self::FetchHistoricalOrders0(_) => "fetchHistoricalOrders_0",
            Self::FetchHistoricalOrders1(_) => "fetchHistoricalOrders_1",
            Self::FetchHistoricalOrders2(_) => "fetchHistoricalOrders_2",
            Self::FetchHistoricalOrders3(_) => "fetchHistoricalOrders_3",
            Self::FetchHistoricalOrders4(_) => "fetchHistoricalOrders_4",
            Self::DeriveClosedPositions0(_) => "deriveClosedPositions_0",
            Self::DeriveClosedPositions1(_) => "deriveClosedPositions_1",
            Self::DeriveClosedPositions2(_) => "deriveClosedPositions_2",
            Self::DeriveClosedPositions3(_) => "deriveClosedPositions_3",
            Self::FetchHistoricalFills0(_) => "fetchHistoricalFills_0",
            Self::FetchHistoricalFills1(_) => "fetchHistoricalFills_1",
            Self::FetchHistoricalFills2(_) => "fetchHistoricalFills_2",
            Self::FetchHistoricalFills3(_) => "fetchHistoricalFills_3",
            Self::FetchOHLCV2(_) => "fetchOHLCV_2",
        }
    }
    pub fn is_async(&self) -> bool {
        matches!(
            self,
            Self::FetchIncomeHistory2(_) | Self::DeriveClosedPositions1(_)
        )
    }
}
enum RestFlow<V> {
    Normal,
    Return(V),
    Next,
    Break,
}
pub trait RestReadHost: PositionOrderHost {
    fn rest_callback(
        &mut self,
        callback: RestCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn rest_exception(&self, value: Self::Value) -> Self::Error;
    fn rest_array_binding(
        &mut self,
        value: &Self::Value,
        mask: &[bool],
    ) -> Result<Vec<Self::Value>, Self::Error>;
    fn rest_argument_spread(
        &mut self,
        args: &mut Vec<Self::Value>,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
}
pub trait RestTaskHost: RestReadHost + HelperTaskHost<Self::Value, Self::Error>
where
    Self::Value: 'static,
    Self::Error: Clone + 'static,
{
}
impl<H> RestTaskHost for H
where
    H: RestReadHost + HelperTaskHost<H::Value, H::Error>,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
}
fn lit<H: RestReadHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
pub fn run_rest_callback<H: RestReadHost>(
    h: &mut H,
    callback: &RestCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    match callback {
        RestCallback::FetchHip3Dexs0(_) => {
            let dex = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let index = args.get(1).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let condition = dex.clone();
                    if h.truthy(&condition) {
                        {
                            let object = h.object()?;
                            let value = dex.clone();
                            h.spread_into(&object, &value)?;
                            let value = index.clone();
                            h.define_data(&object, "dexPosition", &value)?;
                            object
                        }
                    } else {
                        h.null_value()
                    }
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchHip3Dexs1(_) => {
            let dex = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let left = dex.clone();
                    let right = h.null_value();
                    h.boolean(!h.strict_equal(&left, &right))
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchHip3Dexs2(_) => {
            let dex = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let receiver = {
                        let object = h.global("HyperliquidExchange")?;
                        h.get(&object, "TESTNET_HIP3_WHITELIST")?
                    };
                    let function = h.get(&receiver, "includes")?;
                    let mut arguments = Vec::new();
                    let value = {
                        let object = dex.clone();
                        h.get(&object, "name")?
                    };
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchHip3Markets0(captures) => {
            let this_value = captures[0].clone();
            let message = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let severity = {
                let value = args.get(1).cloned().unwrap_or_else(|| h.undefined());
                if h.strict_equal(&value, &h.undefined()) {
                    {
                        let object = h.global("LogSeverity")?;
                        h.get(&object, "Info")?
                    }
                } else {
                    value
                }
            };
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "log")?;
                    let mut arguments = Vec::new();
                    let value = message.clone();
                    arguments.push(value);
                    let value = severity.clone();
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchOHLCV0(captures) => {
            let this_value = captures[0].clone();
            let opts = captures[1].clone();
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                let market_id = {
                    'optional: {
                        let mut object = {
                            'call: {
                                let receiver = {
                                    let object = {
                                        let object = this_value.clone();
                                        h.get(&object, "store")?
                                    };
                                    h.get(&object, "markets")?
                                };
                                if h.is_nullish(&receiver) {
                                    break 'call h.undefined();
                                }
                                let function = h.get(&receiver, "find")?;
                                if h.is_nullish(&function) {
                                    break 'call h.undefined();
                                }
                                let mut arguments = Vec::new();
                                let value =
                                    h.rest_callback(RestCallback::FetchOHLCV2([opts.clone()]))?;
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            }
                        };
                        if h.is_nullish(&object) {
                            break 'optional h.undefined();
                        }
                        object = h.get(&object, "id")?;
                        object
                    }
                };
                if {
                    let condition = market_id.clone();
                    h.truthy(&condition)
                } {
                    return Ok(RestFlow::Return(market_id.clone()));
                }
                let symbol_without_perp = {
                    let receiver = {
                        let object = opts.clone();
                        h.get(&object, "symbol")?
                    };
                    let function = h.get(&receiver, "replace")?;
                    let mut arguments = Vec::new();
                    let value = h.regex("/-PERP$/i")?;
                    arguments.push(value);
                    let value = lit(h, "");
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                };
                if {
                    let condition = {
                        let receiver = symbol_without_perp.clone();
                        let function = h.get(&receiver, "includes")?;
                        let mut arguments = Vec::new();
                        let value = lit(h, "|");
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    h.truthy(&condition)
                } {
                    let binding = {
                        let receiver = symbol_without_perp.clone();
                        let function = h.get(&receiver, "split")?;
                        let mut arguments = Vec::new();
                        let value = lit(h, "|");
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    let elements = h.rest_array_binding(&binding, &[true, true])?;
                    let dex = elements[0].clone();
                    let asset = elements[1].clone();
                    return Ok(RestFlow::Return({
                        let mut units: Vec<u16> = vec![];
                        let value = {
                            let receiver = dex.clone();
                            let function = h.get(&receiver, "toLowerCase")?;
                            let arguments = Vec::new();
                            h.call(&function, &receiver, &arguments)?
                        };
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[58]);
                        let value = asset.clone();
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[]);
                        h.literal(&JsString(units))
                    }));
                }
                return Ok(RestFlow::Return({
                    let receiver = symbol_without_perp.clone();
                    let function = h.get(&receiver, "toLowerCase")?;
                    let arguments = Vec::new();
                    h.call(&function, &receiver, &arguments)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchOHLCV1(_) => {
            let binding = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let t = {
                let value = h.get(&binding, "t")?;
                value
            };
            let o = {
                let value = h.get(&binding, "o")?;
                value
            };
            let candle_high = {
                let value = h.get(&binding, "h")?;
                value
            };
            let l = {
                let value = h.get(&binding, "l")?;
                value
            };
            let c = {
                let value = h.get(&binding, "c")?;
                value
            };
            let v = {
                let value = h.get(&binding, "v")?;
                value
            };
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let object = h.object()?;
                    let value = {
                        let left = t.clone();
                        let right = h.number(1000.0);
                        h.position_operator("divide", &left, &right)?
                    };
                    h.define_data(&object, "timestamp", &value)?;
                    let value = {
                        let function = h.global("parseFloat")?;
                        let receiver = h.undefined();
                        let mut arguments = Vec::new();
                        let value = o.clone();
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    h.define_data(&object, "open", &value)?;
                    let value = {
                        let function = h.global("parseFloat")?;
                        let receiver = h.undefined();
                        let mut arguments = Vec::new();
                        let value = candle_high.clone();
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    h.define_data(&object, "high", &value)?;
                    let value = {
                        let function = h.global("parseFloat")?;
                        let receiver = h.undefined();
                        let mut arguments = Vec::new();
                        let value = l.clone();
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    h.define_data(&object, "low", &value)?;
                    let value = {
                        let function = h.global("parseFloat")?;
                        let receiver = h.undefined();
                        let mut arguments = Vec::new();
                        let value = c.clone();
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    h.define_data(&object, "close", &value)?;
                    let value = {
                        let function = h.global("parseFloat")?;
                        let receiver = h.undefined();
                        let mut arguments = Vec::new();
                        let value = v.clone();
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    h.define_data(&object, "volume", &value)?;
                    object
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchIncomeHistory0(captures) => {
            let this_value = captures[0].clone();
            let coin = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "resolveCoinToSymbol")?;
                    let mut arguments = Vec::new();
                    let value = coin.clone();
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchIncomeHistory1(captures) => {
            let this_value = captures[0].clone();
            let request_type = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let pages = args.get(1).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "log")?;
                    let mut arguments = Vec::new();
                    let value = {
                        let mut units: Vec<u16> = vec![
                            102, 101, 116, 99, 104, 73, 110, 99, 111, 109, 101, 72, 105, 115, 116,
                            111, 114, 121, 32, 115, 116, 111, 112, 112, 101, 100, 32, 97, 116, 32,
                            116, 104, 101, 32,
                        ];
                        let value = pages.clone();
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[
                            45, 112, 97, 103, 101, 32, 99, 97, 112, 32, 102, 111, 114, 32,
                        ]);
                        let value = request_type.clone();
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[
                            59, 32, 111, 108, 100, 101, 114, 32, 114, 111, 119, 115, 32, 105, 110,
                            32, 116, 104, 105, 115, 32, 119, 105, 110, 100, 111, 119, 32, 119, 101,
                            114, 101, 32, 110, 111, 116, 32, 102, 101, 116, 99, 104, 101, 100,
                        ]);
                        h.literal(&JsString(units))
                    };
                    arguments.push(value);
                    let value = {
                        let object = h.global("LogSeverity")?;
                        h.get(&object, "Warning")?
                    };
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchIncomeHistory3(_) => {
            let row = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let function = h.global("String")?;
                    let receiver = h.undefined();
                    let mut arguments = Vec::new();
                    let value = {
                        let left = {
                            let object = row.clone();
                            h.get(&object, "tid")?
                        };
                        if !h.is_nullish(&left) {
                            left
                        } else {
                            {
                                let mut units: Vec<u16> = vec![];
                                let value = {
                                    let object = row.clone();
                                    h.get(&object, "time")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[58]);
                                let value = {
                                    let object = row.clone();
                                    h.get(&object, "coin")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[58]);
                                let value = {
                                    let object = row.clone();
                                    h.get(&object, "px")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[58]);
                                let value = {
                                    let object = row.clone();
                                    h.get(&object, "sz")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[]);
                                h.literal(&JsString(units))
                            }
                        }
                    };
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchIncomeHistory4(_) => {
            let row = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                let delta = {
                    let left = {
                        let object = row.clone();
                        h.get(&object, "delta")?
                    };
                    if !h.is_nullish(&left) {
                        left
                    } else {
                        row.clone()
                    }
                };
                return Ok(RestFlow::Return({
                    let mut units: Vec<u16> = vec![];
                    let value = {
                        let left = {
                            let object = row.clone();
                            h.get(&object, "hash")?
                        };
                        if !h.is_nullish(&left) {
                            left
                        } else {
                            lit(h, "")
                        }
                    };
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[58]);
                    let value = {
                        let left = {
                            let object = delta.clone();
                            h.get(&object, "coin")?
                        };
                        if !h.is_nullish(&left) {
                            left
                        } else {
                            lit(h, "")
                        }
                    };
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[58]);
                    let value = {
                        let left = {
                            let object = row.clone();
                            h.get(&object, "time")?
                        };
                        if !h.is_nullish(&left) {
                            left
                        } else {
                            lit(h, "")
                        }
                    };
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[]);
                    h.literal(&JsString(units))
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchIncomeHistory5(captures) => {
            let opts = captures[0].clone();
            let record = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let left = {
                        let object = record.clone();
                        h.get(&object, "symbol")?
                    };
                    let right = {
                        let object = opts.clone();
                        h.get(&object, "symbol")?
                    };
                    h.boolean(h.strict_equal(&left, &right))
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchIncomeHistory6(_) => {
            let a = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let b = args.get(1).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let left = {
                        let object = b.clone();
                        h.get(&object, "timestamp")?
                    };
                    let right = {
                        let object = a.clone();
                        h.get(&object, "timestamp")?
                    };
                    h.position_operator("subtract", &left, &right)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchHistoricalOrders0(captures) => {
            let this_value = captures[0].clone();
            let coin = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "resolveCoinToSymbol")?;
                    let mut arguments = Vec::new();
                    let value = coin.clone();
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchHistoricalOrders1(captures) => {
            let resolve_symbol = captures[0].clone();
            let row = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let function = h.global("mapHyperliquidHistoricalOrder")?;
                    let receiver = h.undefined();
                    let mut arguments = Vec::new();
                    let value = row.clone();
                    arguments.push(value);
                    let value = resolve_symbol.clone();
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchHistoricalOrders2(_) => {
            let order = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let left = order.clone();
                    let right = h.null_value();
                    h.boolean(!h.strict_equal(&left, &right))
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchHistoricalOrders3(captures) => {
            let opts = captures[0].clone();
            let order = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                if {
                    let condition = {
                        let left = {
                            'optional: {
                                let mut object = opts.clone();
                                if h.is_nullish(&object) {
                                    break 'optional h.undefined();
                                }
                                object = h.get(&object, "symbol")?;
                                object
                            }
                        };
                        if !h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = {
                                    let object = order.clone();
                                    h.get(&object, "symbol")?
                                };
                                let right = {
                                    let object = opts.clone();
                                    h.get(&object, "symbol")?
                                };
                                h.boolean(!h.strict_equal(&left, &right))
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    return Ok(RestFlow::Return(h.boolean(false)));
                }
                if {
                    let condition = {
                        let left = {
                            'optional: {
                                let mut object = opts.clone();
                                if h.is_nullish(&object) {
                                    break 'optional h.undefined();
                                }
                                object = h.get(&object, "from")?;
                                object
                            }
                        };
                        if !h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = {
                                    let object = order.clone();
                                    h.get(&object, "createdTime")?
                                };
                                let right = {
                                    let object = opts.clone();
                                    h.get(&object, "from")?
                                };
                                h.position_operator("less", &left, &right)?
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    return Ok(RestFlow::Return(h.boolean(false)));
                }
                if {
                    let condition = {
                        let left = {
                            'optional: {
                                let mut object = opts.clone();
                                if h.is_nullish(&object) {
                                    break 'optional h.undefined();
                                }
                                object = h.get(&object, "to")?;
                                object
                            }
                        };
                        if !h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = {
                                    let object = order.clone();
                                    h.get(&object, "createdTime")?
                                };
                                let right = {
                                    let object = opts.clone();
                                    h.get(&object, "to")?
                                };
                                h.position_operator("greater", &left, &right)?
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    return Ok(RestFlow::Return(h.boolean(false)));
                }
                return Ok(RestFlow::Return(h.boolean(true)));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchHistoricalOrders4(_) => {
            let a = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let b = args.get(1).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let left = {
                        let object = b.clone();
                        h.get(&object, "createdTime")?
                    };
                    let right = {
                        let object = a.clone();
                        h.get(&object, "createdTime")?
                    };
                    h.position_operator("subtract", &left, &right)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::DeriveClosedPositions0(captures) => {
            let this_value = captures[0].clone();
            let coin = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "resolveCoinToSymbol")?;
                    let mut arguments = Vec::new();
                    let value = coin.clone();
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::DeriveClosedPositions2(_) => {
            let row = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let function = h.global("String")?;
                    let receiver = h.undefined();
                    let mut arguments = Vec::new();
                    let value = {
                        let left = {
                            let object = row.clone();
                            h.get(&object, "tid")?
                        };
                        if !h.is_nullish(&left) {
                            left
                        } else {
                            {
                                let mut units: Vec<u16> = vec![];
                                let value = {
                                    let object = row.clone();
                                    h.get(&object, "time")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[58]);
                                let value = {
                                    let object = row.clone();
                                    h.get(&object, "coin")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[58]);
                                let value = {
                                    let object = row.clone();
                                    h.get(&object, "px")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[58]);
                                let value = {
                                    let object = row.clone();
                                    h.get(&object, "sz")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[]);
                                h.literal(&JsString(units))
                            }
                        }
                    };
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::DeriveClosedPositions3(captures) => {
            let opts = captures[0].clone();
            let entry = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let left = {
                        let object = entry.clone();
                        h.get(&object, "symbol")?
                    };
                    let right = {
                        let object = opts.clone();
                        h.get(&object, "symbol")?
                    };
                    h.boolean(h.strict_equal(&left, &right))
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchHistoricalFills0(_) => {
            let value = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                if {
                    let condition = {
                        let left = {
                            let left = {
                                let left = value.clone();
                                let right = h.undefined();
                                h.boolean(h.strict_equal(&left, &right))
                            };
                            if h.truthy(&left) {
                                left
                            } else {
                                {
                                    let left = value.clone();
                                    let right = h.null_value();
                                    h.boolean(h.strict_equal(&left, &right))
                                }
                            }
                        };
                        if h.truthy(&left) {
                            left
                        } else {
                            {
                                let value = {
                                    let receiver = h.global("Number")?;
                                    let function = h.get(&receiver, "isFinite")?;
                                    let mut arguments = Vec::new();
                                    let value = value.clone();
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                                h.boolean(!h.truthy(&value))
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    return Ok(RestFlow::Return(h.undefined()));
                }
                return Ok(RestFlow::Return({
                    let condition = {
                        let left = value.clone();
                        let right = h.number(1000000000000.0);
                        h.position_operator("greater", &left, &right)?
                    };
                    if h.truthy(&condition) {
                        {
                            let receiver = h.global("Math")?;
                            let function = h.get(&receiver, "floor")?;
                            let mut arguments = Vec::new();
                            let value = value.clone();
                            arguments.push(value);
                            h.call(&function, &receiver, &arguments)?
                        }
                    } else {
                        {
                            let receiver = h.global("Math")?;
                            let function = h.get(&receiver, "floor")?;
                            let mut arguments = Vec::new();
                            let value = {
                                let left = value.clone();
                                let right = h.number(1000.0);
                                h.position_operator("multiply", &left, &right)?
                            };
                            arguments.push(value);
                            h.call(&function, &receiver, &arguments)?
                        }
                    }
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchHistoricalFills1(_) => {
            let value = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let receiver = {
                        let mut units: Vec<u16> = vec![];
                        let value = {
                            let left = value.clone();
                            if h.truthy(&left) { left } else { lit(h, "") }
                        };
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[]);
                        h.literal(&JsString(units))
                    };
                    let function = h.get(&receiver, "trim")?;
                    let arguments = Vec::new();
                    h.call(&function, &receiver, &arguments)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchHistoricalFills2(_) => {
            let value = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                let normalized_side = {
                    let receiver = {
                        let receiver = {
                            let mut units: Vec<u16> = vec![];
                            let value = {
                                let left = value.clone();
                                if h.truthy(&left) { left } else { lit(h, "") }
                            };
                            units.extend(h.template_string(&value)?.0);
                            units.extend_from_slice(&[]);
                            h.literal(&JsString(units))
                        };
                        let function = h.get(&receiver, "trim")?;
                        let arguments = Vec::new();
                        h.call(&function, &receiver, &arguments)?
                    };
                    let function = h.get(&receiver, "toUpperCase")?;
                    let arguments = Vec::new();
                    h.call(&function, &receiver, &arguments)?
                };
                if {
                    let condition = {
                        let left = {
                            let left = normalized_side.clone();
                            let right = lit(h, "B");
                            h.boolean(h.strict_equal(&left, &right))
                        };
                        if h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = normalized_side.clone();
                                let right = lit(h, "BUY");
                                h.boolean(h.strict_equal(&left, &right))
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    return Ok(RestFlow::Return({
                        let object = h.global("OrderSide")?;
                        h.get(&object, "Buy")?
                    }));
                }
                if {
                    let condition = {
                        let left = {
                            let left = {
                                let left = normalized_side.clone();
                                let right = lit(h, "A");
                                h.boolean(h.strict_equal(&left, &right))
                            };
                            if h.truthy(&left) {
                                left
                            } else {
                                {
                                    let left = normalized_side.clone();
                                    let right = lit(h, "S");
                                    h.boolean(h.strict_equal(&left, &right))
                                }
                            }
                        };
                        if h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = normalized_side.clone();
                                let right = lit(h, "SELL");
                                h.boolean(h.strict_equal(&left, &right))
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    return Ok(RestFlow::Return({
                        let object = h.global("OrderSide")?;
                        h.get(&object, "Sell")?
                    }));
                }
                return Ok(RestFlow::Return(h.undefined()));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchHistoricalFills3(_) => {
            let a = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let b = args.get(1).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let left = {
                        let object = b.clone();
                        h.get(&object, "timestamp")?
                    };
                    let right = {
                        let object = a.clone();
                        h.get(&object, "timestamp")?
                    };
                    h.position_operator("subtract", &left, &right)?
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::FetchOHLCV2(captures) => {
            let opts = captures[0].clone();
            let market = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                return Ok(RestFlow::Return({
                    let left = {
                        let object = market.clone();
                        h.get(&object, "symbol")?
                    };
                    let right = {
                        let object = opts.clone();
                        h.get(&object, "symbol")?
                    };
                    h.boolean(h.strict_equal(&left, &right))
                }));
            })();
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        _ => unreachable!("async callback requires owned task"),
    }
}
pub fn start_rest_callback<H: RestTaskHost>(
    host: &H,
    callback: RestCallback<H::Value>,
    args: Vec<H::Value>,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    crate::helper_tasks::start(host, async move {
        run_rest_async_callback(&mut h, &callback, &args).await
    })
}
async fn run_rest_async_callback<H: RestReadHost>(
    h: &mut H,
    callback: &RestCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    match callback {
        RestCallback::FetchIncomeHistory2(captures) => {
            let this_value = captures[0].clone();
            let user = captures[1].clone();
            let request_type = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let body = {
                let value = args.get(1).cloned().unwrap_or_else(|| h.undefined());
                if h.strict_equal(&value, &h.undefined()) {
                    {
                        let object = h.object()?;
                        object
                    }
                } else {
                    value
                }
            };
            let outcome: Result<RestFlow<H::Value>, H::Error> = async {
                let binding = {
                    let value = {
                        let receiver = {
                            let object = this_value.clone();
                            h.get(&object, "infoXhr")?
                        };
                        let function = h.get(&receiver, "post")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let object = h.global("ENDPOINTS")?;
                            h.get(&object, "INFO")?
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.object()?;
                            let value = request_type.clone();
                            h.define_data(&object, "type", &value)?;
                            let value = user.clone();
                            h.define_data(&object, "user", &value)?;
                            let value = body.clone();
                            h.spread_into(&object, &value)?;
                            object
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.object()?;
                            let value = {
                                let object = h.object()?;
                                let value = lit(h, "application/json");
                                h.define_data(&object, "Content-Type", &value)?;
                                object
                            };
                            h.define_data(&object, "headers", &value)?;
                            object
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    h.await_value(value).await?
                };
                let data = {
                    let value = h.get(&binding, "data")?;
                    value
                };
                return Ok(RestFlow::Return({
                    let condition = {
                        let receiver = h.global("Array")?;
                        let function = h.get(&receiver, "isArray")?;
                        let mut arguments = Vec::new();
                        let value = data.clone();
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    if h.truthy(&condition) {
                        data.clone()
                    } else {
                        {
                            let array = h.array()?;
                            array
                        }
                    }
                }));
            }
            .await;
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        RestCallback::DeriveClosedPositions1(captures) => {
            let this_value = captures[0].clone();
            let user = captures[1].clone();
            let request_type = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let body = {
                let value = args.get(1).cloned().unwrap_or_else(|| h.undefined());
                if h.strict_equal(&value, &h.undefined()) {
                    {
                        let object = h.object()?;
                        object
                    }
                } else {
                    value
                }
            };
            let outcome: Result<RestFlow<H::Value>, H::Error> = async {
                let binding = {
                    let value = {
                        let receiver = {
                            let object = this_value.clone();
                            h.get(&object, "infoXhr")?
                        };
                        let function = h.get(&receiver, "post")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let object = h.global("ENDPOINTS")?;
                            h.get(&object, "INFO")?
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.object()?;
                            let value = request_type.clone();
                            h.define_data(&object, "type", &value)?;
                            let value = user.clone();
                            h.define_data(&object, "user", &value)?;
                            let value = body.clone();
                            h.spread_into(&object, &value)?;
                            object
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.object()?;
                            let value = {
                                let object = h.object()?;
                                let value = lit(h, "application/json");
                                h.define_data(&object, "Content-Type", &value)?;
                                object
                            };
                            h.define_data(&object, "headers", &value)?;
                            object
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    h.await_value(value).await?
                };
                let data = {
                    let value = h.get(&binding, "data")?;
                    value
                };
                return Ok(RestFlow::Return({
                    let condition = {
                        let receiver = h.global("Array")?;
                        let function = h.get(&receiver, "isArray")?;
                        let mut arguments = Vec::new();
                        let value = data.clone();
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    if h.truthy(&condition) {
                        data.clone()
                    } else {
                        {
                            let array = h.array()?;
                            array
                        }
                    }
                }));
            }
            .await;
            match outcome? {
                RestFlow::Return(value) => Ok(value),
                RestFlow::Normal => Ok(h.undefined()),
                RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
            }
        }
        _ => unreachable!("sync callback requires immediate body"),
    }
}
/// Source: hyperliquid.exchange.ts::fetchMarketsHelper
pub fn fetch_markets_helper<H: RestReadHost>(
    h: &mut H,
    this_value: &H::Value,
    meta: &H::Value,
    assets: &H::Value,
    fees: &H::Value,
) -> Result<H::Value, H::Error> {
    let meta = meta.clone();
    let assets = assets.clone();
    let fees = fees.clone();
    let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
        let markets = {
            let array = h.array()?;
            array
        };
        let tickers = {
            let array = h.array()?;
            array
        };
        {
            let mut i: usize = 0;
            while {
                let left = h.number(i as f64);
                let right = {
                    let object = {
                        let object = meta.clone();
                        h.get(&object, "universe")?
                    };
                    h.get(&object, "length")?
                };
                let condition = h.position_operator("less", &left, &right)?;
                h.truthy(&condition)
            } {
                let result: Result<RestFlow<H::Value>, H::Error> = (|| {
                    let source_market = {
                        let object = {
                            let object = meta.clone();
                            h.get(&object, "universe")?
                        };
                        {
                            let key = h.number(i as f64);
                            h.index(&object, &key)?
                        }
                    };
                    let source_ticker = {
                        let object = assets.clone();
                        {
                            let key = h.number(i as f64);
                            h.index(&object, &key)?
                        }
                    };
                    let base_asset = {
                        let function = h.global("v")?;
                        let receiver = h.undefined();
                        let mut arguments = Vec::new();
                        let value = source_market.clone();
                        arguments.push(value);
                        let value = lit(h, "name");
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    let quote_asset = lit(h, "USDC");
                    let amount_precision_str = {
                        let function = h.global("safeString")?;
                        let receiver = h.undefined();
                        let mut arguments = Vec::new();
                        let value = source_market.clone();
                        arguments.push(value);
                        let value = lit(h, "szDecimals");
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    let amount_precision = {
                        let function = h.global("parseInt")?;
                        let receiver = h.undefined();
                        let mut arguments = Vec::new();
                        let value = amount_precision_str.clone();
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    let price = {
                        let function = h.global("parseFloat")?;
                        let receiver = h.undefined();
                        let mut arguments = Vec::new();
                        let value = {
                            let function = h.global("v")?;
                            let receiver = h.undefined();
                            let mut arguments = Vec::new();
                            let value = source_ticker.clone();
                            arguments.push(value);
                            let value = lit(h, "midPx");
                            arguments.push(value);
                            h.call(&function, &receiver, &arguments)?
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    if {
                        let condition = {
                            let left = {
                                let value = source_ticker.clone();
                                h.boolean(!h.truthy(&value))
                            };
                            if h.truthy(&left) {
                                left
                            } else {
                                {
                                    let value = {
                                        let object = source_ticker.clone();
                                        {
                                            let key = lit(h, "midPx");
                                            h.index(&object, &key)?
                                        }
                                    };
                                    h.boolean(!h.truthy(&value))
                                }
                            }
                        };
                        h.truthy(&condition)
                    } {
                        return Ok(RestFlow::Next);
                    }
                    {
                        let outcome: Result<RestFlow<H::Value>, H::Error> = (|| {
                            let price_precision = {
                                let function = h.global("calculatePricePrecision")?;
                                let receiver = h.undefined();
                                let mut arguments = Vec::new();
                                let value = price.clone();
                                arguments.push(value);
                                let value = amount_precision.clone();
                                arguments.push(value);
                                let value = h.number(6.0);
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            let price_precision_str = {
                                let function = h.global("numberToString")?;
                                let receiver = h.undefined();
                                let mut arguments = Vec::new();
                                let value = price_precision.clone();
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            let id = base_asset.clone();
                            let symbol = {
                                let mut units: Vec<u16> = vec![];
                                let value = {
                                    let receiver = {
                                        let object = source_market.clone();
                                        h.get(&object, "name")?
                                    };
                                    let function = h.get(&receiver, "toUpperCase")?;
                                    let arguments = Vec::new();
                                    h.call(&function, &receiver, &arguments)?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[45, 80, 69, 82, 80]);
                                h.literal(&JsString(units))
                            };
                            let _ = {
                                let receiver = markets.clone();
                                let function = h.get(&receiver, "push")?;
                                let mut arguments = Vec::new();
                                let value = {
                                    let object = h.object()?;
                                    let value = id.clone();
                                    h.define_data(&object, "id", &value)?;
                                    let value = symbol.clone();
                                    h.define_data(&object, "symbol", &value)?;
                                    let value = base_asset.clone();
                                    h.define_data(&object, "base", &value)?;
                                    let value = quote_asset.clone();
                                    h.define_data(&object, "quote", &value)?;
                                    let value = h.boolean(true);
                                    h.define_data(&object, "active", &value)?;
                                    let value = {
                                        let object = h.object()?;
                                        let value = {
                                            let value = {
                                                let left = {
                                                    let function = h.global("parsePrecision")?;
                                                    let receiver = h.undefined();
                                                    let mut arguments = Vec::new();
                                                    let value = amount_precision_str.clone();
                                                    arguments.push(value);
                                                    h.call(&function, &receiver, &arguments)?
                                                };
                                                if h.truthy(&left) { left } else { h.number(0.0) }
                                            };
                                            h.position_unary("plus", &value)?
                                        };
                                        h.define_data(&object, "amount", &value)?;
                                        let value = {
                                            let value = {
                                                let left = {
                                                    let function = h.global("parsePrecision")?;
                                                    let receiver = h.undefined();
                                                    let mut arguments = Vec::new();
                                                    let value = price_precision_str.clone();
                                                    arguments.push(value);
                                                    h.call(&function, &receiver, &arguments)?
                                                };
                                                if h.truthy(&left) { left } else { h.number(0.0) }
                                            };
                                            h.position_unary("plus", &value)?
                                        };
                                        h.define_data(&object, "price", &value)?;
                                        object
                                    };
                                    h.define_data(&object, "precision", &value)?;
                                    let value = {
                                        let object = h.object()?;
                                        let value = {
                                            let object = h.object()?;
                                            let value = {
                                                let value = {
                                                    let left = {
                                                        let function =
                                                            h.global("parsePrecision")?;
                                                        let receiver = h.undefined();
                                                        let mut arguments = Vec::new();
                                                        let value = amount_precision_str.clone();
                                                        arguments.push(value);
                                                        h.call(&function, &receiver, &arguments)?
                                                    };
                                                    if h.truthy(&left) {
                                                        left
                                                    } else {
                                                        h.number(0.0)
                                                    }
                                                };
                                                h.position_unary("plus", &value)?
                                            };
                                            h.define_data(&object, "min", &value)?;
                                            let value = {
                                                let object = h.global("Number")?;
                                                h.get(&object, "MAX_SAFE_INTEGER")?
                                            };
                                            h.define_data(&object, "max", &value)?;
                                            object
                                        };
                                        h.define_data(&object, "amount", &value)?;
                                        let value = {
                                            let object = h.object()?;
                                            let value = h.number(1.0);
                                            h.define_data(&object, "min", &value)?;
                                            let value = {
                                                let left = {
                                                    let function = h.global("v")?;
                                                    let receiver = h.undefined();
                                                    let mut arguments = Vec::new();
                                                    let value = source_market.clone();
                                                    arguments.push(value);
                                                    let value = lit(h, "maxLeverage");
                                                    arguments.push(value);
                                                    h.call(&function, &receiver, &arguments)?
                                                };
                                                if h.truthy(&left) { left } else { h.number(1.0) }
                                            };
                                            h.define_data(&object, "max", &value)?;
                                            object
                                        };
                                        h.define_data(&object, "leverage", &value)?;
                                        object
                                    };
                                    h.define_data(&object, "limits", &value)?;
                                    let value = {
                                        'optional: {
                                            let mut object = fees.clone();
                                            if h.is_nullish(&object) {
                                                break 'optional h.undefined();
                                            }
                                            object = h.get(&object, "takerFee")?;
                                            object
                                        }
                                    };
                                    h.define_data(&object, "takerFee", &value)?;
                                    let value = {
                                        'optional: {
                                            let mut object = fees.clone();
                                            if h.is_nullish(&object) {
                                                break 'optional h.undefined();
                                            }
                                            object = h.get(&object, "makerFee")?;
                                            object
                                        }
                                    };
                                    h.define_data(&object, "makerFee", &value)?;
                                    let value = {
                                        let object = h.object()?;
                                        let value = h.number(i as f64);
                                        h.define_data(&object, "hyperliquidId", &value)?;
                                        let value = source_market.clone();
                                        h.spread_into(&object, &value)?;
                                        object
                                    };
                                    h.define_data(&object, "info", &value)?;
                                    object
                                };
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            let mark = {
                                let function = h.global("parseFiniteNumber")?;
                                let receiver = h.undefined();
                                let mut arguments = Vec::new();
                                let value = {
                                    let function = h.global("v")?;
                                    let receiver = h.undefined();
                                    let mut arguments = Vec::new();
                                    let value = source_ticker.clone();
                                    arguments.push(value);
                                    let value = lit(h, "markPx");
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            let prev_day_px = {
                                let function = h.global("parseFiniteNumber")?;
                                let receiver = h.undefined();
                                let mut arguments = Vec::new();
                                let value = {
                                    let function = h.global("v")?;
                                    let receiver = h.undefined();
                                    let mut arguments = Vec::new();
                                    let value = source_ticker.clone();
                                    arguments.push(value);
                                    let value = lit(h, "prevDayPx");
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            let notional_price = {
                                let condition = {
                                    let left = mark.clone();
                                    let right = h.number(0.0);
                                    h.position_operator("greater", &left, &right)?
                                };
                                if h.truthy(&condition) {
                                    mark.clone()
                                } else {
                                    price.clone()
                                }
                            };
                            let binding = {
                                let function = h.global("parseHyperliquidVolumeFields")?;
                                let receiver = h.undefined();
                                let mut arguments = Vec::new();
                                let value = source_ticker.clone();
                                arguments.push(value);
                                let value = price.clone();
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            let volume = {
                                let value = h.get(&binding, "volume")?;
                                value
                            };
                            let quote_volume = {
                                let value = h.get(&binding, "quoteVolume")?;
                                value
                            };
                            let _ = {
                                let receiver = tickers.clone();
                                let function = h.get(&receiver, "push")?;
                                let mut arguments = Vec::new();
                                let value = {
                                    let object = h.object()?;
                                    let value = id.clone();
                                    h.define_data(&object, "id", &value)?;
                                    let value = symbol.clone();
                                    h.define_data(&object, "symbol", &value)?;
                                    let value = h.number(0.0);
                                    h.define_data(&object, "bid", &value)?;
                                    let value = h.number(0.0);
                                    h.define_data(&object, "ask", &value)?;
                                    let value = price.clone();
                                    h.define_data(&object, "last", &value)?;
                                    let value = mark.clone();
                                    h.define_data(&object, "mark", &value)?;
                                    let value = {
                                        let function = h.global("parseFiniteNumber")?;
                                        let receiver = h.undefined();
                                        let mut arguments = Vec::new();
                                        let value = {
                                            let function = h.global("v")?;
                                            let receiver = h.undefined();
                                            let mut arguments = Vec::new();
                                            let value = source_ticker.clone();
                                            arguments.push(value);
                                            let value = lit(h, "oraclePx");
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                    h.define_data(&object, "index", &value)?;
                                    let value = price.clone();
                                    h.define_data(&object, "midPrice", &value)?;
                                    let value = {
                                        let function =
                                            h.global("calculatePriceChangePercentage")?;
                                        let receiver = h.undefined();
                                        let mut arguments = Vec::new();
                                        let value = notional_price.clone();
                                        arguments.push(value);
                                        let value = prev_day_px.clone();
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                    h.define_data(&object, "percentage", &value)?;
                                    let value = {
                                        let function = h.global("parseFiniteNumber")?;
                                        let receiver = h.undefined();
                                        let mut arguments = Vec::new();
                                        let value = {
                                            let function = h.global("v")?;
                                            let receiver = h.undefined();
                                            let mut arguments = Vec::new();
                                            let value = source_ticker.clone();
                                            arguments.push(value);
                                            let value = lit(h, "funding");
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                    h.define_data(&object, "fundingRate", &value)?;
                                    let value = h.number(0.0);
                                    h.define_data(&object, "fundingTime", &value)?;
                                    let value = volume.clone();
                                    h.define_data(&object, "volume", &value)?;
                                    let value = quote_volume.clone();
                                    h.define_data(&object, "quoteVolume", &value)?;
                                    let value = {
                                        let function =
                                            h.global("parseHyperliquidOpenInterestNotional")?;
                                        let receiver = h.undefined();
                                        let mut arguments = Vec::new();
                                        let value = source_ticker.clone();
                                        arguments.push(value);
                                        let value = notional_price.clone();
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                    h.define_data(&object, "openInterest", &value)?;
                                    let value = source_ticker.clone();
                                    h.define_data(&object, "info", &value)?;
                                    object
                                };
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            Ok(RestFlow::Normal)
                        })(
                        );
                        match outcome {
                            Ok(RestFlow::Normal) => {}
                            Ok(flow) => return Ok(flow),
                            Err(error) => {
                                let e = h.error_value(&error);
                                let _ = {
                                    let receiver = this_value.clone();
                                    let function = h.get(&receiver, "log")?;
                                    let mut arguments = Vec::new();
                                    let value = {
                                        let mut units: Vec<u16> = vec![
                                            69, 114, 114, 111, 114, 32, 105, 110, 32, 102, 101,
                                            116, 99, 104, 77, 97, 114, 107, 101, 116, 115, 72, 101,
                                            108, 112, 101, 114, 58, 32,
                                        ];
                                        let value = e.clone();
                                        units.extend(h.template_string(&value)?.0);
                                        units.extend_from_slice(&[]);
                                        h.literal(&JsString(units))
                                    };
                                    arguments.push(value);
                                    let value = {
                                        let object = h.global("LogSeverity")?;
                                        h.get(&object, "Error")?
                                    };
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                            }
                        }
                    }
                    Ok(RestFlow::Normal)
                })();
                match result? {
                    RestFlow::Return(value) => return Ok(RestFlow::Return(value)),
                    RestFlow::Break => break,
                    RestFlow::Normal | RestFlow::Next => {}
                }
                i += 1;
            }
        }
        return Ok(RestFlow::Return({
            let object = h.object()?;
            let value = markets.clone();
            h.define_data(&object, "markets", &value)?;
            let value = tickers.clone();
            h.define_data(&object, "tickers", &value)?;
            object
        }));
    })();
    match outcome? {
        RestFlow::Return(value) => Ok(value),
        RestFlow::Normal => Ok(h.undefined()),
        RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
    }
}
/// Source: hyperliquid.exchange.ts::fetchHip3Dexs
async fn fetch_hip3_dexs_body<H: RestReadHost>(
    h: &mut H,
    this_value: &H::Value,
) -> Result<H::Value, H::Error> {
    let outcome: Result<RestFlow<H::Value>, H::Error> = async {
        {
            let outcome: Result<RestFlow<H::Value>, H::Error> = async {
                let binding = {
                    let value = {
                        let receiver = {
                            let object = this_value.clone();
                            h.get(&object, "xhr")?
                        };
                        let function = h.get(&receiver, "post")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let object = h.global("ENDPOINTS")?;
                            h.get(&object, "INFO")?
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.object()?;
                            let value = {
                                let object = h.global("INFO_TYPES")?;
                                h.get(&object, "PERP_DEXS")?
                            };
                            h.define_data(&object, "type", &value)?;
                            object
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.object()?;
                            let value = {
                                let object = h.object()?;
                                let value = lit(h, "application/json");
                                h.define_data(&object, "Content-Type", &value)?;
                                object
                            };
                            h.define_data(&object, "headers", &value)?;
                            object
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    h.await_value(value).await?
                };
                let data = {
                    let value = h.get(&binding, "data")?;
                    value
                };
                let mut dexs_with_position = {
                    let receiver = {
                        let receiver = data.clone();
                        let function = h.get(&receiver, "map")?;
                        let mut arguments = Vec::new();
                        let value = h.rest_callback(RestCallback::FetchHip3Dexs0([]))?;
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    let function = h.get(&receiver, "filter")?;
                    let mut arguments = Vec::new();
                    let value = h.rest_callback(RestCallback::FetchHip3Dexs1([]))?;
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                };
                if {
                    let condition = {
                        let object = {
                            let object = this_value.clone();
                            h.get(&object, "options")?
                        };
                        h.get(&object, "testnet")?
                    };
                    h.truthy(&condition)
                } {
                    dexs_with_position = {
                        let receiver = dexs_with_position.clone();
                        let function = h.get(&receiver, "filter")?;
                        let mut arguments = Vec::new();
                        let value = h.rest_callback(RestCallback::FetchHip3Dexs2([]))?;
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                }
                return Ok(RestFlow::Return(dexs_with_position.clone()));
            }
            .await;
            match outcome {
                Ok(RestFlow::Normal) => {}
                Ok(flow) => return Ok(flow),
                Err(error) => {
                    let error = h.error_value(&error);
                    let _ = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "log")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let mut units: Vec<u16> = vec![
                                70, 97, 105, 108, 101, 100, 32, 116, 111, 32, 102, 101, 116, 99,
                                104, 32, 72, 73, 80, 45, 51, 32, 68, 69, 88, 101, 115, 58, 32,
                            ];
                            let value = error.clone();
                            units.extend(h.template_string(&value)?.0);
                            units.extend_from_slice(&[]);
                            h.literal(&JsString(units))
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.global("LogSeverity")?;
                            h.get(&object, "Warning")?
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    return Ok(RestFlow::Return({
                        let array = h.array()?;
                        array
                    }));
                }
            }
        }
        Ok(RestFlow::Normal)
    }
    .await;
    match outcome? {
        RestFlow::Return(value) => Ok(value),
        RestFlow::Normal => Ok(h.undefined()),
        RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
    }
}
pub fn fetch_hip3_dexs<H: RestTaskHost>(
    host: &H,
    this_value: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let this_value = this_value.clone();
    crate::helper_tasks::start(host, async move {
        fetch_hip3_dexs_body(&mut h, &this_value).await
    })
}
/// Source: hyperliquid.exchange.ts::fetchHip3Markets
async fn fetch_hip3_markets_body<H: RestReadHost>(
    h: &mut H,
    this_value: &H::Value,
    dex_name: &H::Value,
    dex_position: &H::Value,
    fees: &H::Value,
) -> Result<H::Value, H::Error> {
    let dex_name = dex_name.clone();
    let dex_position = dex_position.clone();
    let fees = fees.clone();
    let outcome: Result<RestFlow<H::Value>, H::Error> = async {
        let parsed_response = {
            let value = {
                let function = h.global("fetchHip3MetaAndAssetCtxsWithRetry")?;
                let receiver = h.undefined();
                let mut arguments = Vec::new();
                let value = {
                    let object = h.object()?;
                    let value = {
                        let object = this_value.clone();
                        h.get(&object, "xhr")?
                    };
                    h.define_data(&object, "xhr", &value)?;
                    let value = dex_name.clone();
                    h.define_data(&object, "dexName", &value)?;
                    let value =
                        h.rest_callback(RestCallback::FetchHip3Markets0([this_value.clone()]))?;
                    h.define_data(&object, "log", &value)?;
                    object
                };
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
            h.await_value(value).await?
        };
        if {
            let condition = {
                let value = parsed_response.clone();
                h.boolean(!h.truthy(&value))
            };
            h.truthy(&condition)
        } {
            return Ok(RestFlow::Return(h.null_value()));
        }
        {
            let outcome: Result<RestFlow<H::Value>, H::Error> = async {
                let binding = parsed_response.clone();
                let elements = h.rest_array_binding(&binding, &[true, true])?;
                let meta = elements[0].clone();
                let assets = elements[1].clone();
                let markets = {
                    let array = h.array()?;
                    array
                };
                let tickers = {
                    let array = h.array()?;
                    array
                };
                let asset_index_offset = {
                    let left = h.number(110000.0);
                    let right = {
                        let left = {
                            let left = dex_position.clone();
                            let right = h.number(1.0);
                            h.position_operator("subtract", &left, &right)?
                        };
                        let right = h.number(10000.0);
                        h.position_operator("multiply", &left, &right)?
                    };
                    h.position_operator("add", &left, &right)?
                };
                {
                    let mut i: usize = 0;
                    while {
                        let left = h.number(i as f64);
                        let right = {
                            let object = {
                                let object = meta.clone();
                                h.get(&object, "universe")?
                            };
                            h.get(&object, "length")?
                        };
                        let condition = h.position_operator("less", &left, &right)?;
                        h.truthy(&condition)
                    } {
                        let result: Result<RestFlow<H::Value>, H::Error> = async {
                            let source_market = {
                                let object = {
                                    let object = meta.clone();
                                    h.get(&object, "universe")?
                                };
                                {
                                    let key = h.number(i as f64);
                                    h.index(&object, &key)?
                                }
                            };
                            let source_ticker = {
                                let object = assets.clone();
                                {
                                    let key = h.number(i as f64);
                                    h.index(&object, &key)?
                                }
                            };
                            if {
                                let condition = {
                                    let left = {
                                        let left = {
                                            let object = source_market.clone();
                                            h.get(&object, "isDelisted")?
                                        };
                                        if h.truthy(&left) {
                                            left
                                        } else {
                                            {
                                                let value = source_ticker.clone();
                                                h.boolean(!h.truthy(&value))
                                            }
                                        }
                                    };
                                    if h.truthy(&left) {
                                        left
                                    } else {
                                        {
                                            let value = {
                                                let object = source_ticker.clone();
                                                {
                                                    let key = lit(h, "midPx");
                                                    h.index(&object, &key)?
                                                }
                                            };
                                            h.boolean(!h.truthy(&value))
                                        }
                                    }
                                };
                                h.truthy(&condition)
                            } {
                                return Ok(RestFlow::Next);
                            }
                            let raw_name = {
                                let function = h.global("v")?;
                                let receiver = h.undefined();
                                let mut arguments = Vec::new();
                                let value = source_market.clone();
                                arguments.push(value);
                                let value = lit(h, "name");
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            let base_asset = {
                                let condition = {
                                    let receiver = raw_name.clone();
                                    let function = h.get(&receiver, "includes")?;
                                    let mut arguments = Vec::new();
                                    let value = lit(h, ":");
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                                if h.truthy(&condition) {
                                    {
                                        let object = {
                                            let receiver = raw_name.clone();
                                            let function = h.get(&receiver, "split")?;
                                            let mut arguments = Vec::new();
                                            let value = lit(h, ":");
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        {
                                            let key = h.number(1.0);
                                            h.index(&object, &key)?
                                        }
                                    }
                                } else {
                                    raw_name.clone()
                                }
                            };
                            let amount_precision_str = {
                                let function = h.global("safeString")?;
                                let receiver = h.undefined();
                                let mut arguments = Vec::new();
                                let value = source_market.clone();
                                arguments.push(value);
                                let value = lit(h, "szDecimals");
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            let amount_precision = {
                                let function = h.global("parseInt")?;
                                let receiver = h.undefined();
                                let mut arguments = Vec::new();
                                let value = amount_precision_str.clone();
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            let price = {
                                let function = h.global("parseFloat")?;
                                let receiver = h.undefined();
                                let mut arguments = Vec::new();
                                let value = {
                                    let function = h.global("v")?;
                                    let receiver = h.undefined();
                                    let mut arguments = Vec::new();
                                    let value = source_ticker.clone();
                                    arguments.push(value);
                                    let value = lit(h, "midPx");
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            {
                                let outcome: Result<RestFlow<H::Value>, H::Error> = async {
                                    let price_precision = {
                                        let function = h.global("calculatePricePrecision")?;
                                        let receiver = h.undefined();
                                        let mut arguments = Vec::new();
                                        let value = price.clone();
                                        arguments.push(value);
                                        let value = amount_precision.clone();
                                        arguments.push(value);
                                        let value = h.number(6.0);
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                    let price_precision_str = {
                                        let function = h.global("numberToString")?;
                                        let receiver = h.undefined();
                                        let mut arguments = Vec::new();
                                        let value = price_precision.clone();
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                    let id = {
                                        let condition = {
                                            let receiver = raw_name.clone();
                                            let function = h.get(&receiver, "includes")?;
                                            let mut arguments = Vec::new();
                                            let value = lit(h, ":");
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        if h.truthy(&condition) {
                                            raw_name.clone()
                                        } else {
                                            {
                                                let mut units: Vec<u16> = vec![];
                                                let value = dex_name.clone();
                                                units.extend(h.template_string(&value)?.0);
                                                units.extend_from_slice(&[58]);
                                                let value = raw_name.clone();
                                                units.extend(h.template_string(&value)?.0);
                                                units.extend_from_slice(&[]);
                                                h.literal(&JsString(units))
                                            }
                                        }
                                    };
                                    let symbol = {
                                        let mut units: Vec<u16> = vec![];
                                        let value = {
                                            let receiver = dex_name.clone();
                                            let function = h.get(&receiver, "toUpperCase")?;
                                            let arguments = Vec::new();
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        units.extend(h.template_string(&value)?.0);
                                        units.extend_from_slice(&[124]);
                                        let value = {
                                            let receiver = base_asset.clone();
                                            let function = h.get(&receiver, "toUpperCase")?;
                                            let arguments = Vec::new();
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        units.extend(h.template_string(&value)?.0);
                                        units.extend_from_slice(&[45, 80, 69, 82, 80]);
                                        h.literal(&JsString(units))
                                    };
                                    let _ = {
                                        let receiver = markets.clone();
                                        let function = h.get(&receiver, "push")?;
                                        let mut arguments = Vec::new();
                                        let value = {
                                            let object = h.object()?;
                                            let value = id.clone();
                                            h.define_data(&object, "id", &value)?;
                                            let value = symbol.clone();
                                            h.define_data(&object, "symbol", &value)?;
                                            let value = base_asset.clone();
                                            h.define_data(&object, "base", &value)?;
                                            let value = lit(h, "USDC");
                                            h.define_data(&object, "quote", &value)?;
                                            let value = h.boolean(true);
                                            h.define_data(&object, "active", &value)?;
                                            let value = {
                                                let object = h.object()?;
                                                let value = {
                                                    let value = {
                                                        let left = {
                                                            let function =
                                                                h.global("parsePrecision")?;
                                                            let receiver = h.undefined();
                                                            let mut arguments = Vec::new();
                                                            let value =
                                                                amount_precision_str.clone();
                                                            arguments.push(value);
                                                            h.call(
                                                                &function, &receiver, &arguments,
                                                            )?
                                                        };
                                                        if h.truthy(&left) {
                                                            left
                                                        } else {
                                                            h.number(0.0)
                                                        }
                                                    };
                                                    h.position_unary("plus", &value)?
                                                };
                                                h.define_data(&object, "amount", &value)?;
                                                let value = {
                                                    let value = {
                                                        let left = {
                                                            let function =
                                                                h.global("parsePrecision")?;
                                                            let receiver = h.undefined();
                                                            let mut arguments = Vec::new();
                                                            let value = price_precision_str.clone();
                                                            arguments.push(value);
                                                            h.call(
                                                                &function, &receiver, &arguments,
                                                            )?
                                                        };
                                                        if h.truthy(&left) {
                                                            left
                                                        } else {
                                                            h.number(0.0)
                                                        }
                                                    };
                                                    h.position_unary("plus", &value)?
                                                };
                                                h.define_data(&object, "price", &value)?;
                                                object
                                            };
                                            h.define_data(&object, "precision", &value)?;
                                            let value = {
                                                let object = h.object()?;
                                                let value = {
                                                    let object = h.object()?;
                                                    let value = {
                                                        let value = {
                                                            let left = {
                                                                let function =
                                                                    h.global("parsePrecision")?;
                                                                let receiver = h.undefined();
                                                                let mut arguments = Vec::new();
                                                                let value =
                                                                    amount_precision_str.clone();
                                                                arguments.push(value);
                                                                h.call(
                                                                    &function, &receiver,
                                                                    &arguments,
                                                                )?
                                                            };
                                                            if h.truthy(&left) {
                                                                left
                                                            } else {
                                                                h.number(0.0)
                                                            }
                                                        };
                                                        h.position_unary("plus", &value)?
                                                    };
                                                    h.define_data(&object, "min", &value)?;
                                                    let value = {
                                                        let object = h.global("Number")?;
                                                        h.get(&object, "MAX_SAFE_INTEGER")?
                                                    };
                                                    h.define_data(&object, "max", &value)?;
                                                    object
                                                };
                                                h.define_data(&object, "amount", &value)?;
                                                let value = {
                                                    let object = h.object()?;
                                                    let value = h.number(1.0);
                                                    h.define_data(&object, "min", &value)?;
                                                    let value = {
                                                        let left = {
                                                            let function = h.global("v")?;
                                                            let receiver = h.undefined();
                                                            let mut arguments = Vec::new();
                                                            let value = source_market.clone();
                                                            arguments.push(value);
                                                            let value = lit(h, "maxLeverage");
                                                            arguments.push(value);
                                                            h.call(
                                                                &function, &receiver, &arguments,
                                                            )?
                                                        };
                                                        if h.truthy(&left) {
                                                            left
                                                        } else {
                                                            h.number(1.0)
                                                        }
                                                    };
                                                    h.define_data(&object, "max", &value)?;
                                                    object
                                                };
                                                h.define_data(&object, "leverage", &value)?;
                                                object
                                            };
                                            h.define_data(&object, "limits", &value)?;
                                            let value = {
                                                'optional: {
                                                    let mut object = fees.clone();
                                                    if h.is_nullish(&object) {
                                                        break 'optional h.undefined();
                                                    }
                                                    object = h.get(&object, "takerFee")?;
                                                    object
                                                }
                                            };
                                            h.define_data(&object, "takerFee", &value)?;
                                            let value = {
                                                'optional: {
                                                    let mut object = fees.clone();
                                                    if h.is_nullish(&object) {
                                                        break 'optional h.undefined();
                                                    }
                                                    object = h.get(&object, "makerFee")?;
                                                    object
                                                }
                                            };
                                            h.define_data(&object, "makerFee", &value)?;
                                            let value = {
                                                let object = h.object()?;
                                                let value = {
                                                    let left = h.number(i as f64);
                                                    let right = asset_index_offset.clone();
                                                    h.position_operator("add", &left, &right)?
                                                };
                                                h.define_data(&object, "hyperliquidId", &value)?;
                                                let value = dex_name.clone();
                                                h.define_data(&object, "hip3Dex", &value)?;
                                                let value = source_market.clone();
                                                h.spread_into(&object, &value)?;
                                                object
                                            };
                                            h.define_data(&object, "info", &value)?;
                                            object
                                        };
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                    let mark = {
                                        let function = h.global("parseFiniteNumber")?;
                                        let receiver = h.undefined();
                                        let mut arguments = Vec::new();
                                        let value = {
                                            let function = h.global("v")?;
                                            let receiver = h.undefined();
                                            let mut arguments = Vec::new();
                                            let value = source_ticker.clone();
                                            arguments.push(value);
                                            let value = lit(h, "markPx");
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                    let prev_day_px = {
                                        let function = h.global("parseFiniteNumber")?;
                                        let receiver = h.undefined();
                                        let mut arguments = Vec::new();
                                        let value = {
                                            let function = h.global("v")?;
                                            let receiver = h.undefined();
                                            let mut arguments = Vec::new();
                                            let value = source_ticker.clone();
                                            arguments.push(value);
                                            let value = lit(h, "prevDayPx");
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                    let notional_price = {
                                        let condition = {
                                            let left = mark.clone();
                                            let right = h.number(0.0);
                                            h.position_operator("greater", &left, &right)?
                                        };
                                        if h.truthy(&condition) {
                                            mark.clone()
                                        } else {
                                            price.clone()
                                        }
                                    };
                                    let binding = {
                                        let function = h.global("parseHyperliquidVolumeFields")?;
                                        let receiver = h.undefined();
                                        let mut arguments = Vec::new();
                                        let value = source_ticker.clone();
                                        arguments.push(value);
                                        let value = price.clone();
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                    let volume = {
                                        let value = h.get(&binding, "volume")?;
                                        value
                                    };
                                    let quote_volume = {
                                        let value = h.get(&binding, "quoteVolume")?;
                                        value
                                    };
                                    let _ = {
                                        let receiver = tickers.clone();
                                        let function = h.get(&receiver, "push")?;
                                        let mut arguments = Vec::new();
                                        let value = {
                                            let object = h.object()?;
                                            let value = id.clone();
                                            h.define_data(&object, "id", &value)?;
                                            let value = symbol.clone();
                                            h.define_data(&object, "symbol", &value)?;
                                            let value = h.number(0.0);
                                            h.define_data(&object, "bid", &value)?;
                                            let value = h.number(0.0);
                                            h.define_data(&object, "ask", &value)?;
                                            let value = price.clone();
                                            h.define_data(&object, "last", &value)?;
                                            let value = mark.clone();
                                            h.define_data(&object, "mark", &value)?;
                                            let value = {
                                                let function = h.global("parseFiniteNumber")?;
                                                let receiver = h.undefined();
                                                let mut arguments = Vec::new();
                                                let value = {
                                                    let function = h.global("v")?;
                                                    let receiver = h.undefined();
                                                    let mut arguments = Vec::new();
                                                    let value = source_ticker.clone();
                                                    arguments.push(value);
                                                    let value = lit(h, "oraclePx");
                                                    arguments.push(value);
                                                    h.call(&function, &receiver, &arguments)?
                                                };
                                                arguments.push(value);
                                                h.call(&function, &receiver, &arguments)?
                                            };
                                            h.define_data(&object, "index", &value)?;
                                            let value = price.clone();
                                            h.define_data(&object, "midPrice", &value)?;
                                            let value = {
                                                let function =
                                                    h.global("calculatePriceChangePercentage")?;
                                                let receiver = h.undefined();
                                                let mut arguments = Vec::new();
                                                let value = notional_price.clone();
                                                arguments.push(value);
                                                let value = prev_day_px.clone();
                                                arguments.push(value);
                                                h.call(&function, &receiver, &arguments)?
                                            };
                                            h.define_data(&object, "percentage", &value)?;
                                            let value = {
                                                let function = h.global("parseFiniteNumber")?;
                                                let receiver = h.undefined();
                                                let mut arguments = Vec::new();
                                                let value = {
                                                    let function = h.global("v")?;
                                                    let receiver = h.undefined();
                                                    let mut arguments = Vec::new();
                                                    let value = source_ticker.clone();
                                                    arguments.push(value);
                                                    let value = lit(h, "funding");
                                                    arguments.push(value);
                                                    h.call(&function, &receiver, &arguments)?
                                                };
                                                arguments.push(value);
                                                h.call(&function, &receiver, &arguments)?
                                            };
                                            h.define_data(&object, "fundingRate", &value)?;
                                            let value = h.number(0.0);
                                            h.define_data(&object, "fundingTime", &value)?;
                                            let value = volume.clone();
                                            h.define_data(&object, "volume", &value)?;
                                            let value = quote_volume.clone();
                                            h.define_data(&object, "quoteVolume", &value)?;
                                            let value = {
                                                let function = h.global(
                                                    "parseHyperliquidOpenInterestNotional",
                                                )?;
                                                let receiver = h.undefined();
                                                let mut arguments = Vec::new();
                                                let value = source_ticker.clone();
                                                arguments.push(value);
                                                let value = notional_price.clone();
                                                arguments.push(value);
                                                h.call(&function, &receiver, &arguments)?
                                            };
                                            h.define_data(&object, "openInterest", &value)?;
                                            let value = source_ticker.clone();
                                            h.define_data(&object, "info", &value)?;
                                            object
                                        };
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                    Ok(RestFlow::Normal)
                                }
                                .await;
                                match outcome {
                                    Ok(RestFlow::Normal) => {}
                                    Ok(flow) => return Ok(flow),
                                    Err(error) => {
                                        let e = h.error_value(&error);
                                        let _ = {
                                            let receiver = this_value.clone();
                                            let function = h.get(&receiver, "log")?;
                                            let mut arguments = Vec::new();
                                            let value = {
                                                let mut units: Vec<u16> = vec![
                                                    69, 114, 114, 111, 114, 32, 112, 114, 111, 99,
                                                    101, 115, 115, 105, 110, 103, 32, 72, 73, 80,
                                                    45, 51, 32, 109, 97, 114, 107, 101, 116, 32,
                                                ];
                                                let value = raw_name.clone();
                                                units.extend(h.template_string(&value)?.0);
                                                units.extend_from_slice(&[58, 32]);
                                                let value = e.clone();
                                                units.extend(h.template_string(&value)?.0);
                                                units.extend_from_slice(&[]);
                                                h.literal(&JsString(units))
                                            };
                                            arguments.push(value);
                                            let value = {
                                                let object = h.global("LogSeverity")?;
                                                h.get(&object, "Error")?
                                            };
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                    }
                                }
                            }
                            Ok(RestFlow::Normal)
                        }
                        .await;
                        match result? {
                            RestFlow::Return(value) => return Ok(RestFlow::Return(value)),
                            RestFlow::Break => break,
                            RestFlow::Normal | RestFlow::Next => {}
                        }
                        i += 1;
                    }
                }
                return Ok(RestFlow::Return({
                    let object = h.object()?;
                    let value = markets.clone();
                    h.define_data(&object, "markets", &value)?;
                    let value = tickers.clone();
                    h.define_data(&object, "tickers", &value)?;
                    object
                }));
            }
            .await;
            match outcome {
                Ok(RestFlow::Normal) => {}
                Ok(flow) => return Ok(flow),
                Err(error) => {
                    let error = h.error_value(&error);
                    let _ = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "log")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let mut units: Vec<u16> = vec![
                                70, 97, 105, 108, 101, 100, 32, 116, 111, 32, 102, 101, 116, 99,
                                104, 32, 72, 73, 80, 45, 51, 32, 109, 97, 114, 107, 101, 116, 115,
                                32, 102, 111, 114, 32,
                            ];
                            let value = dex_name.clone();
                            units.extend(h.template_string(&value)?.0);
                            units.extend_from_slice(&[58, 32]);
                            let value = error.clone();
                            units.extend(h.template_string(&value)?.0);
                            units.extend_from_slice(&[]);
                            h.literal(&JsString(units))
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.global("LogSeverity")?;
                            h.get(&object, "Warning")?
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    return Ok(RestFlow::Return(h.null_value()));
                }
            }
        }
        Ok(RestFlow::Normal)
    }
    .await;
    match outcome? {
        RestFlow::Return(value) => Ok(value),
        RestFlow::Normal => Ok(h.undefined()),
        RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
    }
}
pub fn fetch_hip3_markets<H: RestTaskHost>(
    host: &H,
    this_value: &H::Value,
    dex_name: &H::Value,
    dex_position: &H::Value,
    fees: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let this_value = this_value.clone();
    let dex_name = dex_name.clone();
    let dex_position = dex_position.clone();
    let fees = fees.clone();
    crate::helper_tasks::start(host, async move {
        fetch_hip3_markets_body(&mut h, &this_value, &dex_name, &dex_position, &fees).await
    })
}
/// Source: hyperliquid.exchange.ts::fetchOHLCV
async fn fetch_ohlcv_body<H: RestReadHost>(
    h: &mut H,
    this_value: &H::Value,
    opts: &H::Value,
) -> Result<H::Value, H::Error> {
    let opts = opts.clone();
    let outcome: Result<RestFlow<H::Value>, H::Error> = async {
        let interval = {
            let object = opts.clone();
            h.get(&object, "interval")?
        };
        let limit = {
            let receiver = h.global("Math")?;
            let function = h.get(&receiver, "min")?;
            let mut arguments = Vec::new();
            let value = {
                let left = {
                    let object = opts.clone();
                    h.get(&object, "limit")?
                };
                if h.truthy(&left) {
                    left
                } else {
                    h.number(500.0)
                }
            };
            arguments.push(value);
            let value = h.number(1500.0);
            arguments.push(value);
            h.call(&function, &receiver, &arguments)?
        };
        let binding = {
            let receiver = {
                let object = opts.clone();
                h.get(&object, "interval")?
            };
            let function = h.get(&receiver, "split")?;
            let mut arguments = Vec::new();
            let value = h.regex("/(\\d+)/")?;
            arguments.push(value);
            h.call(&function, &receiver, &arguments)?
        };
        let elements = h.rest_array_binding(&binding, &[false, true, true])?;
        // The host returns the selected bindings only; the leading hole consumes
        // an iterator item without occupying a returned binding slot.
        let amount = elements[0].clone();
        let unit = elements[1].clone();
        let end = {
            let condition = {
                let object = opts.clone();
                h.get(&object, "to")?
            };
            if h.truthy(&condition) {
                {
                    let function = h.global("dayjs")?;
                    let receiver = h.undefined();
                    let mut arguments = Vec::new();
                    let value = {
                        let object = opts.clone();
                        h.get(&object, "to")?
                    };
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }
            } else {
                {
                    let function = h.global("dayjs")?;
                    let receiver = h.undefined();
                    let arguments = Vec::new();
                    h.call(&function, &receiver, &arguments)?
                }
            }
        };
        let start = {
            let condition = {
                let left = {
                    let value = {
                        let object = opts.clone();
                        h.get(&object, "limit")?
                    };
                    h.boolean(!h.truthy(&value))
                };
                if !h.truthy(&left) {
                    left
                } else {
                    {
                        let object = opts.clone();
                        h.get(&object, "from")?
                    }
                }
            };
            if h.truthy(&condition) {
                {
                    let function = h.global("dayjs")?;
                    let receiver = h.undefined();
                    let mut arguments = Vec::new();
                    let value = {
                        let object = opts.clone();
                        h.get(&object, "from")?
                    };
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }
            } else {
                {
                    let receiver = end.clone();
                    let function = h.get(&receiver, "subtract")?;
                    let mut arguments = Vec::new();
                    let value = {
                        let left = {
                            let function = h.global("parseFloat")?;
                            let receiver = h.undefined();
                            let mut arguments = Vec::new();
                            let value = amount.clone();
                            arguments.push(value);
                            h.call(&function, &receiver, &arguments)?
                        };
                        let right = limit.clone();
                        h.position_operator("multiply", &left, &right)?
                    };
                    arguments.push(value);
                    let value = unit.clone();
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }
            }
        };
        let binding = {
            let value = {
                let receiver = {
                    let object = this_value.clone();
                    h.get(&object, "xhr")?
                };
                let function = h.get(&receiver, "post")?;
                let mut arguments = Vec::new();
                let value = {
                    let object = h.global("ENDPOINTS")?;
                    h.get(&object, "INFO")?
                };
                arguments.push(value);
                let value = {
                    let object = h.object()?;
                    let value = lit(h, "candleSnapshot");
                    h.define_data(&object, "type", &value)?;
                    let value = {
                        let object = h.object()?;
                        let value = {
                            let function = h.rest_callback(RestCallback::FetchOHLCV0([
                                this_value.clone(),
                                opts.clone(),
                            ]))?;
                            let receiver = h.undefined();
                            let arguments = Vec::new();
                            h.call(&function, &receiver, &arguments)?
                        };
                        h.define_data(&object, "coin", &value)?;
                        let value = interval.clone();
                        h.define_data(&object, "interval", &value)?;
                        let value = {
                            let receiver = start.clone();
                            let function = h.get(&receiver, "valueOf")?;
                            let arguments = Vec::new();
                            h.call(&function, &receiver, &arguments)?
                        };
                        h.define_data(&object, "startTime", &value)?;
                        let value = {
                            let receiver = end.clone();
                            let function = h.get(&receiver, "valueOf")?;
                            let arguments = Vec::new();
                            h.call(&function, &receiver, &arguments)?
                        };
                        h.define_data(&object, "endTime", &value)?;
                        object
                    };
                    h.define_data(&object, "req", &value)?;
                    object
                };
                arguments.push(value);
                let value = {
                    let object = h.object()?;
                    let value = {
                        let object = h.object()?;
                        let value = lit(h, "application/json");
                        h.define_data(&object, "Content-Type", &value)?;
                        object
                    };
                    h.define_data(&object, "headers", &value)?;
                    object
                };
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
            h.await_value(value).await?
        };
        let data = {
            let value = h.get(&binding, "data")?;
            value
        };
        let candle_data = {
            let condition = {
                let receiver = h.global("Array")?;
                let function = h.get(&receiver, "isArray")?;
                let mut arguments = Vec::new();
                let value = data.clone();
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
            if h.truthy(&condition) {
                data.clone()
            } else {
                {
                    let left = {
                        let left = {
                            'optional: {
                                let mut object = data.clone();
                                if h.is_nullish(&object) {
                                    break 'optional h.undefined();
                                }
                                object = h.get(&object, "data")?;
                                object
                            }
                        };
                        if h.truthy(&left) {
                            left
                        } else {
                            {
                                'optional: {
                                    let mut object = data.clone();
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "response")?;
                                    object
                                }
                            }
                        }
                    };
                    if h.truthy(&left) {
                        left
                    } else {
                        {
                            let array = h.array()?;
                            array
                        }
                    }
                }
            }
        };
        if {
            let condition = {
                let value = {
                    let receiver = h.global("Array")?;
                    let function = h.get(&receiver, "isArray")?;
                    let mut arguments = Vec::new();
                    let value = candle_data.clone();
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                };
                h.boolean(!h.truthy(&value))
            };
            h.truthy(&condition)
        } {
            let _ = {
                let receiver = this_value.clone();
                let function = h.get(&receiver, "log")?;
                let mut arguments = Vec::new();
                let value = {
                    let mut units: Vec<u16> = vec![
                        85, 110, 101, 120, 112, 101, 99, 116, 101, 100, 32, 102, 101, 116, 99, 104,
                        79, 72, 76, 67, 86, 32, 114, 101, 115, 112, 111, 110, 115, 101, 32, 102,
                        111, 114, 109, 97, 116, 58, 32,
                    ];
                    let value = {
                        let receiver = h.global("JSON")?;
                        let function = h.get(&receiver, "stringify")?;
                        let mut arguments = Vec::new();
                        let value = data.clone();
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[]);
                    h.literal(&JsString(units))
                };
                arguments.push(value);
                let value = {
                    let object = h.global("LogSeverity")?;
                    h.get(&object, "Error")?
                };
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
            return Ok(RestFlow::Return({
                let array = h.array()?;
                array
            }));
        }
        let candles = {
            let receiver = candle_data.clone();
            let function = h.get(&receiver, "map")?;
            let mut arguments = Vec::new();
            let value = h.rest_callback(RestCallback::FetchOHLCV1([]))?;
            arguments.push(value);
            h.call(&function, &receiver, &arguments)?
        };
        return Ok(RestFlow::Return(candles.clone()));
    }
    .await;
    match outcome? {
        RestFlow::Return(value) => Ok(value),
        RestFlow::Normal => Ok(h.undefined()),
        RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
    }
}
pub fn fetch_ohlcv<H: RestTaskHost>(
    host: &H,
    this_value: &H::Value,
    opts: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let this_value = this_value.clone();
    let opts = opts.clone();
    crate::helper_tasks::start(host, async move {
        fetch_ohlcv_body(&mut h, &this_value, &opts).await
    })
}
/// Source: hyperliquid.exchange.ts::fetchIncomeHistory
async fn fetch_income_history_body<H: RestReadHost>(
    h: &mut H,
    this_value: &H::Value,
    opts: &H::Value,
) -> Result<H::Value, H::Error> {
    let opts = opts.clone();
    let outcome: Result<RestFlow<H::Value>, H::Error> = async {
        if {
            let condition = {
                let object = this_value.clone();
                h.get(&object, "isPublicDataOnly")?
            };
            h.truthy(&condition)
        } {
            return Ok(RestFlow::Return({
                let array = h.array()?;
                array
            }));
        }
        let user = {
            let left = {
                let object = {
                    let object = this_value.clone();
                    h.get(&object, "options")?
                };
                h.get(&object, "vault")?
            };
            if h.truthy(&left) {
                left
            } else {
                {
                    let object = {
                        let object = this_value.clone();
                        h.get(&object, "options")?
                    };
                    h.get(&object, "applicationId")?
                }
            }
        };
        if {
            let condition = {
                let value = user.clone();
                h.boolean(!h.truthy(&value))
            };
            h.truthy(&condition)
        } {
            let _ = {
                let receiver = this_value.clone();
                let function = h.get(&receiver, "log")?;
                let mut arguments = Vec::new();
                let value = lit(h, "Skipping fetchIncomeHistory: no vault/applicationId");
                arguments.push(value);
                let value = {
                    let object = h.global("LogSeverity")?;
                    h.get(&object, "Warning")?
                };
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
            return Ok(RestFlow::Return({
                let array = h.array()?;
                array
            }));
        }
        let end_time = {
            let left = {
                'optional: {
                    let mut object = opts.clone();
                    if h.is_nullish(&object) {
                        break 'optional h.undefined();
                    }
                    object = h.get(&object, "endTime")?;
                    object
                }
            };
            if !h.is_nullish(&left) {
                left
            } else {
                {
                    let receiver = h.global("virtualClock")?;
                    let function = h.get(&receiver, "getCurrentTimeMs")?;
                    let arguments = Vec::new();
                    h.call(&function, &receiver, &arguments)?
                }
            }
        };
        let start_time = {
            let left = {
                'optional: {
                    let mut object = opts.clone();
                    if h.is_nullish(&object) {
                        break 'optional h.undefined();
                    }
                    object = h.get(&object, "startTime")?;
                    object
                }
            };
            if !h.is_nullish(&left) {
                left
            } else {
                {
                    let left = end_time.clone();
                    let right = {
                        let left = {
                            let left = {
                                let left = {
                                    let left = h.number(30.0);
                                    let right = h.number(24.0);
                                    h.position_operator("multiply", &left, &right)?
                                };
                                let right = h.number(60.0);
                                h.position_operator("multiply", &left, &right)?
                            };
                            let right = h.number(60.0);
                            h.position_operator("multiply", &left, &right)?
                        };
                        let right = h.number(1000.0);
                        h.position_operator("multiply", &left, &right)?
                    };
                    h.position_operator("subtract", &left, &right)?
                }
            }
        };
        let resolve_symbol =
            h.rest_callback(RestCallback::FetchIncomeHistory0([this_value.clone()]))?;
        let on_cap_hit =
            h.rest_callback(RestCallback::FetchIncomeHistory1([this_value.clone()]))?;
        let post = h.rest_callback(RestCallback::FetchIncomeHistory2([
            this_value.clone(),
            user.clone(),
        ]))?;
        {
            let outcome: Result<RestFlow<H::Value>, H::Error> = async {
                let records = {
                    let array = h.array()?;
                    array
                };
                if {
                    let condition = {
                        let left = {
                            let value = {
                                'optional: {
                                    let mut object = opts.clone();
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "incomeType")?;
                                    object
                                }
                            };
                            h.boolean(!h.truthy(&value))
                        };
                        if h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = {
                                    let object = opts.clone();
                                    h.get(&object, "incomeType")?
                                };
                                let right = lit(h, "FUNDING_FEE");
                                h.boolean(!h.strict_equal(&left, &right))
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    let fills = {
                        let value = {
                            let function = h.global("paginateHyperliquidByTime")?;
                            let receiver = h.undefined();
                            let mut arguments = Vec::new();
                            let value = post.clone();
                            arguments.push(value);
                            let value = {
                                let object = h.global("INFO_TYPES")?;
                                h.get(&object, "USER_FILLS_BY_TIME")?
                            };
                            arguments.push(value);
                            let value = start_time.clone();
                            arguments.push(value);
                            let value = end_time.clone();
                            arguments.push(value);
                            let value = h.rest_callback(RestCallback::FetchIncomeHistory3([]))?;
                            arguments.push(value);
                            let value = on_cap_hit.clone();
                            arguments.push(value);
                            h.call(&function, &receiver, &arguments)?
                        };
                        h.await_value(value).await?
                    };
                    {
                        let value = fills.clone();
                        let iterator = h.iterator(&value)?;
                        while let Some(fill) = h.next(&iterator)? {
                            let result: Result<RestFlow<H::Value>, H::Error> = async {
                                let _ = {
                                    let receiver = records.clone();
                                    let function = h.get(&receiver, "push")?;
                                    let mut arguments = Vec::new();
                                    let value = {
                                        let function =
                                            h.global("mapHyperliquidFillToIncomeRecords")?;
                                        let receiver = h.undefined();
                                        let mut arguments = Vec::new();
                                        let value = fill.clone();
                                        arguments.push(value);
                                        let value = resolve_symbol.clone();
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                    h.rest_argument_spread(&mut arguments, &value)?;
                                    h.call(&function, &receiver, &arguments)?
                                };
                                Ok(RestFlow::Normal)
                            }
                            .await;
                            match result {
                                Err(error) => {
                                    h.close_after_throw(&iterator);
                                    return Err(error);
                                }
                                Ok(RestFlow::Return(value)) => {
                                    h.close_after_return(&iterator)?;
                                    return Ok(RestFlow::Return(value));
                                }
                                Ok(RestFlow::Break) => {
                                    h.close_after_return(&iterator)?;
                                    break;
                                }
                                Ok(RestFlow::Normal | RestFlow::Next) => {}
                            }
                        }
                    }
                }
                if {
                    let condition = {
                        let left = {
                            let value = {
                                'optional: {
                                    let mut object = opts.clone();
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "incomeType")?;
                                    object
                                }
                            };
                            h.boolean(!h.truthy(&value))
                        };
                        if h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = {
                                    let object = opts.clone();
                                    h.get(&object, "incomeType")?
                                };
                                let right = lit(h, "FUNDING_FEE");
                                h.boolean(h.strict_equal(&left, &right))
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    let funding = {
                        let value = {
                            let function = h.global("paginateHyperliquidByTime")?;
                            let receiver = h.undefined();
                            let mut arguments = Vec::new();
                            let value = post.clone();
                            arguments.push(value);
                            let value = {
                                let object = h.global("INFO_TYPES")?;
                                h.get(&object, "USER_FUNDING")?
                            };
                            arguments.push(value);
                            let value = start_time.clone();
                            arguments.push(value);
                            let value = end_time.clone();
                            arguments.push(value);
                            let value = h.rest_callback(RestCallback::FetchIncomeHistory4([]))?;
                            arguments.push(value);
                            let value = on_cap_hit.clone();
                            arguments.push(value);
                            h.call(&function, &receiver, &arguments)?
                        };
                        h.await_value(value).await?
                    };
                    {
                        let value = funding.clone();
                        let iterator = h.iterator(&value)?;
                        while let Some(row) = h.next(&iterator)? {
                            let result: Result<RestFlow<H::Value>, H::Error> = async {
                                let record = {
                                    let function =
                                        h.global("mapHyperliquidFundingToIncomeRecord")?;
                                    let receiver = h.undefined();
                                    let mut arguments = Vec::new();
                                    let value = row.clone();
                                    arguments.push(value);
                                    let value = resolve_symbol.clone();
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                                if {
                                    let condition = record.clone();
                                    h.truthy(&condition)
                                } {
                                    let _ = {
                                        let receiver = records.clone();
                                        let function = h.get(&receiver, "push")?;
                                        let mut arguments = Vec::new();
                                        let value = record.clone();
                                        arguments.push(value);
                                        h.call(&function, &receiver, &arguments)?
                                    };
                                }
                                Ok(RestFlow::Normal)
                            }
                            .await;
                            match result {
                                Err(error) => {
                                    h.close_after_throw(&iterator);
                                    return Err(error);
                                }
                                Ok(RestFlow::Return(value)) => {
                                    h.close_after_return(&iterator)?;
                                    return Ok(RestFlow::Return(value));
                                }
                                Ok(RestFlow::Break) => {
                                    h.close_after_return(&iterator)?;
                                    break;
                                }
                                Ok(RestFlow::Normal | RestFlow::Next) => {}
                            }
                        }
                    }
                }
                let filtered = {
                    let condition = {
                        'optional: {
                            let mut object = opts.clone();
                            if h.is_nullish(&object) {
                                break 'optional h.undefined();
                            }
                            object = h.get(&object, "symbol")?;
                            object
                        }
                    };
                    if h.truthy(&condition) {
                        {
                            let receiver = records.clone();
                            let function = h.get(&receiver, "filter")?;
                            let mut arguments = Vec::new();
                            let value =
                                h.rest_callback(RestCallback::FetchIncomeHistory5([opts.clone()]))?;
                            arguments.push(value);
                            h.call(&function, &receiver, &arguments)?
                        }
                    } else {
                        records.clone()
                    }
                };
                return Ok(RestFlow::Return({
                    let receiver = filtered.clone();
                    let function = h.get(&receiver, "sort")?;
                    let mut arguments = Vec::new();
                    let value = h.rest_callback(RestCallback::FetchIncomeHistory6([]))?;
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }));
            }
            .await;
            match outcome {
                Ok(RestFlow::Normal) => {}
                Ok(flow) => return Ok(flow),
                Err(error) => {
                    let err = h.error_value(&error);
                    let _ = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "emitAndLogError")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let left = {
                                'optional: {
                                    let mut object = err.clone();
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "response")?;
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "data")?;
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "message")?;
                                    object
                                }
                            };
                            if h.truthy(&left) {
                                left
                            } else {
                                {
                                    'optional: {
                                        let mut object = err.clone();
                                        if h.is_nullish(&object) {
                                            break 'optional h.undefined();
                                        }
                                        object = h.get(&object, "message")?;
                                        object
                                    }
                                }
                            }
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    return Ok(RestFlow::Return({
                        let array = h.array()?;
                        array
                    }));
                }
            }
        }
        Ok(RestFlow::Normal)
    }
    .await;
    match outcome? {
        RestFlow::Return(value) => Ok(value),
        RestFlow::Normal => Ok(h.undefined()),
        RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
    }
}
pub fn fetch_income_history<H: RestTaskHost>(
    host: &H,
    this_value: &H::Value,
    opts: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let this_value = this_value.clone();
    let opts = opts.clone();
    crate::helper_tasks::start(host, async move {
        fetch_income_history_body(&mut h, &this_value, &opts).await
    })
}
/// Source: hyperliquid.exchange.ts::fetchHistoricalOrders
async fn fetch_historical_orders_body<H: RestReadHost>(
    h: &mut H,
    this_value: &H::Value,
    opts: &H::Value,
) -> Result<H::Value, H::Error> {
    let opts = opts.clone();
    let outcome: Result<RestFlow<H::Value>, H::Error> = async {
        if {
            let condition = {
                let object = this_value.clone();
                h.get(&object, "isPublicDataOnly")?
            };
            h.truthy(&condition)
        } {
            return Ok(RestFlow::Return({
                let array = h.array()?;
                array
            }));
        }
        let user = {
            let left = {
                let object = {
                    let object = this_value.clone();
                    h.get(&object, "options")?
                };
                h.get(&object, "vault")?
            };
            if h.truthy(&left) {
                left
            } else {
                {
                    let object = {
                        let object = this_value.clone();
                        h.get(&object, "options")?
                    };
                    h.get(&object, "applicationId")?
                }
            }
        };
        if {
            let condition = {
                let value = user.clone();
                h.boolean(!h.truthy(&value))
            };
            h.truthy(&condition)
        } {
            let _ = {
                let receiver = this_value.clone();
                let function = h.get(&receiver, "log")?;
                let mut arguments = Vec::new();
                let value = lit(h, "Skipping fetchHistoricalOrders: no vault/applicationId");
                arguments.push(value);
                let value = {
                    let object = h.global("LogSeverity")?;
                    h.get(&object, "Warning")?
                };
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
            return Ok(RestFlow::Return({
                let array = h.array()?;
                array
            }));
        }
        let snapshot_cap = h.number(2000.0);
        let resolve_symbol =
            h.rest_callback(RestCallback::FetchHistoricalOrders0([this_value.clone()]))?;
        {
            let outcome: Result<RestFlow<H::Value>, H::Error> = async {
                let binding = {
                    let value = {
                        let receiver = {
                            let object = this_value.clone();
                            h.get(&object, "infoXhr")?
                        };
                        let function = h.get(&receiver, "post")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let object = h.global("ENDPOINTS")?;
                            h.get(&object, "INFO")?
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.object()?;
                            let value = {
                                let object = h.global("INFO_TYPES")?;
                                h.get(&object, "HISTORICAL_ORDERS")?
                            };
                            h.define_data(&object, "type", &value)?;
                            let value = user.clone();
                            h.define_data(&object, "user", &value)?;
                            object
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.object()?;
                            let value = {
                                let object = h.object()?;
                                let value = lit(h, "application/json");
                                h.define_data(&object, "Content-Type", &value)?;
                                object
                            };
                            h.define_data(&object, "headers", &value)?;
                            object
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    h.await_value(value).await?
                };
                let data = {
                    let value = h.get(&binding, "data")?;
                    value
                };
                if {
                    let condition = {
                        let value = {
                            let receiver = h.global("Array")?;
                            let function = h.get(&receiver, "isArray")?;
                            let mut arguments = Vec::new();
                            let value = data.clone();
                            arguments.push(value);
                            h.call(&function, &receiver, &arguments)?
                        };
                        h.boolean(!h.truthy(&value))
                    };
                    h.truthy(&condition)
                } {
                    let _ = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "log")?;
                        let mut arguments = Vec::new();
                        let value = lit(
                            h,
                            "fetchHistoricalOrders got a non-array historicalOrders response",
                        );
                        arguments.push(value);
                        let value = {
                            let object = h.global("LogSeverity")?;
                            h.get(&object, "Warning")?
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    return Ok(RestFlow::Return({
                        let array = h.array()?;
                        array
                    }));
                }
                let rows = data.clone();
                if {
                    let condition = {
                        let left = {
                            let object = rows.clone();
                            h.get(&object, "length")?
                        };
                        let right = snapshot_cap.clone();
                        h.position_operator("greaterEqual", &left, &right)?
                    };
                    h.truthy(&condition)
                } {
                    let _ = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "log")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let mut units: Vec<u16> = vec![
                                102, 101, 116, 99, 104, 72, 105, 115, 116, 111, 114, 105, 99, 97,
                                108, 79, 114, 100, 101, 114, 115, 32, 114, 101, 99, 101, 105, 118,
                                101, 100, 32, 116, 104, 101, 32, 102, 117, 108, 108, 32,
                            ];
                            let value = snapshot_cap.clone();
                            units.extend(h.template_string(&value)?.0);
                            units.extend_from_slice(&[
                                45, 114, 111, 119, 32, 104, 105, 115, 116, 111, 114, 105, 99, 97,
                                108, 79, 114, 100, 101, 114, 115, 32, 115, 110, 97, 112, 115, 104,
                                111, 116, 59, 32, 111, 108, 100, 101, 114, 32, 111, 114, 100, 101,
                                114, 115, 32, 97, 114, 101, 32, 110, 111, 116, 32, 114, 101, 116,
                                114, 105, 101, 118, 97, 98, 108, 101,
                            ]);
                            h.literal(&JsString(units))
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.global("LogSeverity")?;
                            h.get(&object, "Warning")?
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                }
                let orders = {
                    let receiver = {
                        let receiver = {
                            let receiver = {
                                let receiver = {
                                    let function = h.global("collapseHyperliquidOrderHistory")?;
                                    let receiver = h.undefined();
                                    let mut arguments = Vec::new();
                                    let value = rows.clone();
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                                let function = h.get(&receiver, "map")?;
                                let mut arguments = Vec::new();
                                let value =
                                    h.rest_callback(RestCallback::FetchHistoricalOrders1([
                                        resolve_symbol.clone(),
                                    ]))?;
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            let function = h.get(&receiver, "filter")?;
                            let mut arguments = Vec::new();
                            let value =
                                h.rest_callback(RestCallback::FetchHistoricalOrders2([]))?;
                            arguments.push(value);
                            h.call(&function, &receiver, &arguments)?
                        };
                        let function = h.get(&receiver, "filter")?;
                        let mut arguments = Vec::new();
                        let value =
                            h.rest_callback(RestCallback::FetchHistoricalOrders3([opts.clone()]))?;
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    let function = h.get(&receiver, "sort")?;
                    let mut arguments = Vec::new();
                    let value = h.rest_callback(RestCallback::FetchHistoricalOrders4([]))?;
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                };
                return Ok(RestFlow::Return({
                    let condition = {
                        let left = {
                            'optional: {
                                let mut object = opts.clone();
                                if h.is_nullish(&object) {
                                    break 'optional h.undefined();
                                }
                                object = h.get(&object, "limit")?;
                                object
                            }
                        };
                        if !h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = {
                                    let object = opts.clone();
                                    h.get(&object, "limit")?
                                };
                                let right = h.number(0.0);
                                h.position_operator("greater", &left, &right)?
                            }
                        }
                    };
                    if h.truthy(&condition) {
                        {
                            let receiver = orders.clone();
                            let function = h.get(&receiver, "slice")?;
                            let mut arguments = Vec::new();
                            let value = h.number(0.0);
                            arguments.push(value);
                            let value = {
                                let object = opts.clone();
                                h.get(&object, "limit")?
                            };
                            arguments.push(value);
                            h.call(&function, &receiver, &arguments)?
                        }
                    } else {
                        orders.clone()
                    }
                }));
            }
            .await;
            match outcome {
                Ok(RestFlow::Normal) => {}
                Ok(flow) => return Ok(flow),
                Err(error) => {
                    let err = h.error_value(&error);
                    let _ = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "emitAndLogError")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let left = {
                                'optional: {
                                    let mut object = err.clone();
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "response")?;
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "data")?;
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "message")?;
                                    object
                                }
                            };
                            if h.truthy(&left) {
                                left
                            } else {
                                {
                                    'optional: {
                                        let mut object = err.clone();
                                        if h.is_nullish(&object) {
                                            break 'optional h.undefined();
                                        }
                                        object = h.get(&object, "message")?;
                                        object
                                    }
                                }
                            }
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    return Ok(RestFlow::Return({
                        let array = h.array()?;
                        array
                    }));
                }
            }
        }
        Ok(RestFlow::Normal)
    }
    .await;
    match outcome? {
        RestFlow::Return(value) => Ok(value),
        RestFlow::Normal => Ok(h.undefined()),
        RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
    }
}
pub fn fetch_historical_orders<H: RestTaskHost>(
    host: &H,
    this_value: &H::Value,
    opts: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let this_value = this_value.clone();
    let opts = opts.clone();
    crate::helper_tasks::start(host, async move {
        fetch_historical_orders_body(&mut h, &this_value, &opts).await
    })
}
/// Source: hyperliquid.exchange.ts::deriveClosedPositions
async fn derive_closed_positions_body<H: RestReadHost>(
    h: &mut H,
    this_value: &H::Value,
    opts: &H::Value,
) -> Result<H::Value, H::Error> {
    let opts = opts.clone();
    let outcome: Result<RestFlow<H::Value>, H::Error> = async {
        if {
            let condition = {
                let object = this_value.clone();
                h.get(&object, "isPublicDataOnly")?
            };
            h.truthy(&condition)
        } {
            return Ok(RestFlow::Return({
                let array = h.array()?;
                array
            }));
        }
        let user = {
            let left = {
                let object = {
                    let object = this_value.clone();
                    h.get(&object, "options")?
                };
                h.get(&object, "vault")?
            };
            if h.truthy(&left) {
                left
            } else {
                {
                    let object = {
                        let object = this_value.clone();
                        h.get(&object, "options")?
                    };
                    h.get(&object, "applicationId")?
                }
            }
        };
        if {
            let condition = {
                let value = user.clone();
                h.boolean(!h.truthy(&value))
            };
            h.truthy(&condition)
        } {
            return Ok(RestFlow::Return({
                let array = h.array()?;
                array
            }));
        }
        let to = {
            let left = {
                let object = opts.clone();
                h.get(&object, "to")?
            };
            if !h.is_nullish(&left) {
                left
            } else {
                {
                    let receiver = h.global("virtualClock")?;
                    let function = h.get(&receiver, "getCurrentTimeMs")?;
                    let arguments = Vec::new();
                    h.call(&function, &receiver, &arguments)?
                }
            }
        };
        let from = {
            let left = {
                let object = opts.clone();
                h.get(&object, "from")?
            };
            if !h.is_nullish(&left) {
                left
            } else {
                {
                    let left = to.clone();
                    let right = {
                        let left = {
                            let left = {
                                let left = {
                                    let left = h.number(30.0);
                                    let right = h.number(24.0);
                                    h.position_operator("multiply", &left, &right)?
                                };
                                let right = h.number(60.0);
                                h.position_operator("multiply", &left, &right)?
                            };
                            let right = h.number(60.0);
                            h.position_operator("multiply", &left, &right)?
                        };
                        let right = h.number(1000.0);
                        h.position_operator("multiply", &left, &right)?
                    };
                    h.position_operator("subtract", &left, &right)?
                }
            }
        };
        let resolve_symbol =
            h.rest_callback(RestCallback::DeriveClosedPositions0([this_value.clone()]))?;
        let post = h.rest_callback(RestCallback::DeriveClosedPositions1([
            this_value.clone(),
            user.clone(),
        ]))?;
        {
            let outcome: Result<RestFlow<H::Value>, H::Error> = async {
                let fills = {
                    let value = {
                        let function = h.global("paginateHyperliquidByTime")?;
                        let receiver = h.undefined();
                        let mut arguments = Vec::new();
                        let value = post.clone();
                        arguments.push(value);
                        let value = {
                            let object = h.global("INFO_TYPES")?;
                            h.get(&object, "USER_FILLS_BY_TIME")?
                        };
                        arguments.push(value);
                        let value = from.clone();
                        arguments.push(value);
                        let value = to.clone();
                        arguments.push(value);
                        let value = h.rest_callback(RestCallback::DeriveClosedPositions2([]))?;
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    h.await_value(value).await?
                };
                let entries = {
                    let function = h.global("deriveHyperliquidClosedPositions")?;
                    let receiver = h.undefined();
                    let mut arguments = Vec::new();
                    let value = fills.clone();
                    arguments.push(value);
                    let value = resolve_symbol.clone();
                    arguments.push(value);
                    let value = {
                        let left = {
                            let object = opts.clone();
                            h.get(&object, "accountName")?
                        };
                        if h.truthy(&left) { left } else { lit(h, "") }
                    };
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                };
                return Ok(RestFlow::Return({
                    let condition = {
                        let object = opts.clone();
                        h.get(&object, "symbol")?
                    };
                    if h.truthy(&condition) {
                        {
                            let receiver = entries.clone();
                            let function = h.get(&receiver, "filter")?;
                            let mut arguments = Vec::new();
                            let value = h.rest_callback(RestCallback::DeriveClosedPositions3([
                                opts.clone(),
                            ]))?;
                            arguments.push(value);
                            h.call(&function, &receiver, &arguments)?
                        }
                    } else {
                        entries.clone()
                    }
                }));
            }
            .await;
            match outcome {
                Ok(RestFlow::Normal) => {}
                Ok(flow) => return Ok(flow),
                Err(error) => {
                    let err = h.error_value(&error);
                    let _ = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "emitAndLogError")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let left = {
                                'optional: {
                                    let mut object = err.clone();
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "response")?;
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "data")?;
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "message")?;
                                    object
                                }
                            };
                            if h.truthy(&left) {
                                left
                            } else {
                                {
                                    'optional: {
                                        let mut object = err.clone();
                                        if h.is_nullish(&object) {
                                            break 'optional h.undefined();
                                        }
                                        object = h.get(&object, "message")?;
                                        object
                                    }
                                }
                            }
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    return Ok(RestFlow::Return({
                        let array = h.array()?;
                        array
                    }));
                }
            }
        }
        Ok(RestFlow::Normal)
    }
    .await;
    match outcome? {
        RestFlow::Return(value) => Ok(value),
        RestFlow::Normal => Ok(h.undefined()),
        RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
    }
}
pub fn derive_closed_positions<H: RestTaskHost>(
    host: &H,
    this_value: &H::Value,
    opts: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let this_value = this_value.clone();
    let opts = opts.clone();
    crate::helper_tasks::start(host, async move {
        derive_closed_positions_body(&mut h, &this_value, &opts).await
    })
}
/// Source: hyperliquid.exchange.ts::fetchIncomeHistoryRange
async fn fetch_income_history_range_body<H: RestReadHost>(
    h: &mut H,
    this_value: &H::Value,
    start_time: &H::Value,
    end_time: &H::Value,
    income_type: &H::Value,
) -> Result<H::Value, H::Error> {
    let start_time = start_time.clone();
    let end_time = end_time.clone();
    let income_type = income_type.clone();
    let outcome: Result<RestFlow<H::Value>, H::Error> = async {
        return Ok(RestFlow::Return({
            let value = {
                let receiver = this_value.clone();
                let function = h.get(&receiver, "fetchIncomeHistory")?;
                let mut arguments = Vec::new();
                let value = {
                    let object = h.object()?;
                    let value = start_time.clone();
                    h.define_data(&object, "startTime", &value)?;
                    let value = end_time.clone();
                    h.define_data(&object, "endTime", &value)?;
                    let value = income_type.clone();
                    h.define_data(&object, "incomeType", &value)?;
                    object
                };
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
            h.await_value(value).await?
        }));
    }
    .await;
    match outcome? {
        RestFlow::Return(value) => Ok(value),
        RestFlow::Normal => Ok(h.undefined()),
        RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
    }
}
pub fn fetch_income_history_range<H: RestTaskHost>(
    host: &H,
    this_value: &H::Value,
    start_time: &H::Value,
    end_time: &H::Value,
    income_type: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let this_value = this_value.clone();
    let start_time = start_time.clone();
    let end_time = end_time.clone();
    let income_type = income_type.clone();
    crate::helper_tasks::start(host, async move {
        fetch_income_history_range_body(&mut h, &this_value, &start_time, &end_time, &income_type)
            .await
    })
}
/// Source: hyperliquid.exchange.ts::fetchHistoricalFills
async fn fetch_historical_fills_body<H: RestReadHost>(
    h: &mut H,
    this_value: &H::Value,
    opts: &H::Value,
) -> Result<H::Value, H::Error> {
    let opts = opts.clone();
    let outcome: Result<RestFlow<H::Value>, H::Error> = async {
        if {
            let condition = {
                let object = this_value.clone();
                h.get(&object, "isPublicDataOnly")?
            };
            h.truthy(&condition)
        } {
            return Ok(RestFlow::Return({
                let array = h.array()?;
                array
            }));
        }
        let binding = opts.clone();
        let symbol = {
            let value = h.get(&binding, "symbol")?;
            value
        };
        let order_id = {
            let value = h.get(&binding, "orderId")?;
            value
        };
        let from = {
            let value = h.get(&binding, "from")?;
            value
        };
        let to = {
            let value = h.get(&binding, "to")?;
            value
        };
        let limit = {
            let value = h.get(&binding, "limit")?;
            if h.strict_equal(&value, &h.undefined()) {
                h.number(50.0)
            } else {
                value
            }
        };
        if {
            let condition = {
                let value = symbol.clone();
                h.boolean(!h.truthy(&value))
            };
            h.truthy(&condition)
        } {
            return Ok(RestFlow::Return({
                let array = h.array()?;
                array
            }));
        }
        let normalize_to_milliseconds = h.rest_callback(RestCallback::FetchHistoricalFills0([]))?;
        let requested_from_milliseconds = {
            let function = normalize_to_milliseconds.clone();
            let receiver = h.undefined();
            let mut arguments = Vec::new();
            let value = from.clone();
            arguments.push(value);
            h.call(&function, &receiver, &arguments)?
        };
        let requested_to_milliseconds = {
            let left = {
                let function = normalize_to_milliseconds.clone();
                let receiver = h.undefined();
                let mut arguments = Vec::new();
                let value = to.clone();
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
            if !h.is_nullish(&left) {
                left
            } else {
                {
                    let receiver = h.global("virtualClock")?;
                    let function = h.get(&receiver, "getCurrentTimeMs")?;
                    let arguments = Vec::new();
                    h.call(&function, &receiver, &arguments)?
                }
            }
        };
        let max_limit = {
            let receiver = h.global("Math")?;
            let function = h.get(&receiver, "min")?;
            let mut arguments = Vec::new();
            let value = {
                let receiver = h.global("Math")?;
                let function = h.get(&receiver, "max")?;
                let mut arguments = Vec::new();
                let value = limit.clone();
                arguments.push(value);
                let value = h.number(1.0);
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
            arguments.push(value);
            let value = h.number(500.0);
            arguments.push(value);
            h.call(&function, &receiver, &arguments)?
        };
        let execution_query_max_range_ms = {
            let left = {
                let left = {
                    let left = {
                        let left = {
                            let left = h.number(7.0);
                            let right = h.number(24.0);
                            h.position_operator("multiply", &left, &right)?
                        };
                        let right = h.number(60.0);
                        h.position_operator("multiply", &left, &right)?
                    };
                    let right = h.number(60.0);
                    h.position_operator("multiply", &left, &right)?
                };
                let right = h.number(1000.0);
                h.position_operator("multiply", &left, &right)?
            };
            let right = h.number(1.0);
            h.position_operator("subtract", &left, &right)?
        };
        let default_executions_lookback_ms = {
            let left = {
                let left = {
                    let left = {
                        let left = h.number(30.0);
                        let right = h.number(24.0);
                        h.position_operator("multiply", &left, &right)?
                    };
                    let right = h.number(60.0);
                    h.position_operator("multiply", &left, &right)?
                };
                let right = h.number(60.0);
                h.position_operator("multiply", &left, &right)?
            };
            let right = h.number(1000.0);
            h.position_operator("multiply", &left, &right)?
        };
        let minimum_allowed_from_milliseconds = {
            let left = requested_to_milliseconds.clone();
            let right = default_executions_lookback_ms.clone();
            h.position_operator("subtract", &left, &right)?
        };
        let effective_from_milliseconds = {
            let receiver = h.global("Math")?;
            let function = h.get(&receiver, "max")?;
            let mut arguments = Vec::new();
            let value = {
                let left = requested_from_milliseconds.clone();
                if !h.is_nullish(&left) {
                    left
                } else {
                    minimum_allowed_from_milliseconds.clone()
                }
            };
            arguments.push(value);
            let value = minimum_allowed_from_milliseconds.clone();
            arguments.push(value);
            h.call(&function, &receiver, &arguments)?
        };
        if {
            let condition = {
                let left = {
                    let left = requested_from_milliseconds.clone();
                    let right = h.undefined();
                    h.boolean(!h.strict_equal(&left, &right))
                };
                if !h.truthy(&left) {
                    left
                } else {
                    {
                        let left = requested_from_milliseconds.clone();
                        let right = minimum_allowed_from_milliseconds.clone();
                        h.position_operator("less", &left, &right)?
                    }
                }
            };
            h.truthy(&condition)
        } {
            let _ = {
                let receiver = this_value.clone();
                let function = h.get(&receiver, "log")?;
                let mut arguments = Vec::new();
                let value = {
                    let mut units: Vec<u16> = vec![
                        102, 101, 116, 99, 104, 72, 105, 115, 116, 111, 114, 105, 99, 97, 108, 70,
                        105, 108, 108, 115, 32, 99, 97, 112, 112, 101, 100, 32, 116, 111, 32, 108,
                        97, 115, 116, 32, 51, 48, 32, 100, 97, 121, 115, 32, 102, 111, 114, 32,
                    ];
                    let value = symbol.clone();
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[
                        32, 40, 114, 101, 113, 117, 101, 115, 116, 101, 100, 70, 114, 111, 109, 61,
                    ]);
                    let value = requested_from_milliseconds.clone();
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[
                        44, 32, 99, 97, 112, 112, 101, 100, 70, 114, 111, 109, 61,
                    ]);
                    let value = effective_from_milliseconds.clone();
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[41]);
                    h.literal(&JsString(units))
                };
                arguments.push(value);
                let value = {
                    let object = h.global("LogSeverity")?;
                    h.get(&object, "Warning")?
                };
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
        }
        let market = {
            let object = {
                let object = {
                    let object = this_value.clone();
                    h.get(&object, "store")?
                };
                h.get(&object, "marketsBySymbol")?
            };
            {
                let key = symbol.clone();
                h.index(&object, &key)?
            }
        };
        if {
            let condition = {
                let value = market.clone();
                h.boolean(!h.truthy(&value))
            };
            h.truthy(&condition)
        } {
            let _ = {
                let receiver = this_value.clone();
                let function = h.get(&receiver, "log")?;
                let mut arguments = Vec::new();
                let value = {
                    let mut units: Vec<u16> = vec![77, 97, 114, 107, 101, 116, 32];
                    let value = symbol.clone();
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[32, 110, 111, 116, 32, 102, 111, 117, 110, 100]);
                    h.literal(&JsString(units))
                };
                arguments.push(value);
                let value = {
                    let object = h.global("LogSeverity")?;
                    h.get(&object, "Warning")?
                };
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
            return Ok(RestFlow::Return({
                let array = h.array()?;
                array
            }));
        }
        if {
            let condition = {
                let left = effective_from_milliseconds.clone();
                let right = requested_to_milliseconds.clone();
                h.position_operator("greater", &left, &right)?
            };
            h.truthy(&condition)
        } {
            return Ok(RestFlow::Return({
                let array = h.array()?;
                array
            }));
        }
        let query_windows = {
            let array = h.array()?;
            array
        };
        let mut window_end = requested_to_milliseconds.clone();
        while {
            let condition = {
                let left = window_end.clone();
                let right = effective_from_milliseconds.clone();
                h.position_operator("greaterEqual", &left, &right)?
            };
            h.truthy(&condition)
        } {
            let result: Result<RestFlow<H::Value>, H::Error> = async {
                let window_start = {
                    let receiver = h.global("Math")?;
                    let function = h.get(&receiver, "max")?;
                    let mut arguments = Vec::new();
                    let value = effective_from_milliseconds.clone();
                    arguments.push(value);
                    let value = {
                        let left = window_end.clone();
                        let right = execution_query_max_range_ms.clone();
                        h.position_operator("subtract", &left, &right)?
                    };
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                };
                let _ = {
                    let receiver = query_windows.clone();
                    let function = h.get(&receiver, "push")?;
                    let mut arguments = Vec::new();
                    let value = {
                        let object = h.object()?;
                        let value = window_start.clone();
                        h.define_data(&object, "startTime", &value)?;
                        let value = window_end.clone();
                        h.define_data(&object, "endTime", &value)?;
                        object
                    };
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                };
                if {
                    let condition = {
                        let left = window_start.clone();
                        let right = effective_from_milliseconds.clone();
                        h.position_operator("lessEqual", &left, &right)?
                    };
                    h.truthy(&condition)
                } {
                    return Ok(RestFlow::Break);
                }
                window_end = {
                    let left = window_start.clone();
                    let right = h.number(1.0);
                    h.position_operator("subtract", &left, &right)?
                };
                Ok(RestFlow::Normal)
            }
            .await;
            match result? {
                RestFlow::Return(value) => return Ok(RestFlow::Return(value)),
                RestFlow::Break => break,
                RestFlow::Normal | RestFlow::Next => {}
            }
        }
        let user = {
            let object = {
                let object = this_value.clone();
                h.get(&object, "options")?
            };
            h.get(&object, "applicationId")?
        };
        if {
            let condition = {
                let value = user.clone();
                h.boolean(!h.truthy(&value))
            };
            h.truthy(&condition)
        } {
            let _ = {
                let receiver = this_value.clone();
                let function = h.get(&receiver, "log")?;
                let mut arguments = Vec::new();
                let value = lit(h, "Skipping fetchHistoricalFills: applicationId missing");
                arguments.push(value);
                let value = {
                    let object = h.global("LogSeverity")?;
                    h.get(&object, "Warning")?
                };
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
            return Ok(RestFlow::Return({
                let array = h.array()?;
                array
            }));
        }
        let coin_candidates = {
            let function = h.global("Set")?;
            let arg0 = {
                let receiver = {
                    let receiver = {
                        let array = h.array()?;
                        let value = {
                            'optional: {
                                let mut object = market.clone();
                                object = h.get(&object, "info")?;
                                if h.is_nullish(&object) {
                                    break 'optional h.undefined();
                                }
                                object = h.get(&object, "name")?;
                                object
                            }
                        };
                        h.array_define(&array, 0, &value)?;
                        let value = {
                            let object = market.clone();
                            h.get(&object, "id")?
                        };
                        h.array_define(&array, 1, &value)?;
                        let value = {
                            let function = h.global("symbolToCoin")?;
                            let receiver = h.undefined();
                            let mut arguments = Vec::new();
                            let value = symbol.clone();
                            arguments.push(value);
                            h.call(&function, &receiver, &arguments)?
                        };
                        h.array_define(&array, 2, &value)?;
                        array
                    };
                    let function = h.get(&receiver, "map")?;
                    let mut arguments = Vec::new();
                    let value = h.rest_callback(RestCallback::FetchHistoricalFills1([]))?;
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                };
                let function = h.get(&receiver, "filter")?;
                let mut arguments = Vec::new();
                let value = h.global("Boolean")?;
                arguments.push(value);
                h.call(&function, &receiver, &arguments)?
            };
            h.construct(&function, &[arg0])?
        };
        let parse_side = h.rest_callback(RestCallback::FetchHistoricalFills2([]))?;
        {
            let outcome: Result<RestFlow<H::Value>, H::Error> = async {
                let deduped = {
                    let function = h.global("Map")?;
                    h.construct(&function, &[])?
                };
                {
                    let value = query_windows.clone();
                    let iterator = h.iterator(&value)?;
                    while let Some(query_window) = h.next(&iterator)? {
                        let result: Result<RestFlow<H::Value>, H::Error> = async {
                            let binding = {
                                let value = {
                                    let receiver = {
                                        let object = this_value.clone();
                                        h.get(&object, "xhr")?
                                    };
                                    let function = h.get(&receiver, "post")?;
                                    let mut arguments = Vec::new();
                                    let value = {
                                        let object = h.global("ENDPOINTS")?;
                                        h.get(&object, "INFO")?
                                    };
                                    arguments.push(value);
                                    let value = {
                                        let object = h.object()?;
                                        let value = {
                                            let object = h.global("INFO_TYPES")?;
                                            h.get(&object, "USER_FILLS_BY_TIME")?
                                        };
                                        h.define_data(&object, "type", &value)?;
                                        let value = user.clone();
                                        h.define_data(&object, "user", &value)?;
                                        let value = {
                                            let object = query_window.clone();
                                            h.get(&object, "startTime")?
                                        };
                                        h.define_data(&object, "startTime", &value)?;
                                        let value = {
                                            let object = query_window.clone();
                                            h.get(&object, "endTime")?
                                        };
                                        h.define_data(&object, "endTime", &value)?;
                                        object
                                    };
                                    arguments.push(value);
                                    let value = {
                                        let object = h.object()?;
                                        let value = {
                                            let object = h.object()?;
                                            let value = lit(h, "application/json");
                                            h.define_data(&object, "Content-Type", &value)?;
                                            object
                                        };
                                        h.define_data(&object, "headers", &value)?;
                                        object
                                    };
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                                h.await_value(value).await?
                            };
                            let data = {
                                let value = h.get(&binding, "data")?;
                                value
                            };
                            if {
                                let condition = {
                                    let left = {
                                        let value = {
                                            let receiver = h.global("Array")?;
                                            let function = h.get(&receiver, "isArray")?;
                                            let mut arguments = Vec::new();
                                            let value = data.clone();
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        h.boolean(!h.truthy(&value))
                                    };
                                    if h.truthy(&left) {
                                        left
                                    } else {
                                        {
                                            let left = {
                                                let object = data.clone();
                                                h.get(&object, "length")?
                                            };
                                            let right = h.number(0.0);
                                            h.boolean(h.strict_equal(&left, &right))
                                        }
                                    }
                                };
                                h.truthy(&condition)
                            } {
                                return Ok(RestFlow::Next);
                            }
                            {
                                let value = data.clone();
                                let iterator = h.iterator(&value)?;
                                while let Some(fill) = h.next(&iterator)? {
                                    let result: Result<RestFlow<H::Value>, H::Error> = async {
                                        let fill_coin = {
                                            let receiver = {
                                                let mut units: Vec<u16> = vec![];
                                                let value = {
                                                    let left = {
                                                        'optional: {
                                                            let mut object = fill.clone();
                                                            if h.is_nullish(&object) {
                                                                break 'optional h.undefined();
                                                            }
                                                            object = h.get(&object, "coin")?;
                                                            object
                                                        }
                                                    };
                                                    if h.truthy(&left) { left } else { lit(h, "") }
                                                };
                                                units.extend(h.template_string(&value)?.0);
                                                units.extend_from_slice(&[]);
                                                h.literal(&JsString(units))
                                            };
                                            let function = h.get(&receiver, "trim")?;
                                            let arguments = Vec::new();
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        if {
                                            let condition = {
                                                let value = {
                                                    let receiver = coin_candidates.clone();
                                                    let function = h.get(&receiver, "has")?;
                                                    let mut arguments = Vec::new();
                                                    let value = fill_coin.clone();
                                                    arguments.push(value);
                                                    h.call(&function, &receiver, &arguments)?
                                                };
                                                h.boolean(!h.truthy(&value))
                                            };
                                            h.truthy(&condition)
                                        } {
                                            return Ok(RestFlow::Next);
                                        }
                                        let execution_order_id = {
                                            let function = h.global("String")?;
                                            let receiver = h.undefined();
                                            let mut arguments = Vec::new();
                                            let value = {
                                                let left = {
                                                    let left = {
                                                        'optional: {
                                                            let mut object = fill.clone();
                                                            if h.is_nullish(&object) {
                                                                break 'optional h.undefined();
                                                            }
                                                            object = h.get(&object, "oid")?;
                                                            object
                                                        }
                                                    };
                                                    if h.truthy(&left) {
                                                        left
                                                    } else {
                                                        {
                                                            'optional: {
                                                                let mut object = fill.clone();
                                                                if h.is_nullish(&object) {
                                                                    break 'optional h.undefined();
                                                                }
                                                                object =
                                                                    h.get(&object, "orderId")?;
                                                                object
                                                            }
                                                        }
                                                    }
                                                };
                                                if h.truthy(&left) { left } else { lit(h, "") }
                                            };
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        if {
                                            let condition = {
                                                let left = {
                                                    let left = order_id.clone();
                                                    if !h.truthy(&left) {
                                                        left
                                                    } else {
                                                        execution_order_id.clone()
                                                    }
                                                };
                                                if !h.truthy(&left) {
                                                    left
                                                } else {
                                                    {
                                                        let left = execution_order_id.clone();
                                                        let right = {
                                                            let function = h.global("String")?;
                                                            let receiver = h.undefined();
                                                            let mut arguments = Vec::new();
                                                            let value = order_id.clone();
                                                            arguments.push(value);
                                                            h.call(
                                                                &function, &receiver, &arguments,
                                                            )?
                                                        };
                                                        h.boolean(!h.strict_equal(&left, &right))
                                                    }
                                                }
                                            };
                                            h.truthy(&condition)
                                        } {
                                            return Ok(RestFlow::Next);
                                        }
                                        let timestamp_milliseconds = {
                                            let function = h.global("Number")?;
                                            let receiver = h.undefined();
                                            let mut arguments = Vec::new();
                                            let value = {
                                                'optional: {
                                                    let mut object = fill.clone();
                                                    if h.is_nullish(&object) {
                                                        break 'optional h.undefined();
                                                    }
                                                    object = h.get(&object, "time")?;
                                                    object
                                                }
                                            };
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        if {
                                            let condition = {
                                                let left = {
                                                    let value = {
                                                        let receiver = h.global("Number")?;
                                                        let function =
                                                            h.get(&receiver, "isFinite")?;
                                                        let mut arguments = Vec::new();
                                                        let value = timestamp_milliseconds.clone();
                                                        arguments.push(value);
                                                        h.call(&function, &receiver, &arguments)?
                                                    };
                                                    h.boolean(!h.truthy(&value))
                                                };
                                                if h.truthy(&left) {
                                                    left
                                                } else {
                                                    {
                                                        let left = timestamp_milliseconds.clone();
                                                        let right = h.number(0.0);
                                                        h.position_operator(
                                                            "lessEqual",
                                                            &left,
                                                            &right,
                                                        )?
                                                    }
                                                }
                                            };
                                            h.truthy(&condition)
                                        } {
                                            return Ok(RestFlow::Next);
                                        }
                                        if {
                                            let condition = {
                                                let left = {
                                                    let left = {
                                                        let left =
                                                            requested_from_milliseconds.clone();
                                                        let right = h.undefined();
                                                        h.boolean(!h.strict_equal(&left, &right))
                                                    };
                                                    if !h.truthy(&left) {
                                                        left
                                                    } else {
                                                        {
                                                            let left =
                                                                timestamp_milliseconds.clone();
                                                            let right =
                                                                requested_from_milliseconds.clone();
                                                            h.position_operator(
                                                                "less", &left, &right,
                                                            )?
                                                        }
                                                    }
                                                };
                                                if h.truthy(&left) {
                                                    left
                                                } else {
                                                    {
                                                        let left = timestamp_milliseconds.clone();
                                                        let right =
                                                            requested_to_milliseconds.clone();
                                                        h.position_operator(
                                                            "greater", &left, &right,
                                                        )?
                                                    }
                                                }
                                            };
                                            h.truthy(&condition)
                                        } {
                                            return Ok(RestFlow::Next);
                                        }
                                        let price = {
                                            let receiver = h.global("Number")?;
                                            let function = h.get(&receiver, "parseFloat")?;
                                            let mut arguments = Vec::new();
                                            let value = {
                                                let function = h.global("String")?;
                                                let receiver = h.undefined();
                                                let mut arguments = Vec::new();
                                                let value = {
                                                    let left = {
                                                        'optional: {
                                                            let mut object = fill.clone();
                                                            if h.is_nullish(&object) {
                                                                break 'optional h.undefined();
                                                            }
                                                            object = h.get(&object, "px")?;
                                                            object
                                                        }
                                                    };
                                                    if h.truthy(&left) {
                                                        left
                                                    } else {
                                                        h.number(0.0)
                                                    }
                                                };
                                                arguments.push(value);
                                                h.call(&function, &receiver, &arguments)?
                                            };
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        let qty = {
                                            let receiver = h.global("Number")?;
                                            let function = h.get(&receiver, "parseFloat")?;
                                            let mut arguments = Vec::new();
                                            let value = {
                                                let function = h.global("String")?;
                                                let receiver = h.undefined();
                                                let mut arguments = Vec::new();
                                                let value = {
                                                    let left = {
                                                        'optional: {
                                                            let mut object = fill.clone();
                                                            if h.is_nullish(&object) {
                                                                break 'optional h.undefined();
                                                            }
                                                            object = h.get(&object, "sz")?;
                                                            object
                                                        }
                                                    };
                                                    if h.truthy(&left) {
                                                        left
                                                    } else {
                                                        h.number(0.0)
                                                    }
                                                };
                                                arguments.push(value);
                                                h.call(&function, &receiver, &arguments)?
                                            };
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        if {
                                            let condition = {
                                                let left = {
                                                    let left = {
                                                        let left = {
                                                            let value = {
                                                                let receiver =
                                                                    h.global("Number")?;
                                                                let function =
                                                                    h.get(&receiver, "isFinite")?;
                                                                let mut arguments = Vec::new();
                                                                let value = price.clone();
                                                                arguments.push(value);
                                                                h.call(
                                                                    &function, &receiver,
                                                                    &arguments,
                                                                )?
                                                            };
                                                            h.boolean(!h.truthy(&value))
                                                        };
                                                        if h.truthy(&left) {
                                                            left
                                                        } else {
                                                            {
                                                                let value = {
                                                                    let receiver =
                                                                        h.global("Number")?;
                                                                    let function = h.get(
                                                                        &receiver, "isFinite",
                                                                    )?;
                                                                    let mut arguments = Vec::new();
                                                                    let value = qty.clone();
                                                                    arguments.push(value);
                                                                    h.call(
                                                                        &function, &receiver,
                                                                        &arguments,
                                                                    )?
                                                                };
                                                                h.boolean(!h.truthy(&value))
                                                            }
                                                        }
                                                    };
                                                    if h.truthy(&left) {
                                                        left
                                                    } else {
                                                        {
                                                            let left = price.clone();
                                                            let right = h.number(0.0);
                                                            h.position_operator(
                                                                "lessEqual",
                                                                &left,
                                                                &right,
                                                            )?
                                                        }
                                                    }
                                                };
                                                if h.truthy(&left) {
                                                    left
                                                } else {
                                                    {
                                                        let left = qty.clone();
                                                        let right = h.number(0.0);
                                                        h.position_operator(
                                                            "lessEqual",
                                                            &left,
                                                            &right,
                                                        )?
                                                    }
                                                }
                                            };
                                            h.truthy(&condition)
                                        } {
                                            return Ok(RestFlow::Next);
                                        }
                                        let side = {
                                            let function = parse_side.clone();
                                            let receiver = h.undefined();
                                            let mut arguments = Vec::new();
                                            let value = {
                                                'optional: {
                                                    let mut object = fill.clone();
                                                    if h.is_nullish(&object) {
                                                        break 'optional h.undefined();
                                                    }
                                                    object = h.get(&object, "side")?;
                                                    object
                                                }
                                            };
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        if {
                                            let condition = {
                                                let value = side.clone();
                                                h.boolean(!h.truthy(&value))
                                            };
                                            h.truthy(&condition)
                                        } {
                                            return Ok(RestFlow::Next);
                                        }
                                        let fee_raw = {
                                            let receiver = h.global("Number")?;
                                            let function = h.get(&receiver, "parseFloat")?;
                                            let mut arguments = Vec::new();
                                            let value = {
                                                let function = h.global("String")?;
                                                let receiver = h.undefined();
                                                let mut arguments = Vec::new();
                                                let value = {
                                                    let left = {
                                                        'optional: {
                                                            let mut object = fill.clone();
                                                            if h.is_nullish(&object) {
                                                                break 'optional h.undefined();
                                                            }
                                                            object = h.get(&object, "fee")?;
                                                            object
                                                        }
                                                    };
                                                    if h.truthy(&left) {
                                                        left
                                                    } else {
                                                        h.number(0.0)
                                                    }
                                                };
                                                arguments.push(value);
                                                h.call(&function, &receiver, &arguments)?
                                            };
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        let builder_fee_raw = {
                                            let receiver = h.global("Number")?;
                                            let function = h.get(&receiver, "parseFloat")?;
                                            let mut arguments = Vec::new();
                                            let value = {
                                                let function = h.global("String")?;
                                                let receiver = h.undefined();
                                                let mut arguments = Vec::new();
                                                let value = {
                                                    let left = {
                                                        'optional: {
                                                            let mut object = fill.clone();
                                                            if h.is_nullish(&object) {
                                                                break 'optional h.undefined();
                                                            }
                                                            object =
                                                                h.get(&object, "builderFee")?;
                                                            object
                                                        }
                                                    };
                                                    if !h.is_nullish(&left) {
                                                        left
                                                    } else {
                                                        lit(h, "")
                                                    }
                                                };
                                                arguments.push(value);
                                                h.call(&function, &receiver, &arguments)?
                                            };
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        let closed_pnl_raw = {
                                            let receiver = h.global("Number")?;
                                            let function = h.get(&receiver, "parseFloat")?;
                                            let mut arguments = Vec::new();
                                            let value = {
                                                let function = h.global("String")?;
                                                let receiver = h.undefined();
                                                let mut arguments = Vec::new();
                                                let value = {
                                                    let left = {
                                                        'optional: {
                                                            let mut object = fill.clone();
                                                            if h.is_nullish(&object) {
                                                                break 'optional h.undefined();
                                                            }
                                                            object = h.get(&object, "closedPnl")?;
                                                            object
                                                        }
                                                    };
                                                    if !h.is_nullish(&left) {
                                                        left
                                                    } else {
                                                        lit(h, "")
                                                    }
                                                };
                                                arguments.push(value);
                                                h.call(&function, &receiver, &arguments)?
                                            };
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        let execution = {
                                            let object = h.object()?;
                                            let value = {
                                                let condition = {
                                                    let left = {
                                                        let left = {
                                                            'optional: {
                                                                let mut object = fill.clone();
                                                                if h.is_nullish(&object) {
                                                                    break 'optional h.undefined();
                                                                }
                                                                object = h.get(&object, "tid")?;
                                                                object
                                                            }
                                                        };
                                                        let right = h.undefined();
                                                        h.boolean(!h.strict_equal(&left, &right))
                                                    };
                                                    if !h.truthy(&left) {
                                                        left
                                                    } else {
                                                        {
                                                            let left = {
                                                                'optional: {
                                                                    let mut object = fill.clone();
                                                                    if h.is_nullish(&object) {
                                                                        break 'optional h
                                                                            .undefined();
                                                                    }
                                                                    object =
                                                                        h.get(&object, "tid")?;
                                                                    object
                                                                }
                                                            };
                                                            let right = h.null_value();
                                                            h.boolean(
                                                                !h.strict_equal(&left, &right),
                                                            )
                                                        }
                                                    }
                                                };
                                                if h.truthy(&condition) {
                                                    {
                                                        let mut units: Vec<u16> = vec![];
                                                        let value = {
                                                            let object = fill.clone();
                                                            h.get(&object, "tid")?
                                                        };
                                                        units.extend(h.template_string(&value)?.0);
                                                        units.extend_from_slice(&[]);
                                                        h.literal(&JsString(units))
                                                    }
                                                } else {
                                                    h.undefined()
                                                }
                                            };
                                            h.define_data(&object, "id", &value)?;
                                            let value = {
                                                let receiver = h.global("Math")?;
                                                let function = h.get(&receiver, "floor")?;
                                                let mut arguments = Vec::new();
                                                let value = {
                                                    let left = timestamp_milliseconds.clone();
                                                    let right = h.number(1000.0);
                                                    h.position_operator("divide", &left, &right)?
                                                };
                                                arguments.push(value);
                                                h.call(&function, &receiver, &arguments)?
                                            };
                                            h.define_data(&object, "timestamp", &value)?;
                                            let value = symbol.clone();
                                            h.define_data(&object, "symbol", &value)?;
                                            let value = {
                                                let left = execution_order_id.clone();
                                                if h.truthy(&left) { left } else { h.undefined() }
                                            };
                                            h.define_data(&object, "orderId", &value)?;
                                            let value = price.clone();
                                            h.define_data(&object, "price", &value)?;
                                            let value = qty.clone();
                                            h.define_data(&object, "qty", &value)?;
                                            let value = side.clone();
                                            h.define_data(&object, "side", &value)?;
                                            let value = {
                                                let condition = {
                                                    let receiver = h.global("Number")?;
                                                    let function = h.get(&receiver, "isFinite")?;
                                                    let mut arguments = Vec::new();
                                                    let value = fee_raw.clone();
                                                    arguments.push(value);
                                                    h.call(&function, &receiver, &arguments)?
                                                };
                                                if h.truthy(&condition) {
                                                    {
                                                        let receiver = h.global("Math")?;
                                                        let function = h.get(&receiver, "abs")?;
                                                        let mut arguments = Vec::new();
                                                        let value = fee_raw.clone();
                                                        arguments.push(value);
                                                        h.call(&function, &receiver, &arguments)?
                                                    }
                                                } else {
                                                    h.undefined()
                                                }
                                            };
                                            h.define_data(&object, "commission", &value)?;
                                            let value = {
                                                let left = {
                                                    'optional: {
                                                        let mut object = fill.clone();
                                                        if h.is_nullish(&object) {
                                                            break 'optional h.undefined();
                                                        }
                                                        object = h.get(&object, "feeToken")?;
                                                        object
                                                    }
                                                };
                                                if h.truthy(&left) {
                                                    left
                                                } else {
                                                    lit(h, "USDC")
                                                }
                                            };
                                            h.define_data(&object, "commissionAsset", &value)?;
                                            let value = {
                                                let condition = {
                                                    let receiver = h.global("Number")?;
                                                    let function = h.get(&receiver, "isFinite")?;
                                                    let mut arguments = Vec::new();
                                                    let value = closed_pnl_raw.clone();
                                                    arguments.push(value);
                                                    h.call(&function, &receiver, &arguments)?
                                                };
                                                if h.truthy(&condition) {
                                                    closed_pnl_raw.clone()
                                                } else {
                                                    h.undefined()
                                                }
                                            };
                                            h.define_data(&object, "realizedPnl", &value)?;
                                            let value = {
                                                let condition = {
                                                    let left = {
                                                        let value = {
                                                            'optional: {
                                                                let mut object = fill.clone();
                                                                if h.is_nullish(&object) {
                                                                    break 'optional h.undefined();
                                                                }
                                                                object =
                                                                    h.get(&object, "crossed")?;
                                                                object
                                                            }
                                                        };
                                                        lit(h, h.typeof_value(&value))
                                                    };
                                                    let right = lit(h, "boolean");
                                                    h.boolean(h.strict_equal(&left, &right))
                                                };
                                                if h.truthy(&condition) {
                                                    {
                                                        let value = {
                                                            let object = fill.clone();
                                                            h.get(&object, "crossed")?
                                                        };
                                                        h.boolean(!h.truthy(&value))
                                                    }
                                                } else {
                                                    h.undefined()
                                                }
                                            };
                                            h.define_data(&object, "maker", &value)?;
                                            let value = {
                                                let condition = {
                                                    let receiver = h.global("Number")?;
                                                    let function = h.get(&receiver, "isFinite")?;
                                                    let mut arguments = Vec::new();
                                                    let value = builder_fee_raw.clone();
                                                    arguments.push(value);
                                                    h.call(&function, &receiver, &arguments)?
                                                };
                                                if h.truthy(&condition) {
                                                    {
                                                        let receiver = h.global("Math")?;
                                                        let function = h.get(&receiver, "abs")?;
                                                        let mut arguments = Vec::new();
                                                        let value = builder_fee_raw.clone();
                                                        arguments.push(value);
                                                        h.call(&function, &receiver, &arguments)?
                                                    }
                                                } else {
                                                    h.undefined()
                                                }
                                            };
                                            h.define_data(&object, "venueReportedRevenue", &value)?;
                                            let value = {
                                                let condition = {
                                                    let receiver = h.global("Number")?;
                                                    let function = h.get(&receiver, "isFinite")?;
                                                    let mut arguments = Vec::new();
                                                    let value = builder_fee_raw.clone();
                                                    arguments.push(value);
                                                    h.call(&function, &receiver, &arguments)?
                                                };
                                                if h.truthy(&condition) {
                                                    {
                                                        let left = {
                                                            'optional: {
                                                                let mut object = fill.clone();
                                                                if h.is_nullish(&object) {
                                                                    break 'optional h.undefined();
                                                                }
                                                                object =
                                                                    h.get(&object, "feeToken")?;
                                                                object
                                                            }
                                                        };
                                                        if h.truthy(&left) {
                                                            left
                                                        } else {
                                                            lit(h, "USDC")
                                                        }
                                                    }
                                                } else {
                                                    h.undefined()
                                                }
                                            };
                                            h.define_data(
                                                &object,
                                                "venueReportedRevenueAsset",
                                                &value,
                                            )?;
                                            object
                                        };
                                        let key = {
                                            let mut units: Vec<u16> = vec![];
                                            let value = {
                                                let left = {
                                                    let object = execution.clone();
                                                    h.get(&object, "orderId")?
                                                };
                                                if h.truthy(&left) {
                                                    left
                                                } else {
                                                    lit(h, "no-order")
                                                }
                                            };
                                            units.extend(h.template_string(&value)?.0);
                                            units.extend_from_slice(&[58]);
                                            let value = {
                                                let object = execution.clone();
                                                h.get(&object, "timestamp")?
                                            };
                                            units.extend(h.template_string(&value)?.0);
                                            units.extend_from_slice(&[58]);
                                            let value = {
                                                let object = execution.clone();
                                                h.get(&object, "price")?
                                            };
                                            units.extend(h.template_string(&value)?.0);
                                            units.extend_from_slice(&[58]);
                                            let value = {
                                                let object = execution.clone();
                                                h.get(&object, "qty")?
                                            };
                                            units.extend(h.template_string(&value)?.0);
                                            units.extend_from_slice(&[58]);
                                            let value = {
                                                let object = execution.clone();
                                                h.get(&object, "side")?
                                            };
                                            units.extend(h.template_string(&value)?.0);
                                            units.extend_from_slice(&[]);
                                            h.literal(&JsString(units))
                                        };
                                        let _ = {
                                            let receiver = deduped.clone();
                                            let function = h.get(&receiver, "set")?;
                                            let mut arguments = Vec::new();
                                            let value = key.clone();
                                            arguments.push(value);
                                            let value = execution.clone();
                                            arguments.push(value);
                                            h.call(&function, &receiver, &arguments)?
                                        };
                                        Ok(RestFlow::Normal)
                                    }
                                    .await;
                                    match result {
                                        Err(error) => {
                                            h.close_after_throw(&iterator);
                                            return Err(error);
                                        }
                                        Ok(RestFlow::Return(value)) => {
                                            h.close_after_return(&iterator)?;
                                            return Ok(RestFlow::Return(value));
                                        }
                                        Ok(RestFlow::Break) => {
                                            h.close_after_return(&iterator)?;
                                            break;
                                        }
                                        Ok(RestFlow::Normal | RestFlow::Next) => {}
                                    }
                                }
                            }
                            if {
                                let condition = {
                                    let left = {
                                        let object = deduped.clone();
                                        h.get(&object, "size")?
                                    };
                                    let right = max_limit.clone();
                                    h.position_operator("greaterEqual", &left, &right)?
                                };
                                h.truthy(&condition)
                            } {
                                return Ok(RestFlow::Break);
                            }
                            Ok(RestFlow::Normal)
                        }
                        .await;
                        match result {
                            Err(error) => {
                                h.close_after_throw(&iterator);
                                return Err(error);
                            }
                            Ok(RestFlow::Return(value)) => {
                                h.close_after_return(&iterator)?;
                                return Ok(RestFlow::Return(value));
                            }
                            Ok(RestFlow::Break) => {
                                h.close_after_return(&iterator)?;
                                break;
                            }
                            Ok(RestFlow::Normal | RestFlow::Next) => {}
                        }
                    }
                }
                return Ok(RestFlow::Return({
                    let receiver = {
                        let receiver = {
                            let array = h.array()?;
                            let value = {
                                let receiver = deduped.clone();
                                let function = h.get(&receiver, "values")?;
                                let arguments = Vec::new();
                                h.call(&function, &receiver, &arguments)?
                            };
                            h.append_position_spread(&array, &value)?;
                            array
                        };
                        let function = h.get(&receiver, "sort")?;
                        let mut arguments = Vec::new();
                        let value = h.rest_callback(RestCallback::FetchHistoricalFills3([]))?;
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    let function = h.get(&receiver, "slice")?;
                    let mut arguments = Vec::new();
                    let value = h.number(0.0);
                    arguments.push(value);
                    let value = max_limit.clone();
                    arguments.push(value);
                    h.call(&function, &receiver, &arguments)?
                }));
            }
            .await;
            match outcome {
                Ok(RestFlow::Normal) => {}
                Ok(flow) => return Ok(flow),
                Err(error) => {
                    let error = h.error_value(&error);
                    let _ = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "log")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let mut units: Vec<u16> = vec![
                                69, 114, 114, 111, 114, 32, 102, 101, 116, 99, 104, 105, 110, 103,
                                32, 101, 120, 101, 99, 117, 116, 105, 111, 110, 115, 58, 32,
                            ];
                            let value = {
                                let object = error.clone();
                                h.get(&object, "message")?
                            };
                            units.extend(h.template_string(&value)?.0);
                            units.extend_from_slice(&[]);
                            h.literal(&JsString(units))
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.global("LogSeverity")?;
                            h.get(&object, "Error")?
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    return Ok(RestFlow::Return({
                        let array = h.array()?;
                        array
                    }));
                }
            }
        }
        Ok(RestFlow::Normal)
    }
    .await;
    match outcome? {
        RestFlow::Return(value) => Ok(value),
        RestFlow::Normal => Ok(h.undefined()),
        RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
    }
}
pub fn fetch_historical_fills<H: RestTaskHost>(
    host: &H,
    this_value: &H::Value,
    opts: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let this_value = this_value.clone();
    let opts = opts.clone();
    crate::helper_tasks::start(host, async move {
        fetch_historical_fills_body(&mut h, &this_value, &opts).await
    })
}
/// Source: hyperliquid.exchange.ts::fetchFillsForOrderIds
async fn fetch_fills_for_order_ids_body<H: RestReadHost>(
    h: &mut H,
    this_value: &H::Value,
    order_ids: &H::Value,
) -> Result<H::Value, H::Error> {
    let order_ids = order_ids.clone();
    let outcome: Result<RestFlow<H::Value>, H::Error> = async {
        let fills_by_order_id = {
            let function = h.global("Map")?;
            h.construct(&function, &[])?
        };
        if {
            let condition = {
                let left = {
                    let object = order_ids.clone();
                    h.get(&object, "length")?
                };
                let right = h.number(0.0);
                h.boolean(h.strict_equal(&left, &right))
            };
            h.truthy(&condition)
        } {
            return Ok(RestFlow::Return(fills_by_order_id.clone()));
        }
        {
            let outcome: Result<RestFlow<H::Value>, H::Error> = async {
                let binding = {
                    let value = {
                        let receiver = {
                            let object = this_value.clone();
                            h.get(&object, "xhr")?
                        };
                        let function = h.get(&receiver, "post")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let object = h.global("ENDPOINTS")?;
                            h.get(&object, "INFO")?
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.object()?;
                            let value = {
                                let object = h.global("INFO_TYPES")?;
                                h.get(&object, "USER_FILLS")?
                            };
                            h.define_data(&object, "type", &value)?;
                            let value = {
                                let object = {
                                    let object = this_value.clone();
                                    h.get(&object, "options")?
                                };
                                h.get(&object, "key")?
                            };
                            h.define_data(&object, "user", &value)?;
                            let value = h.boolean(true);
                            h.define_data(&object, "aggregateByTime", &value)?;
                            object
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.object()?;
                            let value = {
                                let object = h.object()?;
                                let value = lit(h, "application/json");
                                h.define_data(&object, "Content-Type", &value)?;
                                object
                            };
                            h.define_data(&object, "headers", &value)?;
                            object
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    h.await_value(value).await?
                };
                let data = {
                    let value = h.get(&binding, "data")?;
                    value
                };
                if {
                    let condition = {
                        let left = {
                            let value = data.clone();
                            h.boolean(!h.truthy(&value))
                        };
                        if h.truthy(&left) {
                            left
                        } else {
                            {
                                let value = {
                                    let receiver = h.global("Array")?;
                                    let function = h.get(&receiver, "isArray")?;
                                    let mut arguments = Vec::new();
                                    let value = data.clone();
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                                h.boolean(!h.truthy(&value))
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    {
                        let value = {
                            let function = h.global("Error")?;
                            let arg0 = lit(
                                h,
                                "Hyperliquid fill reconciliation returned an invalid response",
                            );
                            h.construct(&function, &[arg0])?
                        };
                        return Err(h.rest_exception(value));
                    }
                }
                {
                    let value = data.clone();
                    let iterator = h.iterator(&value)?;
                    while let Some(fill) = h.next(&iterator)? {
                        let result: Result<RestFlow<H::Value>, H::Error> = async {
                            let oid = {
                                let function = h.global("String")?;
                                let receiver = h.undefined();
                                let mut arguments = Vec::new();
                                let value = {
                                    let object = fill.clone();
                                    h.get(&object, "oid")?
                                };
                                arguments.push(value);
                                h.call(&function, &receiver, &arguments)?
                            };
                            if {
                                let condition = {
                                    let receiver = order_ids.clone();
                                    let function = h.get(&receiver, "includes")?;
                                    let mut arguments = Vec::new();
                                    let value = oid.clone();
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                                h.truthy(&condition)
                            } {
                                let amount = {
                                    let function = h.global("parseFloat")?;
                                    let receiver = h.undefined();
                                    let mut arguments = Vec::new();
                                    let value = {
                                        let left = {
                                            let object = fill.clone();
                                            h.get(&object, "sz")?
                                        };
                                        if h.truthy(&left) { left } else { lit(h, "0") }
                                    };
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                                let _ = {
                                    let receiver = fills_by_order_id.clone();
                                    let function = h.get(&receiver, "set")?;
                                    let mut arguments = Vec::new();
                                    let value = oid.clone();
                                    arguments.push(value);
                                    let value = {
                                        let left = {
                                            let left = {
                                                let receiver = fills_by_order_id.clone();
                                                let function = h.get(&receiver, "get")?;
                                                let mut arguments = Vec::new();
                                                let value = oid.clone();
                                                arguments.push(value);
                                                h.call(&function, &receiver, &arguments)?
                                            };
                                            if h.truthy(&left) { left } else { h.number(0.0) }
                                        };
                                        let right = amount.clone();
                                        h.position_operator("add", &left, &right)?
                                    };
                                    arguments.push(value);
                                    h.call(&function, &receiver, &arguments)?
                                };
                            }
                            Ok(RestFlow::Normal)
                        }
                        .await;
                        match result {
                            Err(error) => {
                                h.close_after_throw(&iterator);
                                return Err(error);
                            }
                            Ok(RestFlow::Return(value)) => {
                                h.close_after_return(&iterator)?;
                                return Ok(RestFlow::Return(value));
                            }
                            Ok(RestFlow::Break) => {
                                h.close_after_return(&iterator)?;
                                break;
                            }
                            Ok(RestFlow::Normal | RestFlow::Next) => {}
                        }
                    }
                }
                return Ok(RestFlow::Return(fills_by_order_id.clone()));
            }
            .await;
            match outcome {
                Ok(RestFlow::Normal) => {}
                Ok(flow) => return Ok(flow),
                Err(error) => {
                    let error = h.error_value(&error);
                    let _ = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "log")?;
                        let mut arguments = Vec::new();
                        let value = {
                            let mut units: Vec<u16> = vec![
                                69, 114, 114, 111, 114, 32, 102, 101, 116, 99, 104, 105, 110, 103,
                                32, 102, 105, 108, 108, 115, 32, 102, 111, 114, 32, 111, 114, 100,
                                101, 114, 32, 73, 68, 115, 58, 32,
                            ];
                            let value = {
                                let object = error.clone();
                                h.get(&object, "message")?
                            };
                            units.extend(h.template_string(&value)?.0);
                            units.extend_from_slice(&[]);
                            h.literal(&JsString(units))
                        };
                        arguments.push(value);
                        let value = {
                            let object = h.global("LogSeverity")?;
                            h.get(&object, "Warning")?
                        };
                        arguments.push(value);
                        h.call(&function, &receiver, &arguments)?
                    };
                    {
                        let value = error.clone();
                        return Err(h.rest_exception(value));
                    }
                }
            }
        }
        Ok(RestFlow::Normal)
    }
    .await;
    match outcome? {
        RestFlow::Return(value) => Ok(value),
        RestFlow::Normal => Ok(h.undefined()),
        RestFlow::Next | RestFlow::Break => unreachable!("loop flow escaped source body"),
    }
}
pub fn fetch_fills_for_order_ids<H: RestTaskHost>(
    host: &H,
    this_value: &H::Value,
    order_ids: &H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let mut h = host.clone();
    let this_value = this_value.clone();
    let order_ids = order_ids.clone();
    crate::helper_tasks::start(host, async move {
        fetch_fills_for_order_ids_body(&mut h, &this_value, &order_ids).await
    })
}
