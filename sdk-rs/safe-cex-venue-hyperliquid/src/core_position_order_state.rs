//! Position and order ingestion bodies, lowered statement-by-statement from TypeScript.
//! Virtual callees and ECMAScript primitives remain explicit venue-local host bindings.
use crate::{core_account_snapshots::CoreSnapshotHost, spot_json::JsString};
#[derive(Clone, Debug)]
pub enum PositionCallback<V> {
    HandleFills0([V; 0]),
    ApplyOptimisticCorePositionFromFill0([V; 1]),
    HandleOrderUpdates0([V; 0]),
    ApplyRealizedPnlToCurrentPosition0([V; 2]),
    GenerateFakePositionsFromMarkets0([V; 1]),
    FetchHip3Positions0([V; 2]),
    FetchHip3Positions1([V; 2]),
    FetchHip3Positions2([V; 0]),
    FetchHip3Positions3([V; 2]),
    FetchHip3Positions4([V; 2]),
    ParseOpenOrders0([V; 1]),
    ParseOpenOrders1([V; 2]),
    ParseOpenOrders2([V; 0]),
}
pub trait PositionOrderHost: CoreSnapshotHost {
    fn position_operator(
        &mut self,
        operator: &str,
        left: &Self::Value,
        right: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn position_unary(
        &mut self,
        operator: &str,
        value: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn position_callback(
        &mut self,
        callback: PositionCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn set_position_index(
        &mut self,
        object: &Self::Value,
        key: &Self::Value,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn append_position_spread(
        &mut self,
        array: &Self::Value,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
}
fn lit<H: PositionOrderHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
pub fn run_position_callback<H: PositionOrderHost>(
    h: &mut H,
    callback: &PositionCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    match callback {
        PositionCallback::HandleFills0(_) => {
            let a = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let b = args.get(1).cloned().unwrap_or_else(|| h.undefined());
            return Ok({
                let left = {
                    let left = {
                        let function = h.global("Number")?;
                        let receiver = h.undefined();
                        let arg0 = {
                            let left = {
                                let object = a.clone();
                                h.get(&object, "time")?
                            };
                            if h.truthy(&left) { left } else { h.number(0.0) }
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                    let right = {
                        let function = h.global("Number")?;
                        let receiver = h.undefined();
                        let arg0 = {
                            let left = {
                                let object = b.clone();
                                h.get(&object, "time")?
                            };
                            if h.truthy(&left) { left } else { h.number(0.0) }
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                    h.position_operator("subtract", &left, &right)?
                };
                if h.truthy(&left) {
                    left
                } else {
                    {
                        let left = {
                            let function = h.global("Number")?;
                            let receiver = h.undefined();
                            let arg0 = {
                                let left = {
                                    let object = a.clone();
                                    h.get(&object, "tid")?
                                };
                                if h.truthy(&left) { left } else { h.number(0.0) }
                            };
                            h.call(&function, &receiver, &[arg0])?
                        };
                        let right = {
                            let function = h.global("Number")?;
                            let receiver = h.undefined();
                            let arg0 = {
                                let left = {
                                    let object = b.clone();
                                    h.get(&object, "tid")?
                                };
                                if h.truthy(&left) { left } else { h.number(0.0) }
                            };
                            h.call(&function, &receiver, &[arg0])?
                        };
                        h.position_operator("subtract", &left, &right)?
                    }
                }
            });
        }
        PositionCallback::ApplyOptimisticCorePositionFromFill0(captures) => {
            let market = captures[0].clone();
            let position = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            return Ok({
                let left = {
                    let object = position.clone();
                    h.get(&object, "symbol")?
                };
                let right = {
                    let object = market.clone();
                    h.get(&object, "symbol")?
                };
                h.boolean(h.strict_equal(&left, &right))
            });
        }
        PositionCallback::HandleOrderUpdates0(_) => {
            let k = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            return Ok({
                let receiver = k.clone();
                let function = h.get(&receiver, "includes")?;
                let arg0 = lit(h, ":");
                h.call(&function, &receiver, &[arg0])?
            });
        }
        PositionCallback::ApplyRealizedPnlToCurrentPosition0(captures) => {
            let symbol = captures[0].clone();
            let side = captures[1].clone();
            let position = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            return Ok({
                let left = {
                    let left = {
                        let object = position.clone();
                        h.get(&object, "symbol")?
                    };
                    let right = symbol.clone();
                    h.boolean(h.strict_equal(&left, &right))
                };
                if !h.truthy(&left) {
                    left
                } else {
                    {
                        let left = {
                            let object = position.clone();
                            h.get(&object, "side")?
                        };
                        let right = side.clone();
                        h.boolean(h.strict_equal(&left, &right))
                    }
                }
            });
        }
        PositionCallback::GenerateFakePositionsFromMarkets0(captures) => {
            let this_value = captures[0].clone();
            let market = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            if {
                let condition = {
                    let object = market.clone();
                    h.get(&object, "isSpot")?
                };
                h.truthy(&condition)
            } {
                return Ok({
                    let array = h.array()?;
                    array
                });
            }
            let market_config = {
                let receiver = this_value.clone();
                let function = h.get(&receiver, "_getSymbolConfigurationFromAssetData")?;
                let arg0 = {
                    let object = market.clone();
                    h.get(&object, "id")?
                };
                h.call(&function, &receiver, &[arg0])?
            };
            return Ok({
                let condition = {
                    let left = {
                        let object = market_config.clone();
                        h.get(&object, "positionMode")?
                    };
                    let right = {
                        let object = h.global("PositionMode")?;
                        h.get(&object, "OneWay")?
                    };
                    h.boolean(h.strict_equal(&left, &right))
                };
                if h.truthy(&condition) {
                    {
                        let array = h.array()?;
                        let value = {
                            let object = h.object()?;
                            let value = {
                                let function = h.global("createPositionId")?;
                                let receiver = h.undefined();
                                let arg0 = {
                                    let object = market.clone();
                                    h.get(&object, "symbol")?
                                };
                                let arg1 = {
                                    let object = h.global("PositionSide")?;
                                    h.get(&object, "Long")?
                                };
                                let arg2 = {
                                    let object = h.global("PositionMode")?;
                                    h.get(&object, "OneWay")?
                                };
                                h.call(&function, &receiver, &[arg0, arg1, arg2])?
                            };
                            h.define_data(&object, "id", &value)?;
                            let value = {
                                let object = market.clone();
                                h.get(&object, "symbol")?
                            };
                            h.define_data(&object, "symbol", &value)?;
                            let value = {
                                let object = h.global("PositionSide")?;
                                h.get(&object, "Long")?
                            };
                            h.define_data(&object, "side", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "entryPrice", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "notional", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "unrealizedPnl", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "contracts", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "liquidationPrice", &value)?;
                            let value = market_config.clone();
                            h.spread_into(&object, &value)?;
                            object
                        };
                        h.array_define(&array, 0, &value)?;
                        array
                    }
                } else {
                    {
                        let array = h.array()?;
                        let value = {
                            let object = h.object()?;
                            let value = {
                                let function = h.global("createPositionId")?;
                                let receiver = h.undefined();
                                let arg0 = {
                                    let object = market.clone();
                                    h.get(&object, "symbol")?
                                };
                                let arg1 = {
                                    let object = h.global("PositionSide")?;
                                    h.get(&object, "Long")?
                                };
                                let arg2 = {
                                    let object = h.global("PositionMode")?;
                                    h.get(&object, "Hedged")?
                                };
                                h.call(&function, &receiver, &[arg0, arg1, arg2])?
                            };
                            h.define_data(&object, "id", &value)?;
                            let value = {
                                let object = market.clone();
                                h.get(&object, "symbol")?
                            };
                            h.define_data(&object, "symbol", &value)?;
                            let value = {
                                let object = h.global("PositionSide")?;
                                h.get(&object, "Long")?
                            };
                            h.define_data(&object, "side", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "entryPrice", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "notional", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "unrealizedPnl", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "contracts", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "liquidationPrice", &value)?;
                            let value = market_config.clone();
                            h.spread_into(&object, &value)?;
                            object
                        };
                        h.array_define(&array, 0, &value)?;
                        let value = {
                            let object = h.object()?;
                            let value = {
                                let function = h.global("createPositionId")?;
                                let receiver = h.undefined();
                                let arg0 = {
                                    let object = market.clone();
                                    h.get(&object, "symbol")?
                                };
                                let arg1 = {
                                    let object = h.global("PositionSide")?;
                                    h.get(&object, "Short")?
                                };
                                let arg2 = {
                                    let object = h.global("PositionMode")?;
                                    h.get(&object, "Hedged")?
                                };
                                h.call(&function, &receiver, &[arg0, arg1, arg2])?
                            };
                            h.define_data(&object, "id", &value)?;
                            let value = {
                                let object = market.clone();
                                h.get(&object, "symbol")?
                            };
                            h.define_data(&object, "symbol", &value)?;
                            let value = {
                                let object = h.global("PositionSide")?;
                                h.get(&object, "Short")?
                            };
                            h.define_data(&object, "side", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "entryPrice", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "notional", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "unrealizedPnl", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "contracts", &value)?;
                            let value = h.number(0.0);
                            h.define_data(&object, "liquidationPrice", &value)?;
                            let value = market_config.clone();
                            h.spread_into(&object, &value)?;
                            object
                        };
                        h.array_define(&array, 1, &value)?;
                        array
                    }
                }
            });
        }
        PositionCallback::FetchHip3Positions0(captures) => {
            let this_value = captures[0].clone();
            let dex_name = captures[1].clone();
            let p = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let market = {
                let object = {
                    let object = {
                        let object = this_value.clone();
                        h.get(&object, "store")?
                    };
                    h.get(&object, "marketsBySymbol")?
                };
                {
                    let key = {
                        let object = p.clone();
                        h.get(&object, "symbol")?
                    };
                    h.index(&object, &key)?
                }
            };
            return Ok({
                let left = {
                    'optional: {
                        let mut object = market.clone();
                        if h.is_nullish(&object) {
                            break 'optional h.undefined();
                        }
                        object = h.get(&object, "info")?;
                        if h.is_nullish(&object) {
                            break 'optional h.undefined();
                        }
                        object = h.get(&object, "hip3Dex")?;
                        object
                    }
                };
                let right = dex_name.clone();
                h.boolean(h.strict_equal(&left, &right))
            });
        }
        PositionCallback::FetchHip3Positions1(captures) => {
            let this_value = captures[0].clone();
            let dex_name = captures[1].clone();
            let p = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let market = {
                let object = {
                    let object = {
                        let object = this_value.clone();
                        h.get(&object, "store")?
                    };
                    h.get(&object, "marketsBySymbol")?
                };
                {
                    let key = {
                        let object = p.clone();
                        h.get(&object, "symbol")?
                    };
                    h.index(&object, &key)?
                }
            };
            return Ok({
                let left = {
                    'optional: {
                        let mut object = market.clone();
                        if h.is_nullish(&object) {
                            break 'optional h.undefined();
                        }
                        object = h.get(&object, "info")?;
                        if h.is_nullish(&object) {
                            break 'optional h.undefined();
                        }
                        object = h.get(&object, "hip3Dex")?;
                        object
                    }
                };
                let right = dex_name.clone();
                h.boolean(!h.strict_equal(&left, &right))
            });
        }
        PositionCallback::FetchHip3Positions2(_) => {
            let k = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            return Ok({
                let receiver = k.clone();
                let function = h.get(&receiver, "includes")?;
                let arg0 = lit(h, ":");
                h.call(&function, &receiver, &[arg0])?
            });
        }
        PositionCallback::FetchHip3Positions3(captures) => {
            let this_value = captures[0].clone();
            let dex_name = captures[1].clone();
            let p = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let market = {
                let object = {
                    let object = {
                        let object = this_value.clone();
                        h.get(&object, "store")?
                    };
                    h.get(&object, "marketsBySymbol")?
                };
                {
                    let key = {
                        let object = p.clone();
                        h.get(&object, "symbol")?
                    };
                    h.index(&object, &key)?
                }
            };
            return Ok({
                let left = {
                    'optional: {
                        let mut object = market.clone();
                        if h.is_nullish(&object) {
                            break 'optional h.undefined();
                        }
                        object = h.get(&object, "info")?;
                        if h.is_nullish(&object) {
                            break 'optional h.undefined();
                        }
                        object = h.get(&object, "hip3Dex")?;
                        object
                    }
                };
                let right = dex_name.clone();
                h.boolean(h.strict_equal(&left, &right))
            });
        }
        PositionCallback::FetchHip3Positions4(captures) => {
            let this_value = captures[0].clone();
            let dex_name = captures[1].clone();
            let p = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let market = {
                let object = {
                    let object = {
                        let object = this_value.clone();
                        h.get(&object, "store")?
                    };
                    h.get(&object, "marketsBySymbol")?
                };
                {
                    let key = {
                        let object = p.clone();
                        h.get(&object, "symbol")?
                    };
                    h.index(&object, &key)?
                }
            };
            return Ok({
                let left = {
                    'optional: {
                        let mut object = market.clone();
                        if h.is_nullish(&object) {
                            break 'optional h.undefined();
                        }
                        object = h.get(&object, "info")?;
                        if h.is_nullish(&object) {
                            break 'optional h.undefined();
                        }
                        object = h.get(&object, "hip3Dex")?;
                        object
                    }
                };
                let right = dex_name.clone();
                h.boolean(!h.strict_equal(&left, &right))
            });
        }
        PositionCallback::ParseOpenOrders0(captures) => {
            let this_value = captures[0].clone();
            let open_order = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let oid = {
                let mut units: Vec<u16> = vec![];
                let value = {
                    let object = open_order.clone();
                    h.get(&object, "oid")?
                };
                units.extend(h.template_string(&value)?.0);
                units.extend_from_slice(&[]);
                h.literal(&JsString(units))
            };
            if {
                let condition = {
                    let receiver = {
                        let object = this_value.clone();
                        h.get(&object, "_deadOrders")?
                    };
                    let function = h.get(&receiver, "has")?;
                    let arg0 = oid.clone();
                    h.call(&function, &receiver, &[arg0])?
                };
                h.truthy(&condition)
            } {
                return Ok(h.boolean(false));
            }
            if {
                let condition = {
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "isOrderCancelled")?;
                    let arg0 = oid.clone();
                    h.call(&function, &receiver, &[arg0])?
                };
                h.truthy(&condition)
            } {
                return Ok(h.boolean(false));
            }
            return Ok(h.boolean(true));
        }
        PositionCallback::ParseOpenOrders1(captures) => {
            let markets_by_id = captures[0].clone();
            let this_value = captures[1].clone();
            let open_order = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            let mut market = {
                let object = markets_by_id.clone();
                {
                    let key = {
                        let object = open_order.clone();
                        h.get(&object, "coin")?
                    };
                    h.index(&object, &key)?
                }
            };
            if {
                let condition = {
                    let left = {
                        let value = market.clone();
                        h.boolean(!h.truthy(&value))
                    };
                    if !h.truthy(&left) {
                        left
                    } else {
                        {
                            let receiver = {
                                let object = open_order.clone();
                                h.get(&object, "coin")?
                            };
                            let function = h.get(&receiver, "includes")?;
                            let arg0 = lit(h, ":");
                            h.call(&function, &receiver, &[arg0])?
                        }
                    }
                };
                h.truthy(&condition)
            } {
                market = {
                    let receiver = {
                        let object = this_value.clone();
                        h.get(&object, "marketsLowerCaseMap")?
                    };
                    let function = h.get(&receiver, "get")?;
                    let arg0 = {
                        let receiver = {
                            let object = open_order.clone();
                            h.get(&object, "coin")?
                        };
                        let function = h.get(&receiver, "toLowerCase")?;
                        h.call(&function, &receiver, &[])?
                    };
                    h.call(&function, &receiver, &[arg0])?
                };
            }
            if {
                let condition = {
                    let value = market.clone();
                    h.boolean(!h.truthy(&value))
                };
                h.truthy(&condition)
            } {
                return Ok(h.null_value());
            }
            let inferred = {
                let function = h.global("inferHyperliquidOrderTypeAndPrice")?;
                let receiver = h.undefined();
                let arg0 = open_order.clone();
                h.call(&function, &receiver, &[arg0])?
            };
            return Ok({
                let object = h.object()?;
                let value = {
                    let mut units: Vec<u16> = vec![];
                    let value = {
                        let object = open_order.clone();
                        h.get(&object, "oid")?
                    };
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[]);
                    h.literal(&JsString(units))
                };
                h.define_data(&object, "id", &value)?;
                let value = {
                    let object = h.global("OrderStatus")?;
                    h.get(&object, "Open")?
                };
                h.define_data(&object, "status", &value)?;
                let value = {
                    let object = market.clone();
                    h.get(&object, "symbol")?
                };
                h.define_data(&object, "symbol", &value)?;
                let value = {
                    let object = inferred.clone();
                    h.get(&object, "type")?
                };
                h.define_data(&object, "type", &value)?;
                let value = {
                    let condition = {
                        let left = {
                            let object = open_order.clone();
                            h.get(&object, "side")?
                        };
                        let right = lit(h, "B");
                        h.boolean(h.strict_equal(&left, &right))
                    };
                    if h.truthy(&condition) {
                        {
                            let object = h.global("OrderSide")?;
                            h.get(&object, "Buy")?
                        }
                    } else {
                        {
                            let object = h.global("OrderSide")?;
                            h.get(&object, "Sell")?
                        }
                    }
                };
                h.define_data(&object, "side", &value)?;
                let value = {
                    let object = inferred.clone();
                    h.get(&object, "price")?
                };
                h.define_data(&object, "price", &value)?;
                let value = {
                    let value = {
                        let object = open_order.clone();
                        h.get(&object, "origSz")?
                    };
                    h.position_unary("plus", &value)?
                };
                h.define_data(&object, "amount", &value)?;
                let value = {
                    let left = {
                        let value = {
                            let object = open_order.clone();
                            h.get(&object, "origSz")?
                        };
                        h.position_unary("plus", &value)?
                    };
                    let right = {
                        let value = {
                            let object = open_order.clone();
                            h.get(&object, "sz")?
                        };
                        h.position_unary("plus", &value)?
                    };
                    h.position_operator("subtract", &left, &right)?
                };
                h.define_data(&object, "filled", &value)?;
                let value = {
                    let value = {
                        let object = open_order.clone();
                        h.get(&object, "sz")?
                    };
                    h.position_unary("plus", &value)?
                };
                h.define_data(&object, "remaining", &value)?;
                let value = {
                    let function = h.global("Boolean")?;
                    let receiver = h.undefined();
                    let arg0 = {
                        let object = open_order.clone();
                        h.get(&object, "reduceOnly")?
                    };
                    h.call(&function, &receiver, &[arg0])?
                };
                h.define_data(&object, "reduceOnly", &value)?;
                let value = {
                    let receiver = h.global("Date")?;
                    let function = h.get(&receiver, "now")?;
                    h.call(&function, &receiver, &[])?
                };
                h.define_data(&object, "lastUpdate", &value)?;
                let value = {
                    let function = h.global("parseHyperliquidOrderTimeInForce")?;
                    let receiver = h.undefined();
                    let arg0 = {
                        let object = open_order.clone();
                        h.get(&object, "tif")?
                    };
                    h.call(&function, &receiver, &[arg0])?
                };
                h.define_data(&object, "timeInForce", &value)?;
                let value = open_order.clone();
                h.define_data(&object, "info", &value)?;
                object
            });
        }
        PositionCallback::ParseOpenOrders2(_) => {
            let order = args.get(0).cloned().unwrap_or_else(|| h.undefined());
            return Ok({
                let left = order.clone();
                let right = h.null_value();
                h.boolean(!h.strict_equal(&left, &right))
            });
        }
    }
}
/// Source: hyperliquid.ws-private.ts::applyCoreClearinghouseState
pub fn apply_core_clearinghouse_state<H: PositionOrderHost>(
    h: &mut H,
    this_value: &H::Value,
    data: &H::Value,
    authoritative: &H::Value,
) -> Result<H::Value, H::Error> {
    let data = data.clone();
    let authoritative = authoritative.clone();
    let authoritative = if h.strict_equal(&authoritative, &h.undefined()) {
        h.boolean(false)
    } else {
        authoritative
    };
    let server_time = {
        let receiver = this_value.clone();
        let function = h.get(&receiver, "getMessageTimestamp")?;
        let arg0 = data.clone();
        h.call(&function, &receiver, &[arg0])?
    };
    if {
        let condition = {
            let left = server_time.clone();
            let right = h.null_value();
            h.boolean(!h.strict_equal(&left, &right))
        };
        h.truthy(&condition)
    } {
        if {
            let condition = {
                let left = server_time.clone();
                let right = {
                    let object = this_value.clone();
                    h.get(&object, "lastCoreClearinghouseStateServerTime")?
                };
                h.position_operator("less", &left, &right)?
            };
            h.truthy(&condition)
        } {
            return Ok(h.undefined());
        }
        {
            let object = this_value.clone();
            let value = server_time.clone();
            h.set(&object, "lastCoreClearinghouseStateServerTime", &value)?;
        }
    }
    if {
        let condition = {
            'optional: {
                let mut object = data.clone();
                if h.is_nullish(&object) {
                    break 'optional h.undefined();
                }
                object = h.get(&object, "assetPositions")?;
                object
            }
        };
        h.truthy(&condition)
    } {
        {
            let object = this_value.clone();
            let value = {
                let receiver = h.global("Date")?;
                let function = h.get(&receiver, "now")?;
                h.call(&function, &receiver, &[])?
            };
            h.set(&object, "lastCoreClearinghouseStateAt", &value)?;
        }
        let markets_by_id = {
            let object = {
                let object = {
                    let object = this_value.clone();
                    h.get(&object, "parent")?
                };
                h.get(&object, "store")?
            };
            h.get(&object, "marketsById")?
        };
        let asset_positions = {
            let object = data.clone();
            h.get(&object, "assetPositions")?
        };
        let core_positions = {
            let array = h.array()?;
            array
        };
        {
            let mut i: usize = 0;
            while {
                let left = h.number(i as f64);
                let right = {
                    let object = asset_positions.clone();
                    h.get(&object, "length")?
                };
                let condition = h.position_operator("less", &left, &right)?;
                h.truthy(&condition)
            } {
                let step = (|| -> Result<(), H::Error> {
                    let asset_position = {
                        let object = asset_positions.clone();
                        {
                            let key = h.number(i as f64);
                            h.index(&object, &key)?
                        }
                    };
                    let market = {
                        let object = markets_by_id.clone();
                        {
                            let key = {
                                let object = {
                                    let object = asset_position.clone();
                                    h.get(&object, "position")?
                                };
                                h.get(&object, "coin")?
                            };
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
                        return Ok(());
                    }
                    let position_mode = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "resolvePositionMode")?;
                        let arg0 = asset_position.clone();
                        h.call(&function, &receiver, &[arg0])?
                    };
                    let side = {
                        let condition = {
                            let left = {
                                let value = {
                                    let object = {
                                        let object = asset_position.clone();
                                        h.get(&object, "position")?
                                    };
                                    h.get(&object, "szi")?
                                };
                                h.position_unary("plus", &value)?
                            };
                            let right = h.number(0.0);
                            h.position_operator("greater", &left, &right)?
                        };
                        if h.truthy(&condition) {
                            {
                                let object = h.global("PositionSide")?;
                                h.get(&object, "Long")?
                            }
                        } else {
                            {
                                let object = h.global("PositionSide")?;
                                h.get(&object, "Short")?
                            }
                        }
                    };
                    let realized_pnl = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "getRealizedPnlForPosition")?;
                        let arg0 = {
                            let object = market.clone();
                            h.get(&object, "symbol")?
                        };
                        let arg1 = side.clone();
                        h.call(&function, &receiver, &[arg0, arg1])?
                    };
                    let _ = {
                        let receiver = core_positions.clone();
                        let function = h.get(&receiver, "push")?;
                        let arg0 = {
                            let object = h.object()?;
                            let value = {
                                let receiver = {
                                    let object = this_value.clone();
                                    h.get(&object, "parent")?
                                };
                                let function =
                                    h.get(&receiver, "_getSymbolConfigurationFromAssetData")?;
                                let arg0 = {
                                    let object = market.clone();
                                    h.get(&object, "id")?
                                };
                                h.call(&function, &receiver, &[arg0])?
                            };
                            h.spread_into(&object, &value)?;
                            let value = {
                                let function = h.global("createPositionId")?;
                                let receiver = h.undefined();
                                let arg0 = {
                                    let object = market.clone();
                                    h.get(&object, "symbol")?
                                };
                                let arg1 = side.clone();
                                let arg2 = position_mode.clone();
                                h.call(&function, &receiver, &[arg0, arg1, arg2])?
                            };
                            h.define_data(&object, "id", &value)?;
                            let value = {
                                let object = market.clone();
                                h.get(&object, "symbol")?
                            };
                            h.define_data(&object, "symbol", &value)?;
                            let value = side.clone();
                            h.define_data(&object, "side", &value)?;
                            let value = {
                                let value = {
                                    let object = {
                                        let object = asset_position.clone();
                                        h.get(&object, "position")?
                                    };
                                    h.get(&object, "entryPx")?
                                };
                                h.position_unary("plus", &value)?
                            };
                            h.define_data(&object, "entryPrice", &value)?;
                            let value = {
                                let value = {
                                    let object = {
                                        let object = asset_position.clone();
                                        h.get(&object, "position")?
                                    };
                                    h.get(&object, "positionValue")?
                                };
                                h.position_unary("plus", &value)?
                            };
                            h.define_data(&object, "notional", &value)?;
                            let value = {
                                let value = {
                                    let object = {
                                        let object = {
                                            let object = asset_position.clone();
                                            h.get(&object, "position")?
                                        };
                                        h.get(&object, "leverage")?
                                    };
                                    h.get(&object, "value")?
                                };
                                h.position_unary("plus", &value)?
                            };
                            h.define_data(&object, "leverage", &value)?;
                            let value = {
                                let value = {
                                    let object = {
                                        let object = asset_position.clone();
                                        h.get(&object, "position")?
                                    };
                                    h.get(&object, "unrealizedPnl")?
                                };
                                h.position_unary("plus", &value)?
                            };
                            h.define_data(&object, "unrealizedPnl", &value)?;
                            let value = {
                                let condition = {
                                    let left = realized_pnl.clone();
                                    let right = h.undefined();
                                    h.boolean(!h.strict_equal(&left, &right))
                                };
                                if h.truthy(&condition) {
                                    {
                                        let object = h.object()?;
                                        let value = realized_pnl.clone();
                                        h.define_data(&object, "realizedPnl", &value)?;
                                        object
                                    }
                                } else {
                                    {
                                        let object = h.object()?;
                                        object
                                    }
                                }
                            };
                            h.spread_into(&object, &value)?;
                            let value = {
                                let receiver = h.global("Math")?;
                                let function = h.get(&receiver, "abs")?;
                                let arg0 = {
                                    let value = {
                                        let object = {
                                            let object = asset_position.clone();
                                            h.get(&object, "position")?
                                        };
                                        h.get(&object, "szi")?
                                    };
                                    h.position_unary("plus", &value)?
                                };
                                h.call(&function, &receiver, &[arg0])?
                            };
                            h.define_data(&object, "contracts", &value)?;
                            let value = {
                                let value = {
                                    let object = {
                                        let object = asset_position.clone();
                                        h.get(&object, "position")?
                                    };
                                    h.get(&object, "liquidationPx")?
                                };
                                h.position_unary("plus", &value)?
                            };
                            h.define_data(&object, "liquidationPrice", &value)?;
                            let value = {
                                let condition = {
                                    let left = {
                                        let object = {
                                            let object = asset_position.clone();
                                            h.get(&object, "position")?
                                        };
                                        h.get(&object, "marginUsed")?
                                    };
                                    h.boolean(!h.is_nullish(&left))
                                };
                                if h.truthy(&condition) {
                                    {
                                        let value = {
                                            let object = {
                                                let object = asset_position.clone();
                                                h.get(&object, "position")?
                                            };
                                            h.get(&object, "marginUsed")?
                                        };
                                        h.position_unary("plus", &value)?
                                    }
                                } else {
                                    h.undefined()
                                }
                            };
                            h.define_data(&object, "marginSize", &value)?;
                            let value = position_mode.clone();
                            h.define_data(&object, "positionMode", &value)?;
                            let value = {
                                let condition = {
                                    let left = {
                                        let object = {
                                            let object = {
                                                let object = asset_position.clone();
                                                h.get(&object, "position")?
                                            };
                                            h.get(&object, "leverage")?
                                        };
                                        h.get(&object, "type")?
                                    };
                                    let right = lit(h, "cross");
                                    h.boolean(h.strict_equal(&left, &right))
                                };
                                if h.truthy(&condition) {
                                    {
                                        let object = h.global("MarginType")?;
                                        h.get(&object, "Cross")?
                                    }
                                } else {
                                    {
                                        let object = h.global("MarginType")?;
                                        h.get(&object, "Isolated")?
                                    }
                                }
                            };
                            h.define_data(&object, "marginType", &value)?;
                            object
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                    Ok(())
                })();
                step?;
                i += 1;
            }
        }
        if {
            let condition = authoritative.clone();
            h.truthy(&condition)
        } {
            let merged = {
                let function = h.global("Map")?;
                h.construct(&function, &[])?
            };
            let fakes = {
                let receiver = {
                    let object = this_value.clone();
                    h.get(&object, "parent")?
                };
                let function = h.get(&receiver, "getFakePositions")?;
                h.call(&function, &receiver, &[])?
            };
            {
                let mut i: usize = 0;
                while {
                    let left = h.number(i as f64);
                    let right = {
                        let object = fakes.clone();
                        h.get(&object, "length")?
                    };
                    let condition = h.position_operator("less", &left, &right)?;
                    h.truthy(&condition)
                } {
                    let step = (|| -> Result<(), H::Error> {
                        let _ = {
                            let receiver = merged.clone();
                            let function = h.get(&receiver, "set")?;
                            let arg0 = {
                                let object = {
                                    let object = fakes.clone();
                                    {
                                        let key = h.number(i as f64);
                                        h.index(&object, &key)?
                                    }
                                };
                                h.get(&object, "id")?
                            };
                            let arg1 = {
                                let object = fakes.clone();
                                {
                                    let key = h.number(i as f64);
                                    h.index(&object, &key)?
                                }
                            };
                            h.call(&function, &receiver, &[arg0, arg1])?
                        };
                        Ok(())
                    })();
                    step?;
                    i += 1;
                }
            }
            let current_positions = {
                let object = {
                    let object = {
                        let object = this_value.clone();
                        h.get(&object, "parent")?
                    };
                    h.get(&object, "store")?
                };
                h.get(&object, "positions")?
            };
            {
                let mut i: usize = 0;
                while {
                    let left = h.number(i as f64);
                    let right = {
                        let object = current_positions.clone();
                        h.get(&object, "length")?
                    };
                    let condition = h.position_operator("less", &left, &right)?;
                    h.truthy(&condition)
                } {
                    let step = (|| -> Result<(), H::Error> {
                        let position = {
                            let object = current_positions.clone();
                            {
                                let key = h.number(i as f64);
                                h.index(&object, &key)?
                            }
                        };
                        let market = {
                            let object = {
                                let object = {
                                    let object = {
                                        let object = this_value.clone();
                                        h.get(&object, "parent")?
                                    };
                                    h.get(&object, "store")?
                                };
                                h.get(&object, "marketsBySymbol")?
                            };
                            {
                                let key = {
                                    let object = position.clone();
                                    h.get(&object, "symbol")?
                                };
                                h.index(&object, &key)?
                            }
                        };
                        if {
                            let condition = {
                                'optional: {
                                    let mut object = market.clone();
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "info")?;
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "hip3Dex")?;
                                    object
                                }
                            };
                            h.truthy(&condition)
                        } {
                            let _ = {
                                let receiver = merged.clone();
                                let function = h.get(&receiver, "set")?;
                                let arg0 = {
                                    let object = position.clone();
                                    h.get(&object, "id")?
                                };
                                let arg1 = position.clone();
                                h.call(&function, &receiver, &[arg0, arg1])?
                            };
                        }
                        Ok(())
                    })();
                    step?;
                    i += 1;
                }
            }
            {
                let mut i: usize = 0;
                while {
                    let left = h.number(i as f64);
                    let right = {
                        let object = core_positions.clone();
                        h.get(&object, "length")?
                    };
                    let condition = h.position_operator("less", &left, &right)?;
                    h.truthy(&condition)
                } {
                    let step = (|| -> Result<(), H::Error> {
                        let _ = {
                            let receiver = merged.clone();
                            let function = h.get(&receiver, "set")?;
                            let arg0 = {
                                let object = {
                                    let object = core_positions.clone();
                                    {
                                        let key = h.number(i as f64);
                                        h.index(&object, &key)?
                                    }
                                };
                                h.get(&object, "id")?
                            };
                            let arg1 = {
                                let object = core_positions.clone();
                                {
                                    let key = h.number(i as f64);
                                    h.index(&object, &key)?
                                }
                            };
                            h.call(&function, &receiver, &[arg0, arg1])?
                        };
                        Ok(())
                    })();
                    step?;
                    i += 1;
                }
            }
            let _ = {
                let receiver = {
                    let object = {
                        let object = this_value.clone();
                        h.get(&object, "parent")?
                    };
                    h.get(&object, "store")?
                };
                let function = h.get(&receiver, "addOrUpdatePositionsFull")?;
                let arg0 = {
                    let receiver = h.global("Array")?;
                    let function = h.get(&receiver, "from")?;
                    let arg0 = {
                        let receiver = merged.clone();
                        let function = h.get(&receiver, "values")?;
                        h.call(&function, &receiver, &[])?
                    };
                    h.call(&function, &receiver, &[arg0])?
                };
                h.call(&function, &receiver, &[arg0])?
            };
        } else {
            let _ = {
                let receiver = {
                    let object = {
                        let object = this_value.clone();
                        h.get(&object, "parent")?
                    };
                    h.get(&object, "store")?
                };
                let function = h.get(&receiver, "addOrUpdatePositions")?;
                let arg0 = core_positions.clone();
                h.call(&function, &receiver, &[arg0])?
            };
        }
    }
    let balance_summary = {
        let receiver = this_value.clone();
        let function = h.get(&receiver, "extractCoreBalanceSummary")?;
        let arg0 = data.clone();
        h.call(&function, &receiver, &[arg0])?
    };
    if {
        let condition = balance_summary.clone();
        h.truthy(&condition)
    } {
        let total = {
            let object = balance_summary.clone();
            h.get(&object, "accountValue")?
        };
        let used = {
            let object = balance_summary.clone();
            h.get(&object, "totalMarginUsed")?
        };
        let positions = {
            let object = {
                let object = this_value.clone();
                h.get(&object, "store")?
            };
            h.get(&object, "positions")?
        };
        let mut upnl = h.number(0.0);
        {
            let mut i: usize = 0;
            while {
                let left = h.number(i as f64);
                let right = {
                    let object = positions.clone();
                    h.get(&object, "length")?
                };
                let condition = h.position_operator("less", &left, &right)?;
                h.truthy(&condition)
            } {
                let step = (|| -> Result<(), H::Error> {
                    upnl = {
                        let left = upnl.clone();
                        let right = {
                            let object = {
                                let object = positions.clone();
                                {
                                    let key = h.number(i as f64);
                                    h.index(&object, &key)?
                                }
                            };
                            h.get(&object, "unrealizedPnl")?
                        };
                        h.position_operator("add", &left, &right)?
                    };
                    Ok(())
                })();
                step?;
                i += 1;
            }
        }
        let _ = {
            let receiver = {
                let object = this_value.clone();
                h.get(&object, "parent")?
            };
            let function = h.get(&receiver, "applyCorePerpsBalanceSnapshot")?;
            let arg0 = {
                let object = h.object()?;
                let value = total.clone();
                h.define_data(&object, "accountValue", &value)?;
                let value = used.clone();
                h.define_data(&object, "totalMarginUsed", &value)?;
                let value = upnl.clone();
                h.define_data(&object, "upnl", &value)?;
                object
            };
            h.call(&function, &receiver, &[arg0])?
        };
    }
    let _ = {
        let receiver = {
            let object = this_value.clone();
            h.get(&object, "parent")?
        };
        let function = h.get(&receiver, "syncBalanceFromLivePositions")?;
        h.call(&function, &receiver, &[])?
    };
    Ok(h.undefined())
}
/// Source: hyperliquid.ws-private.ts::handleHip3ClearinghouseState
pub fn handle_hip3_clearinghouse_state<H: PositionOrderHost>(
    h: &mut H,
    this_value: &H::Value,
    dex: &H::Value,
    data: &H::Value,
) -> Result<H::Value, H::Error> {
    let dex = dex.clone();
    let data = data.clone();
    let message_dex = {
        let receiver = {
            let function = h.global("String")?;
            let receiver = h.undefined();
            let arg0 = {
                let left = dex.clone();
                if h.truthy(&left) { left } else { lit(h, "") }
            };
            h.call(&function, &receiver, &[arg0])?
        };
        let function = h.get(&receiver, "toLowerCase")?;
        h.call(&function, &receiver, &[])?
    };
    if {
        let condition = {
            let left = {
                let value = message_dex.clone();
                h.boolean(!h.truthy(&value))
            };
            if h.truthy(&left) {
                left
            } else {
                {
                    let value = {
                        let receiver = h.global("Array")?;
                        let function = h.get(&receiver, "isArray")?;
                        let arg0 = {
                            'optional: {
                                let mut object = data.clone();
                                if h.is_nullish(&object) {
                                    break 'optional h.undefined();
                                }
                                object = h.get(&object, "assetPositions")?;
                                object
                            }
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                    h.boolean(!h.truthy(&value))
                }
            }
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    let markets_by_id = {
        let object = {
            let object = {
                let object = this_value.clone();
                h.get(&object, "parent")?
            };
            h.get(&object, "store")?
        };
        h.get(&object, "marketsById")?
    };
    let incoming_positions = {
        let array = h.array()?;
        array
    };
    {
        let mut i: usize = 0;
        while {
            let left = h.number(i as f64);
            let right = {
                let object = {
                    let object = data.clone();
                    h.get(&object, "assetPositions")?
                };
                h.get(&object, "length")?
            };
            let condition = h.position_operator("less", &left, &right)?;
            h.truthy(&condition)
        } {
            let step = (|| -> Result<(), H::Error> {
                let asset_position = {
                    let object = {
                        let object = data.clone();
                        h.get(&object, "assetPositions")?
                    };
                    {
                        let key = h.number(i as f64);
                        h.index(&object, &key)?
                    }
                };
                let coin = {
                    let function = h.global("String")?;
                    let receiver = h.undefined();
                    let arg0 = {
                        let left = {
                            'optional: {
                                let mut object = asset_position.clone();
                                if h.is_nullish(&object) {
                                    break 'optional h.undefined();
                                }
                                object = h.get(&object, "position")?;
                                if h.is_nullish(&object) {
                                    break 'optional h.undefined();
                                }
                                object = h.get(&object, "coin")?;
                                object
                            }
                        };
                        if h.truthy(&left) { left } else { lit(h, "") }
                    };
                    h.call(&function, &receiver, &[arg0])?
                };
                let mut market = {
                    let object = markets_by_id.clone();
                    {
                        let key = coin.clone();
                        h.index(&object, &key)?
                    }
                };
                if {
                    let condition = {
                        let left = {
                            let value = market.clone();
                            h.boolean(!h.truthy(&value))
                        };
                        if !h.truthy(&left) {
                            left
                        } else {
                            {
                                let receiver = coin.clone();
                                let function = h.get(&receiver, "includes")?;
                                let arg0 = lit(h, ":");
                                h.call(&function, &receiver, &[arg0])?
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    market = {
                        let receiver = {
                            let object = {
                                let object = this_value.clone();
                                h.get(&object, "parent")?
                            };
                            h.get(&object, "marketsLowerCaseMap")?
                        };
                        let function = h.get(&receiver, "get")?;
                        let arg0 = {
                            let receiver = coin.clone();
                            let function = h.get(&receiver, "toLowerCase")?;
                            h.call(&function, &receiver, &[])?
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                }
                if {
                    let condition = {
                        let value = market.clone();
                        h.boolean(!h.truthy(&value))
                    };
                    h.truthy(&condition)
                } {
                    return Ok(());
                }
                let position_mode = {
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "resolvePositionMode")?;
                    let arg0 = asset_position.clone();
                    h.call(&function, &receiver, &[arg0])?
                };
                let side = {
                    let condition = {
                        let left = {
                            let value = {
                                let object = {
                                    let object = asset_position.clone();
                                    h.get(&object, "position")?
                                };
                                h.get(&object, "szi")?
                            };
                            h.position_unary("plus", &value)?
                        };
                        let right = h.number(0.0);
                        h.position_operator("greater", &left, &right)?
                    };
                    if h.truthy(&condition) {
                        {
                            let object = h.global("PositionSide")?;
                            h.get(&object, "Long")?
                        }
                    } else {
                        {
                            let object = h.global("PositionSide")?;
                            h.get(&object, "Short")?
                        }
                    }
                };
                let realized_pnl = {
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "getRealizedPnlForPosition")?;
                    let arg0 = {
                        let object = market.clone();
                        h.get(&object, "symbol")?
                    };
                    let arg1 = side.clone();
                    h.call(&function, &receiver, &[arg0, arg1])?
                };
                let _ = {
                    let receiver = incoming_positions.clone();
                    let function = h.get(&receiver, "push")?;
                    let arg0 = {
                        let object = h.object()?;
                        let value = {
                            let receiver = {
                                let object = this_value.clone();
                                h.get(&object, "parent")?
                            };
                            let function =
                                h.get(&receiver, "_getSymbolConfigurationFromAssetData")?;
                            let arg0 = {
                                let object = market.clone();
                                h.get(&object, "id")?
                            };
                            h.call(&function, &receiver, &[arg0])?
                        };
                        h.spread_into(&object, &value)?;
                        let value = {
                            let function = h.global("createPositionId")?;
                            let receiver = h.undefined();
                            let arg0 = {
                                let object = market.clone();
                                h.get(&object, "symbol")?
                            };
                            let arg1 = side.clone();
                            let arg2 = position_mode.clone();
                            h.call(&function, &receiver, &[arg0, arg1, arg2])?
                        };
                        h.define_data(&object, "id", &value)?;
                        let value = {
                            let object = market.clone();
                            h.get(&object, "symbol")?
                        };
                        h.define_data(&object, "symbol", &value)?;
                        let value = side.clone();
                        h.define_data(&object, "side", &value)?;
                        let value = {
                            let value = {
                                let object = {
                                    let object = asset_position.clone();
                                    h.get(&object, "position")?
                                };
                                h.get(&object, "entryPx")?
                            };
                            h.position_unary("plus", &value)?
                        };
                        h.define_data(&object, "entryPrice", &value)?;
                        let value = {
                            let value = {
                                let object = {
                                    let object = asset_position.clone();
                                    h.get(&object, "position")?
                                };
                                h.get(&object, "positionValue")?
                            };
                            h.position_unary("plus", &value)?
                        };
                        h.define_data(&object, "notional", &value)?;
                        let value = {
                            let value = {
                                let object = {
                                    let object = {
                                        let object = asset_position.clone();
                                        h.get(&object, "position")?
                                    };
                                    h.get(&object, "leverage")?
                                };
                                h.get(&object, "value")?
                            };
                            h.position_unary("plus", &value)?
                        };
                        h.define_data(&object, "leverage", &value)?;
                        let value = {
                            let value = {
                                let object = {
                                    let object = asset_position.clone();
                                    h.get(&object, "position")?
                                };
                                h.get(&object, "unrealizedPnl")?
                            };
                            h.position_unary("plus", &value)?
                        };
                        h.define_data(&object, "unrealizedPnl", &value)?;
                        let value = {
                            let condition = {
                                let left = realized_pnl.clone();
                                let right = h.undefined();
                                h.boolean(!h.strict_equal(&left, &right))
                            };
                            if h.truthy(&condition) {
                                {
                                    let object = h.object()?;
                                    let value = realized_pnl.clone();
                                    h.define_data(&object, "realizedPnl", &value)?;
                                    object
                                }
                            } else {
                                {
                                    let object = h.object()?;
                                    object
                                }
                            }
                        };
                        h.spread_into(&object, &value)?;
                        let value = {
                            let receiver = h.global("Math")?;
                            let function = h.get(&receiver, "abs")?;
                            let arg0 = {
                                let value = {
                                    let object = {
                                        let object = asset_position.clone();
                                        h.get(&object, "position")?
                                    };
                                    h.get(&object, "szi")?
                                };
                                h.position_unary("plus", &value)?
                            };
                            h.call(&function, &receiver, &[arg0])?
                        };
                        h.define_data(&object, "contracts", &value)?;
                        let value = {
                            let value = {
                                let object = {
                                    let object = asset_position.clone();
                                    h.get(&object, "position")?
                                };
                                h.get(&object, "liquidationPx")?
                            };
                            h.position_unary("plus", &value)?
                        };
                        h.define_data(&object, "liquidationPrice", &value)?;
                        let value = {
                            let condition = {
                                let left = {
                                    let object = {
                                        let object = asset_position.clone();
                                        h.get(&object, "position")?
                                    };
                                    h.get(&object, "marginUsed")?
                                };
                                h.boolean(!h.is_nullish(&left))
                            };
                            if h.truthy(&condition) {
                                {
                                    let value = {
                                        let object = {
                                            let object = asset_position.clone();
                                            h.get(&object, "position")?
                                        };
                                        h.get(&object, "marginUsed")?
                                    };
                                    h.position_unary("plus", &value)?
                                }
                            } else {
                                h.undefined()
                            }
                        };
                        h.define_data(&object, "marginSize", &value)?;
                        let value = position_mode.clone();
                        h.define_data(&object, "positionMode", &value)?;
                        let value = {
                            let condition = {
                                let left = {
                                    let object = {
                                        let object = {
                                            let object = asset_position.clone();
                                            h.get(&object, "position")?
                                        };
                                        h.get(&object, "leverage")?
                                    };
                                    h.get(&object, "type")?
                                };
                                let right = lit(h, "cross");
                                h.boolean(h.strict_equal(&left, &right))
                            };
                            if h.truthy(&condition) {
                                {
                                    let object = h.global("MarginType")?;
                                    h.get(&object, "Cross")?
                                }
                            } else {
                                {
                                    let object = h.global("MarginType")?;
                                    h.get(&object, "Isolated")?
                                }
                            }
                        };
                        h.define_data(&object, "marginType", &value)?;
                        object
                    };
                    h.call(&function, &receiver, &[arg0])?
                };
                Ok(())
            })();
            step?;
            i += 1;
        }
    }
    let current_positions = {
        let object = {
            let object = {
                let object = this_value.clone();
                h.get(&object, "parent")?
            };
            h.get(&object, "store")?
        };
        h.get(&object, "positions")?
    };
    let fakes = {
        let receiver = {
            let object = this_value.clone();
            h.get(&object, "parent")?
        };
        let function = h.get(&receiver, "getFakePositions")?;
        h.call(&function, &receiver, &[])?
    };
    let merged = {
        let function = h.global("Map")?;
        h.construct(&function, &[])?
    };
    {
        let mut i: usize = 0;
        while {
            let left = h.number(i as f64);
            let right = {
                let object = fakes.clone();
                h.get(&object, "length")?
            };
            let condition = h.position_operator("less", &left, &right)?;
            h.truthy(&condition)
        } {
            let step = (|| -> Result<(), H::Error> {
                let _ = {
                    let receiver = merged.clone();
                    let function = h.get(&receiver, "set")?;
                    let arg0 = {
                        let object = {
                            let object = fakes.clone();
                            {
                                let key = h.number(i as f64);
                                h.index(&object, &key)?
                            }
                        };
                        h.get(&object, "id")?
                    };
                    let arg1 = {
                        let object = fakes.clone();
                        {
                            let key = h.number(i as f64);
                            h.index(&object, &key)?
                        }
                    };
                    h.call(&function, &receiver, &[arg0, arg1])?
                };
                Ok(())
            })();
            step?;
            i += 1;
        }
    }
    {
        let mut i: usize = 0;
        while {
            let left = h.number(i as f64);
            let right = {
                let object = current_positions.clone();
                h.get(&object, "length")?
            };
            let condition = h.position_operator("less", &left, &right)?;
            h.truthy(&condition)
        } {
            let step = (|| -> Result<(), H::Error> {
                let position = {
                    let object = current_positions.clone();
                    {
                        let key = h.number(i as f64);
                        h.index(&object, &key)?
                    }
                };
                let position_dex = {
                    'optional_call: {
                        let receiver = {
                            'optional: {
                                let mut object = this_value.clone();
                                object = h.get(&object, "parent")?;
                                object = h.get(&object, "store")?;
                                object = h.get(&object, "marketsBySymbol")?;
                                object = {
                                    let key = {
                                        let object = position.clone();
                                        h.get(&object, "symbol")?
                                    };
                                    h.index(&object, &key)?
                                };
                                if h.is_nullish(&object) {
                                    break 'optional h.undefined();
                                }
                                object = h.get(&object, "info")?;
                                if h.is_nullish(&object) {
                                    break 'optional h.undefined();
                                }
                                object = h.get(&object, "hip3Dex")?;
                                object
                            }
                        };
                        if h.is_nullish(&receiver) {
                            break 'optional_call h.undefined();
                        }
                        let function = h.get(&receiver, "toLowerCase")?;
                        h.call(&function, &receiver, &[])?
                    }
                };
                if {
                    let condition = {
                        let left = {
                            let value = position_dex.clone();
                            h.boolean(!h.truthy(&value))
                        };
                        if h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = position_dex.clone();
                                let right = message_dex.clone();
                                h.boolean(!h.strict_equal(&left, &right))
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    let _ = {
                        let receiver = merged.clone();
                        let function = h.get(&receiver, "set")?;
                        let arg0 = {
                            let object = position.clone();
                            h.get(&object, "id")?
                        };
                        let arg1 = position.clone();
                        h.call(&function, &receiver, &[arg0, arg1])?
                    };
                }
                Ok(())
            })();
            step?;
            i += 1;
        }
    }
    {
        let mut i: usize = 0;
        while {
            let left = h.number(i as f64);
            let right = {
                let object = incoming_positions.clone();
                h.get(&object, "length")?
            };
            let condition = h.position_operator("less", &left, &right)?;
            h.truthy(&condition)
        } {
            let step = (|| -> Result<(), H::Error> {
                let _ = {
                    let receiver = merged.clone();
                    let function = h.get(&receiver, "set")?;
                    let arg0 = {
                        let object = {
                            let object = incoming_positions.clone();
                            {
                                let key = h.number(i as f64);
                                h.index(&object, &key)?
                            }
                        };
                        h.get(&object, "id")?
                    };
                    let arg1 = {
                        let object = incoming_positions.clone();
                        {
                            let key = h.number(i as f64);
                            h.index(&object, &key)?
                        }
                    };
                    h.call(&function, &receiver, &[arg0, arg1])?
                };
                Ok(())
            })();
            step?;
            i += 1;
        }
    }
    let _ = {
        let receiver = {
            let object = {
                let object = this_value.clone();
                h.get(&object, "parent")?
            };
            h.get(&object, "store")?
        };
        let function = h.get(&receiver, "addOrUpdatePositionsFull")?;
        let arg0 = {
            let receiver = h.global("Array")?;
            let function = h.get(&receiver, "from")?;
            let arg0 = {
                let receiver = merged.clone();
                let function = h.get(&receiver, "values")?;
                h.call(&function, &receiver, &[])?
            };
            h.call(&function, &receiver, &[arg0])?
        };
        h.call(&function, &receiver, &[arg0])?
    };
    Ok(h.undefined())
}
/// Source: hyperliquid.ws-private.ts::handleFills
pub fn handle_fills<H: PositionOrderHost>(
    h: &mut H,
    this_value: &H::Value,
    data: &H::Value,
) -> Result<H::Value, H::Error> {
    let data = data.clone();
    let fills = {
        let left = {
            let object = data.clone();
            h.get(&object, "fills")?
        };
        if h.truthy(&left) {
            left
        } else {
            {
                let array = h.array()?;
                array
            }
        }
    };
    if {
        let condition = {
            let value = {
                let object = fills.clone();
                h.get(&object, "length")?
            };
            h.boolean(!h.truthy(&value))
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    let markets_by_id = {
        let object = {
            let object = {
                let object = this_value.clone();
                h.get(&object, "parent")?
            };
            h.get(&object, "store")?
        };
        h.get(&object, "marketsById")?
    };
    let sorted_fills = {
        let receiver = {
            let receiver = fills.clone();
            let function = h.get(&receiver, "slice")?;
            h.call(&function, &receiver, &[])?
        };
        let function = h.get(&receiver, "sort")?;
        let arg0 = h.position_callback(PositionCallback::HandleFills0([]))?;
        h.call(&function, &receiver, &[arg0])?
    };
    {
        let mut fill_idx: usize = 0;
        while {
            let left = h.number(fill_idx as f64);
            let right = {
                let object = sorted_fills.clone();
                h.get(&object, "length")?
            };
            let condition = h.position_operator("less", &left, &right)?;
            h.truthy(&condition)
        } {
            let step = (|| -> Result<(), H::Error> {
                let fill = {
                    let object = sorted_fills.clone();
                    {
                        let key = h.number(fill_idx as f64);
                        h.index(&object, &key)?
                    }
                };
                let fill_id = {
                    let mut units: Vec<u16> = vec![];
                    let value = {
                        let object = fill.clone();
                        h.get(&object, "tid")?
                    };
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[]);
                    h.literal(&JsString(units))
                };
                let fill_dir = {
                    let receiver = {
                        let function = h.global("String")?;
                        let receiver = h.undefined();
                        let arg0 = {
                            let left = {
                                let object = fill.clone();
                                h.get(&object, "dir")?
                            };
                            if h.truthy(&left) { left } else { lit(h, "") }
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                    let function = h.get(&receiver, "toLowerCase")?;
                    h.call(&function, &receiver, &[])?
                };
                let fill_position_side = {
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "getFillPositionSide")?;
                    let arg0 = fill_dir.clone();
                    h.call(&function, &receiver, &[arg0])?
                };
                let fee = {
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "safeParseNumber")?;
                    let arg0 = {
                        let object = fill.clone();
                        h.get(&object, "fee")?
                    };
                    h.call(&function, &receiver, &[arg0])?
                };
                let closed_pnl = {
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "safeParseNumber")?;
                    let arg0 = {
                        let object = fill.clone();
                        h.get(&object, "closedPnl")?
                    };
                    h.call(&function, &receiver, &[arg0])?
                };
                let net_realized_pnl = {
                    let left = closed_pnl.clone();
                    let right = fee.clone();
                    h.position_operator("subtract", &left, &right)?
                };
                if {
                    let condition = {
                        let value = {
                            let receiver = {
                                let object = this_value.clone();
                                h.get(&object, "processedRealizedPnlFillIds")?
                            };
                            let function = h.get(&receiver, "has")?;
                            let arg0 = fill_id.clone();
                            h.call(&function, &receiver, &[arg0])?
                        };
                        h.boolean(!h.truthy(&value))
                    };
                    h.truthy(&condition)
                } {
                    let _ = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "trackProcessedRealizedPnlFill")?;
                        let arg0 = fill_id.clone();
                        h.call(&function, &receiver, &[arg0])?
                    };
                    if {
                        let condition = {
                            let left = {
                                let left = fill_position_side.clone();
                                if !h.truthy(&left) {
                                    left
                                } else {
                                    {
                                        let receiver = fill_dir.clone();
                                        let function = h.get(&receiver, "includes")?;
                                        let arg0 = lit(h, "open");
                                        h.call(&function, &receiver, &[arg0])?
                                    }
                                }
                            };
                            if !h.truthy(&left) {
                                left
                            } else {
                                {
                                    let left = {
                                        let receiver = h.global("Math")?;
                                        let function = h.get(&receiver, "abs")?;
                                        let arg0 = {
                                            let receiver = this_value.clone();
                                            let function = h.get(&receiver, "safeParseNumber")?;
                                            let arg0 = {
                                                let object = fill.clone();
                                                h.get(&object, "startPosition")?
                                            };
                                            h.call(&function, &receiver, &[arg0])?
                                        };
                                        h.call(&function, &receiver, &[arg0])?
                                    };
                                    let right = h.number(0.0);
                                    h.boolean(h.strict_equal(&left, &right))
                                }
                            }
                        };
                        h.truthy(&condition)
                    } {
                        let _ = {
                            let receiver = this_value.clone();
                            let function = h.get(&receiver, "setRealizedPnlForPosition")?;
                            let arg0 = {
                                let object = fill.clone();
                                h.get(&object, "coin")?
                            };
                            let arg1 = fill_position_side.clone();
                            let arg2 = h.number(0.0);
                            let arg3 = markets_by_id.clone();
                            h.call(&function, &receiver, &[arg0, arg1, arg2, arg3])?
                        };
                    }
                    if {
                        let condition = {
                            let left = fill_position_side.clone();
                            if !h.truthy(&left) {
                                left
                            } else {
                                {
                                    let receiver = fill_dir.clone();
                                    let function = h.get(&receiver, "includes")?;
                                    let arg0 = lit(h, "close");
                                    h.call(&function, &receiver, &[arg0])?
                                }
                            }
                        };
                        h.truthy(&condition)
                    } {
                        let _ = {
                            let receiver = this_value.clone();
                            let function = h.get(&receiver, "incrementRealizedPnlForPosition")?;
                            let arg0 = {
                                let object = fill.clone();
                                h.get(&object, "coin")?
                            };
                            let arg1 = fill_position_side.clone();
                            let arg2 = net_realized_pnl.clone();
                            let arg3 = markets_by_id.clone();
                            h.call(&function, &receiver, &[arg0, arg1, arg2, arg3])?
                        };
                        let _ = {
                            let receiver = this_value.clone();
                            let function = h.get(&receiver, "applyRealizedPnlToCurrentPosition")?;
                            let arg0 = {
                                let object = fill.clone();
                                h.get(&object, "coin")?
                            };
                            let arg1 = fill_position_side.clone();
                            let arg2 = markets_by_id.clone();
                            h.call(&function, &receiver, &[arg0, arg1, arg2])?
                        };
                    }
                }
                if {
                    let condition = {
                        let object = data.clone();
                        h.get(&object, "isSnapshot")?
                    };
                    h.truthy(&condition)
                } {
                    return Ok(());
                }
                let _ = {
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "applyOptimisticCorePositionFromFill")?;
                    let arg0 = fill.clone();
                    let arg1 = markets_by_id.clone();
                    h.call(&function, &receiver, &[arg0, arg1])?
                };
                if {
                    let condition = {
                        let function = h.global("hasSeenFill")?;
                        let receiver = h.undefined();
                        let arg0 = fill_id.clone();
                        h.call(&function, &receiver, &[arg0])?
                    };
                    h.truthy(&condition)
                } {
                    return Ok(());
                }
                let _ = {
                    let function = h.global("markFillSeen")?;
                    let receiver = h.undefined();
                    let arg0 = fill_id.clone();
                    h.call(&function, &receiver, &[arg0])?
                };
                let mut market = {
                    let object = markets_by_id.clone();
                    {
                        let key = {
                            let object = fill.clone();
                            h.get(&object, "coin")?
                        };
                        h.index(&object, &key)?
                    }
                };
                if {
                    let condition = {
                        let left = {
                            let value = market.clone();
                            h.boolean(!h.truthy(&value))
                        };
                        if !h.truthy(&left) {
                            left
                        } else {
                            {
                                let receiver = {
                                    let object = fill.clone();
                                    h.get(&object, "coin")?
                                };
                                let function = h.get(&receiver, "includes")?;
                                let arg0 = lit(h, ":");
                                h.call(&function, &receiver, &[arg0])?
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    market = {
                        let receiver = {
                            let object = {
                                let object = this_value.clone();
                                h.get(&object, "parent")?
                            };
                            h.get(&object, "marketsLowerCaseMap")?
                        };
                        let function = h.get(&receiver, "get")?;
                        let arg0 = {
                            let receiver = {
                                let object = fill.clone();
                                h.get(&object, "coin")?
                            };
                            let function = h.get(&receiver, "toLowerCase")?;
                            h.call(&function, &receiver, &[])?
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                }
                let symbol = {
                    let left = {
                        'optional: {
                            let mut object = market.clone();
                            if h.is_nullish(&object) {
                                break 'optional h.undefined();
                            }
                            object = h.get(&object, "symbol")?;
                            object
                        }
                    };
                    if h.truthy(&left) {
                        left
                    } else {
                        {
                            let object = fill.clone();
                            h.get(&object, "coin")?
                        }
                    }
                };
                let _ = {
                    let receiver = {
                        let object = {
                            let object = this_value.clone();
                            h.get(&object, "parent")?
                        };
                        h.get(&object, "emitter")?
                    };
                    let function = h.get(&receiver, "emit")?;
                    let arg0 = lit(h, "fill");
                    let arg1 = {
                        let object = h.object()?;
                        let value = fill_id.clone();
                        h.define_data(&object, "id", &value)?;
                        let value = {
                            let mut units: Vec<u16> = vec![];
                            let value = {
                                let object = fill.clone();
                                h.get(&object, "oid")?
                            };
                            units.extend(h.template_string(&value)?.0);
                            units.extend_from_slice(&[]);
                            h.literal(&JsString(units))
                        };
                        h.define_data(&object, "orderId", &value)?;
                        let value = {
                            let condition = {
                                let left = {
                                    let object = fill.clone();
                                    h.get(&object, "side")?
                                };
                                let right = lit(h, "B");
                                h.boolean(h.strict_equal(&left, &right))
                            };
                            if h.truthy(&condition) {
                                {
                                    let object = h.global("OrderSide")?;
                                    h.get(&object, "Buy")?
                                }
                            } else {
                                {
                                    let object = h.global("OrderSide")?;
                                    h.get(&object, "Sell")?
                                }
                            }
                        };
                        h.define_data(&object, "side", &value)?;
                        let value = symbol.clone();
                        h.define_data(&object, "symbol", &value)?;
                        let value = {
                            let function = h.global("parseFloat")?;
                            let receiver = h.undefined();
                            let arg0 = {
                                let object = fill.clone();
                                h.get(&object, "px")?
                            };
                            h.call(&function, &receiver, &[arg0])?
                        };
                        h.define_data(&object, "price", &value)?;
                        let value = {
                            let function = h.global("parseFloat")?;
                            let receiver = h.undefined();
                            let arg0 = {
                                let object = fill.clone();
                                h.get(&object, "sz")?
                            };
                            h.call(&function, &receiver, &[arg0])?
                        };
                        h.define_data(&object, "amount", &value)?;
                        let value = {
                            let condition = {
                                let left = fill_position_side.clone();
                                if !h.truthy(&left) {
                                    left
                                } else {
                                    {
                                        let receiver = fill_dir.clone();
                                        let function = h.get(&receiver, "includes")?;
                                        let arg0 = lit(h, "close");
                                        h.call(&function, &receiver, &[arg0])?
                                    }
                                }
                            };
                            if h.truthy(&condition) {
                                {
                                    let object = h.object()?;
                                    let value = net_realized_pnl.clone();
                                    h.define_data(&object, "realizedPnl", &value)?;
                                    let value = fee.clone();
                                    h.define_data(&object, "fee", &value)?;
                                    object
                                }
                            } else {
                                {
                                    let object = h.object()?;
                                    object
                                }
                            }
                        };
                        h.spread_into(&object, &value)?;
                        object
                    };
                    h.call(&function, &receiver, &[arg0, arg1])?
                };
                Ok(())
            })();
            step?;
            fill_idx += 1;
        }
    }
    if {
        let condition = {
            let value = {
                let object = data.clone();
                h.get(&object, "isSnapshot")?
            };
            h.boolean(!h.truthy(&value))
        };
        h.truthy(&condition)
    } {
        let hip3_dexes = {
            let receiver = this_value.clone();
            let function = h.get(&receiver, "collectHip3DexesFromFills")?;
            let arg0 = sorted_fills.clone();
            h.call(&function, &receiver, &[arg0])?
        };
        if {
            let condition = {
                let left = {
                    let object = hip3_dexes.clone();
                    h.get(&object, "size")?
                };
                let right = h.number(0.0);
                h.position_operator("greater", &left, &right)?
            };
            h.truthy(&condition)
        } {
            let _ = {
                let receiver = {
                    let object = this_value.clone();
                    h.get(&object, "parent")?
                };
                let function = h.get(&receiver, "scheduleHip3PositionRefresh")?;
                let arg0 = hip3_dexes.clone();
                h.call(&function, &receiver, &[arg0])?
            };
        }
    }
    Ok(h.undefined())
}
/// Source: hyperliquid.ws-private.ts::applyOptimisticCorePositionFromFill
pub fn apply_optimistic_core_position_from_fill<H: PositionOrderHost>(
    h: &mut H,
    this_value: &H::Value,
    fill: &H::Value,
    markets_by_id: &H::Value,
) -> Result<H::Value, H::Error> {
    let fill = fill.clone();
    let markets_by_id = markets_by_id.clone();
    if {
        let condition = {
            let left = {
                let value = {
                    'optional: {
                        let mut object = fill.clone();
                        if h.is_nullish(&object) {
                            break 'optional h.undefined();
                        }
                        object = h.get(&object, "coin")?;
                        object
                    }
                };
                h.boolean(!h.truthy(&value))
            };
            if h.truthy(&left) {
                left
            } else {
                {
                    let receiver = {
                        let object = fill.clone();
                        h.get(&object, "coin")?
                    };
                    let function = h.get(&receiver, "includes")?;
                    let arg0 = lit(h, ":");
                    h.call(&function, &receiver, &[arg0])?
                }
            }
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    let fill_price = {
        let receiver = this_value.clone();
        let function = h.get(&receiver, "safeParseNumber")?;
        let arg0 = {
            let object = fill.clone();
            h.get(&object, "px")?
        };
        h.call(&function, &receiver, &[arg0])?
    };
    let fill_size = {
        let receiver = this_value.clone();
        let function = h.get(&receiver, "safeParseNumber")?;
        let arg0 = {
            let object = fill.clone();
            h.get(&object, "sz")?
        };
        h.call(&function, &receiver, &[arg0])?
    };
    let start_position = {
        let function = h.global("Number")?;
        let receiver = h.undefined();
        let arg0 = {
            let object = fill.clone();
            h.get(&object, "startPosition")?
        };
        h.call(&function, &receiver, &[arg0])?
    };
    if {
        let condition = {
            let left = {
                let left = {
                    let left = {
                        let value = {
                            let receiver = h.global("Number")?;
                            let function = h.get(&receiver, "isFinite")?;
                            let arg0 = fill_price.clone();
                            h.call(&function, &receiver, &[arg0])?
                        };
                        h.boolean(!h.truthy(&value))
                    };
                    if h.truthy(&left) {
                        left
                    } else {
                        {
                            let left = fill_price.clone();
                            let right = h.number(0.0);
                            h.position_operator("lessEqual", &left, &right)?
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
                            let arg0 = fill_size.clone();
                            h.call(&function, &receiver, &[arg0])?
                        };
                        h.boolean(!h.truthy(&value))
                    }
                }
            };
            if h.truthy(&left) {
                left
            } else {
                {
                    let left = fill_size.clone();
                    let right = h.number(0.0);
                    h.position_operator("lessEqual", &left, &right)?
                }
            }
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    if {
        let condition = {
            let value = {
                let receiver = h.global("Number")?;
                let function = h.get(&receiver, "isFinite")?;
                let arg0 = start_position.clone();
                h.call(&function, &receiver, &[arg0])?
            };
            h.boolean(!h.truthy(&value))
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    let side = {
        let receiver = {
            let function = h.global("String")?;
            let receiver = h.undefined();
            let arg0 = {
                let left = {
                    let object = fill.clone();
                    h.get(&object, "side")?
                };
                if h.truthy(&left) { left } else { lit(h, "") }
            };
            h.call(&function, &receiver, &[arg0])?
        };
        let function = h.get(&receiver, "toUpperCase")?;
        h.call(&function, &receiver, &[])?
    };
    let signed_fill_size = {
        let condition = {
            let left = side.clone();
            let right = lit(h, "B");
            h.boolean(h.strict_equal(&left, &right))
        };
        if h.truthy(&condition) {
            fill_size.clone()
        } else {
            {
                let value = fill_size.clone();
                h.position_unary("minus", &value)?
            }
        }
    };
    let mut market = {
        let object = markets_by_id.clone();
        {
            let key = {
                let object = fill.clone();
                h.get(&object, "coin")?
            };
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
        market = {
            let receiver = {
                let object = {
                    let object = this_value.clone();
                    h.get(&object, "parent")?
                };
                h.get(&object, "marketsLowerCaseMap")?
            };
            let function = h.get(&receiver, "get")?;
            let arg0 = {
                let receiver = {
                    let object = fill.clone();
                    h.get(&object, "coin")?
                };
                let function = h.get(&receiver, "toLowerCase")?;
                h.call(&function, &receiver, &[])?
            };
            h.call(&function, &receiver, &[arg0])?
        };
    }
    if {
        let condition = {
            let left = {
                let value = market.clone();
                h.boolean(!h.truthy(&value))
            };
            if h.truthy(&left) {
                left
            } else {
                {
                    let receiver = {
                        let object = market.clone();
                        h.get(&object, "id")?
                    };
                    let function = h.get(&receiver, "includes")?;
                    let arg0 = lit(h, ":");
                    h.call(&function, &receiver, &[arg0])?
                }
            }
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    if {
        let condition = {
            let object = market.clone();
            h.get(&object, "isSpot")?
        };
        h.truthy(&condition)
    } {
        let _ = {
            let receiver = {
                let object = this_value.clone();
                h.get(&object, "parent")?
            };
            let function = h.get(&receiver, "requestUnifiedSpotCollateralRefresh")?;
            h.call(&function, &receiver, &[])?
        };
        return Ok(h.undefined());
    }
    let mut next_signed_position = {
        let left = start_position.clone();
        let right = signed_fill_size.clone();
        h.position_operator("add", &left, &right)?
    };
    if {
        let condition = {
            let left = {
                let receiver = h.global("Math")?;
                let function = h.get(&receiver, "abs")?;
                let arg0 = next_signed_position.clone();
                h.call(&function, &receiver, &[arg0])?
            };
            let right = {
                let object = h.global("HyperliquidPrivateWebSocket")?;
                h.get(&object, "CORE_FILL_POSITION_EPSILON")?
            };
            h.position_operator("less", &left, &right)?
        };
        h.truthy(&condition)
    } {
        next_signed_position = h.number(0.0);
    }
    let next_contracts = {
        let receiver = h.global("Math")?;
        let function = h.get(&receiver, "abs")?;
        let arg0 = next_signed_position.clone();
        h.call(&function, &receiver, &[arg0])?
    };
    let next_side = {
        let condition = {
            let left = next_signed_position.clone();
            let right = h.number(0.0);
            h.position_operator("less", &left, &right)?
        };
        if h.truthy(&condition) {
            {
                let object = h.global("PositionSide")?;
                h.get(&object, "Short")?
            }
        } else {
            {
                let object = h.global("PositionSide")?;
                h.get(&object, "Long")?
            }
        }
    };
    let existing_position = {
        let receiver = {
            let object = {
                let object = this_value.clone();
                h.get(&object, "store")?
            };
            h.get(&object, "positions")?
        };
        let function = h.get(&receiver, "find")?;
        let arg0 =
            h.position_callback(PositionCallback::ApplyOptimisticCorePositionFromFill0([
                market.clone(),
            ]))?;
        h.call(&function, &receiver, &[arg0])?
    };
    let symbol_config = {
        let receiver = {
            let object = this_value.clone();
            h.get(&object, "parent")?
        };
        let function = h.get(&receiver, "_getSymbolConfigurationFromAssetData")?;
        let arg0 = {
            let object = market.clone();
            h.get(&object, "id")?
        };
        h.call(&function, &receiver, &[arg0])?
    };
    if {
        let condition = {
            let left = {
                let left = {
                    'optional: {
                        let mut object = existing_position.clone();
                        if h.is_nullish(&object) {
                            break 'optional h.undefined();
                        }
                        object = h.get(&object, "positionMode")?;
                        object
                    }
                };
                if h.truthy(&left) {
                    left
                } else {
                    {
                        let object = symbol_config.clone();
                        h.get(&object, "positionMode")?
                    }
                }
            };
            let right = {
                let object = h.global("PositionMode")?;
                h.get(&object, "Hedged")?
            };
            h.boolean(h.strict_equal(&left, &right))
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    if {
        let condition = {
            let left = {
                let value = existing_position.clone();
                h.boolean(!h.truthy(&value))
            };
            if !h.truthy(&left) {
                left
            } else {
                {
                    let left = next_contracts.clone();
                    let right = h.number(0.0);
                    h.boolean(h.strict_equal(&left, &right))
                }
            }
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    if {
        let condition = {
            let left = {
                let left = existing_position.clone();
                if !h.truthy(&left) {
                    left
                } else {
                    {
                        let left = {
                            let receiver = h.global("Math")?;
                            let function = h.get(&receiver, "abs")?;
                            let arg0 = {
                                let left = {
                                    let object = existing_position.clone();
                                    h.get(&object, "contracts")?
                                };
                                let right = next_contracts.clone();
                                h.position_operator("subtract", &left, &right)?
                            };
                            h.call(&function, &receiver, &[arg0])?
                        };
                        let right = {
                            let object = h.global("HyperliquidPrivateWebSocket")?;
                            h.get(&object, "CORE_FILL_POSITION_EPSILON")?
                        };
                        h.position_operator("less", &left, &right)?
                    }
                }
            };
            if !h.truthy(&left) {
                left
            } else {
                {
                    let left = {
                        let object = existing_position.clone();
                        h.get(&object, "side")?
                    };
                    let right = next_side.clone();
                    h.boolean(h.strict_equal(&left, &right))
                }
            }
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    let position_mode = {
        let left = {
            'optional: {
                let mut object = existing_position.clone();
                if h.is_nullish(&object) {
                    break 'optional h.undefined();
                }
                object = h.get(&object, "positionMode")?;
                object
            }
        };
        if h.truthy(&left) {
            left
        } else {
            {
                let object = h.global("PositionMode")?;
                h.get(&object, "OneWay")?
            }
        }
    };
    let leverage = {
        let left = {
            'optional: {
                let mut object = existing_position.clone();
                if h.is_nullish(&object) {
                    break 'optional h.undefined();
                }
                object = h.get(&object, "leverage")?;
                object
            }
        };
        if h.truthy(&left) {
            left
        } else {
            {
                let object = symbol_config.clone();
                h.get(&object, "leverage")?
            }
        }
    };
    let margin_type = {
        let left = {
            'optional: {
                let mut object = existing_position.clone();
                if h.is_nullish(&object) {
                    break 'optional h.undefined();
                }
                object = h.get(&object, "marginType")?;
                object
            }
        };
        if h.truthy(&left) {
            left
        } else {
            {
                let object = symbol_config.clone();
                h.get(&object, "marginType")?
            }
        }
    };
    let mut next_entry_price = h.number(0.0);
    if {
        let condition = {
            let left = next_contracts.clone();
            let right = h.number(0.0);
            h.position_operator("greater", &left, &right)?
        };
        h.truthy(&condition)
    } {
        let prev_contracts = {
            let receiver = h.global("Math")?;
            let function = h.get(&receiver, "max")?;
            let arg0 = {
                let receiver = h.global("Math")?;
                let function = h.get(&receiver, "abs")?;
                let arg0 = start_position.clone();
                h.call(&function, &receiver, &[arg0])?
            };
            let arg1 = h.number(0.0);
            h.call(&function, &receiver, &[arg0, arg1])?
        };
        let same_direction = {
            let left = {
                let left = {
                    let receiver = h.global("Math")?;
                    let function = h.get(&receiver, "sign")?;
                    let arg0 = start_position.clone();
                    h.call(&function, &receiver, &[arg0])?
                };
                let right = {
                    let receiver = h.global("Math")?;
                    let function = h.get(&receiver, "sign")?;
                    let arg0 = next_signed_position.clone();
                    h.call(&function, &receiver, &[arg0])?
                };
                h.boolean(h.strict_equal(&left, &right))
            };
            if !h.truthy(&left) {
                left
            } else {
                {
                    let left = {
                        let receiver = h.global("Math")?;
                        let function = h.get(&receiver, "sign")?;
                        let arg0 = start_position.clone();
                        h.call(&function, &receiver, &[arg0])?
                    };
                    let right = h.number(0.0);
                    h.boolean(!h.strict_equal(&left, &right))
                }
            }
        };
        let is_adding_to_position = {
            let left = same_direction.clone();
            if !h.truthy(&left) {
                left
            } else {
                {
                    let left = next_contracts.clone();
                    let right = prev_contracts.clone();
                    h.position_operator("greater", &left, &right)?
                }
            }
        };
        let has_usable_existing_entry = {
            let left = {
                let left = {
                    let value = {
                        let value = existing_position.clone();
                        h.boolean(!h.truthy(&value))
                    };
                    h.boolean(!h.truthy(&value))
                };
                if !h.truthy(&left) {
                    left
                } else {
                    {
                        let receiver = h.global("Number")?;
                        let function = h.get(&receiver, "isFinite")?;
                        let arg0 = {
                            let object = existing_position.clone();
                            h.get(&object, "entryPrice")?
                        };
                        h.call(&function, &receiver, &[arg0])?
                    }
                }
            };
            if !h.truthy(&left) {
                left
            } else {
                {
                    let left = {
                        let object = existing_position.clone();
                        h.get(&object, "entryPrice")?
                    };
                    let right = h.number(0.0);
                    h.position_operator("greater", &left, &right)?
                }
            }
        };
        if {
            let condition = {
                let value = has_usable_existing_entry.clone();
                h.boolean(!h.truthy(&value))
            };
            h.truthy(&condition)
        } {
            next_entry_price = fill_price.clone();
        } else {
            if {
                let condition = {
                    let left = is_adding_to_position.clone();
                    if !h.truthy(&left) {
                        left
                    } else {
                        {
                            let left = prev_contracts.clone();
                            let right = h.number(0.0);
                            h.position_operator("greater", &left, &right)?
                        }
                    }
                };
                h.truthy(&condition)
            } {
                next_entry_price = {
                    let left = {
                        let left = {
                            let left = {
                                let object = existing_position.clone();
                                h.get(&object, "entryPrice")?
                            };
                            let right = prev_contracts.clone();
                            h.position_operator("multiply", &left, &right)?
                        };
                        let right = {
                            let left = fill_price.clone();
                            let right = {
                                let receiver = h.global("Math")?;
                                let function = h.get(&receiver, "abs")?;
                                let arg0 = signed_fill_size.clone();
                                h.call(&function, &receiver, &[arg0])?
                            };
                            h.position_operator("multiply", &left, &right)?
                        };
                        h.position_operator("add", &left, &right)?
                    };
                    let right = next_contracts.clone();
                    h.position_operator("divide", &left, &right)?
                };
            } else {
                if {
                    let condition = {
                        let left = same_direction.clone();
                        if !h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = next_contracts.clone();
                                let right = prev_contracts.clone();
                                h.position_operator("lessEqual", &left, &right)?
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    next_entry_price = {
                        let object = existing_position.clone();
                        h.get(&object, "entryPrice")?
                    };
                } else {
                    next_entry_price = fill_price.clone();
                }
            }
        }
    }
    let optimistic_position = {
        let object = h.object()?;
        let value = {
            let left = existing_position.clone();
            if h.truthy(&left) {
                left
            } else {
                {
                    let object = h.object()?;
                    let value = {
                        let function = h.global("createPositionId")?;
                        let receiver = h.undefined();
                        let arg0 = {
                            let object = market.clone();
                            h.get(&object, "symbol")?
                        };
                        let arg1 = next_side.clone();
                        let arg2 = position_mode.clone();
                        h.call(&function, &receiver, &[arg0, arg1, arg2])?
                    };
                    h.define_data(&object, "id", &value)?;
                    let value = {
                        let object = market.clone();
                        h.get(&object, "symbol")?
                    };
                    h.define_data(&object, "symbol", &value)?;
                    let value = next_side.clone();
                    h.define_data(&object, "side", &value)?;
                    let value = h.number(0.0);
                    h.define_data(&object, "entryPrice", &value)?;
                    let value = h.number(0.0);
                    h.define_data(&object, "notional", &value)?;
                    let value = leverage.clone();
                    h.define_data(&object, "leverage", &value)?;
                    let value = h.number(0.0);
                    h.define_data(&object, "unrealizedPnl", &value)?;
                    let value = h.number(0.0);
                    h.define_data(&object, "contracts", &value)?;
                    let value = h.number(0.0);
                    h.define_data(&object, "liquidationPrice", &value)?;
                    let value = position_mode.clone();
                    h.define_data(&object, "positionMode", &value)?;
                    let value = margin_type.clone();
                    h.define_data(&object, "marginType", &value)?;
                    object
                }
            }
        };
        h.spread_into(&object, &value)?;
        let value = {
            let function = h.global("createPositionId")?;
            let receiver = h.undefined();
            let arg0 = {
                let object = market.clone();
                h.get(&object, "symbol")?
            };
            let arg1 = next_side.clone();
            let arg2 = position_mode.clone();
            h.call(&function, &receiver, &[arg0, arg1, arg2])?
        };
        h.define_data(&object, "id", &value)?;
        let value = {
            let object = market.clone();
            h.get(&object, "symbol")?
        };
        h.define_data(&object, "symbol", &value)?;
        let value = next_side.clone();
        h.define_data(&object, "side", &value)?;
        let value = next_entry_price.clone();
        h.define_data(&object, "entryPrice", &value)?;
        let value = {
            let condition = {
                let left = next_contracts.clone();
                let right = h.number(0.0);
                h.boolean(h.strict_equal(&left, &right))
            };
            if h.truthy(&condition) {
                h.number(0.0)
            } else {
                {
                    let left = next_contracts.clone();
                    let right = fill_price.clone();
                    h.position_operator("multiply", &left, &right)?
                }
            }
        };
        h.define_data(&object, "notional", &value)?;
        let value = leverage.clone();
        h.define_data(&object, "leverage", &value)?;
        let value = margin_type.clone();
        h.define_data(&object, "marginType", &value)?;
        let value = position_mode.clone();
        h.define_data(&object, "positionMode", &value)?;
        let value = {
            let condition = {
                let left = next_contracts.clone();
                let right = h.number(0.0);
                h.boolean(h.strict_equal(&left, &right))
            };
            if h.truthy(&condition) {
                h.number(0.0)
            } else {
                {
                    let left = {
                        'optional: {
                            let mut object = existing_position.clone();
                            if h.is_nullish(&object) {
                                break 'optional h.undefined();
                            }
                            object = h.get(&object, "unrealizedPnl")?;
                            object
                        }
                    };
                    if h.truthy(&left) { left } else { h.number(0.0) }
                }
            }
        };
        h.define_data(&object, "unrealizedPnl", &value)?;
        let value = next_contracts.clone();
        h.define_data(&object, "contracts", &value)?;
        let value = {
            let condition = {
                let left = next_contracts.clone();
                let right = h.number(0.0);
                h.boolean(h.strict_equal(&left, &right))
            };
            if h.truthy(&condition) {
                h.number(0.0)
            } else {
                {
                    let left = {
                        'optional: {
                            let mut object = existing_position.clone();
                            if h.is_nullish(&object) {
                                break 'optional h.undefined();
                            }
                            object = h.get(&object, "liquidationPrice")?;
                            object
                        }
                    };
                    if h.truthy(&left) { left } else { h.number(0.0) }
                }
            }
        };
        h.define_data(&object, "liquidationPrice", &value)?;
        object
    };
    let _ = {
        let receiver = {
            let object = {
                let object = this_value.clone();
                h.get(&object, "parent")?
            };
            h.get(&object, "store")?
        };
        let function = h.get(&receiver, "addOrUpdatePosition")?;
        let arg0 = optimistic_position.clone();
        h.call(&function, &receiver, &[arg0])?
    };
    Ok(h.undefined())
}
/// Source: hyperliquid.ws-private.ts::handleOrderUpdates
pub fn handle_order_updates<H: PositionOrderHost>(
    h: &mut H,
    this_value: &H::Value,
    updates: &H::Value,
) -> Result<H::Value, H::Error> {
    let updates = updates.clone();
    if {
        let condition = {
            let left = {
                let value = {
                    let receiver = h.global("Array")?;
                    let function = h.get(&receiver, "isArray")?;
                    let arg0 = updates.clone();
                    h.call(&function, &receiver, &[arg0])?
                };
                h.boolean(!h.truthy(&value))
            };
            if h.truthy(&left) {
                left
            } else {
                {
                    let left = {
                        let object = updates.clone();
                        h.get(&object, "length")?
                    };
                    let right = h.number(0.0);
                    h.boolean(h.strict_equal(&left, &right))
                }
            }
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    let markets_by_id = {
        let object = {
            let object = {
                let object = this_value.clone();
                h.get(&object, "parent")?
            };
            h.get(&object, "store")?
        };
        h.get(&object, "marketsById")?
    };
    let store_orders = {
        let object = {
            let object = {
                let object = this_value.clone();
                h.get(&object, "parent")?
            };
            h.get(&object, "store")?
        };
        h.get(&object, "orders")?
    };
    let existing_orders_by_id = {
        let object = h.object()?;
        object
    };
    {
        let mut i: usize = 0;
        while {
            let left = h.number(i as f64);
            let right = {
                let object = store_orders.clone();
                h.get(&object, "length")?
            };
            let condition = h.position_operator("less", &left, &right)?;
            h.truthy(&condition)
        } {
            let step = (|| -> Result<(), H::Error> {
                {
                    let object = existing_orders_by_id.clone();
                    let key = {
                        let object = {
                            let object = store_orders.clone();
                            {
                                let key = h.number(i as f64);
                                h.index(&object, &key)?
                            }
                        };
                        h.get(&object, "id")?
                    };
                    let value = {
                        let object = store_orders.clone();
                        {
                            let key = h.number(i as f64);
                            h.index(&object, &key)?
                        }
                    };
                    h.set_position_index(&object, &key, &value)?;
                }
                Ok(())
            })();
            step?;
            i += 1;
        }
    }
    {
        let value = updates.clone();
        let iterator = h.iterator(&value)?;
        while let Some(update) = h.next(&iterator)? {
            let step = (|| -> Result<(), H::Error> {
                let order_data = {
                    'optional: {
                        let mut object = update.clone();
                        if h.is_nullish(&object) {
                            break 'optional h.undefined();
                        }
                        object = h.get(&object, "order")?;
                        object
                    }
                };
                if {
                    let condition = {
                        let value = order_data.clone();
                        h.boolean(!h.truthy(&value))
                    };
                    h.truthy(&condition)
                } {
                    return Ok(());
                }
                let mut market = {
                    let object = markets_by_id.clone();
                    {
                        let key = {
                            let object = order_data.clone();
                            h.get(&object, "coin")?
                        };
                        h.index(&object, &key)?
                    }
                };
                if {
                    let condition = {
                        let left = {
                            let value = market.clone();
                            h.boolean(!h.truthy(&value))
                        };
                        if !h.truthy(&left) {
                            left
                        } else {
                            {
                                let receiver = {
                                    let object = order_data.clone();
                                    h.get(&object, "coin")?
                                };
                                let function = h.get(&receiver, "includes")?;
                                let arg0 = lit(h, ":");
                                h.call(&function, &receiver, &[arg0])?
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    market = {
                        let receiver = {
                            let object = {
                                let object = this_value.clone();
                                h.get(&object, "parent")?
                            };
                            h.get(&object, "marketsLowerCaseMap")?
                        };
                        let function = h.get(&receiver, "get")?;
                        let arg0 = {
                            let receiver = {
                                let object = order_data.clone();
                                h.get(&object, "coin")?
                            };
                            let function = h.get(&receiver, "toLowerCase")?;
                            h.call(&function, &receiver, &[])?
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                    if {
                        let condition = market.clone();
                        h.truthy(&condition)
                    } {
                        let _ = {
                            let receiver = {
                                let object = this_value.clone();
                                h.get(&object, "parent")?
                            };
                            let function = h.get(&receiver, "log")?;
                            let arg0 = {
                                let mut units: Vec<u16> = vec![
                                    91, 111, 114, 100, 101, 114, 85, 112, 100, 97, 116, 101, 115,
                                    93, 32, 70, 111, 117, 110, 100, 32, 109, 97, 114, 107, 101,
                                    116, 32, 118, 105, 97, 32, 99, 97, 115, 101, 45, 105, 110, 115,
                                    101, 110, 115, 105, 116, 105, 118, 101, 32, 108, 111, 111, 107,
                                    117, 112, 58, 32, 99, 111, 105, 110, 61, 34,
                                ];
                                let value = {
                                    let object = order_data.clone();
                                    h.get(&object, "coin")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[
                                    34, 32, 109, 97, 116, 99, 104, 101, 100, 32, 109, 97, 114, 107,
                                    101, 116, 46, 105, 100, 61, 34,
                                ]);
                                let value = {
                                    let object = market.clone();
                                    h.get(&object, "id")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[34]);
                                h.literal(&JsString(units))
                            };
                            let arg1 = {
                                let object = h.global("LogSeverity")?;
                                h.get(&object, "Info")?
                            };
                            h.call(&function, &receiver, &[arg0, arg1])?
                        };
                    }
                }
                if {
                    let condition = {
                        let value = market.clone();
                        h.boolean(!h.truthy(&value))
                    };
                    h.truthy(&condition)
                } {
                    let _ = {
                        let receiver = {
                            let object = this_value.clone();
                            h.get(&object, "parent")?
                        };
                        let function = h.get(&receiver, "log")?;
                        let arg0 = {
                            let mut units: Vec<u16> = vec![
                                91, 111, 114, 100, 101, 114, 85, 112, 100, 97, 116, 101, 115, 93,
                                32, 77, 97, 114, 107, 101, 116, 32, 110, 111, 116, 32, 102, 111,
                                117, 110, 100, 32, 102, 111, 114, 32, 111, 114, 100, 101, 114, 32,
                                99, 111, 105, 110, 61, 34,
                            ];
                            let value = {
                                let object = order_data.clone();
                                h.get(&object, "coin")?
                            };
                            units.extend(h.template_string(&value)?.0);
                            units.extend_from_slice(&[
                                34, 46, 32, 65, 118, 97, 105, 108, 97, 98, 108, 101, 32, 72, 73,
                                80, 45, 51, 32, 109, 97, 114, 107, 101, 116, 115, 58, 32,
                            ]);
                            let value = {
                                let receiver = {
                                    let receiver = {
                                        let receiver = {
                                            let receiver = h.global("Object")?;
                                            let function = h.get(&receiver, "keys")?;
                                            let arg0 = markets_by_id.clone();
                                            h.call(&function, &receiver, &[arg0])?
                                        };
                                        let function = h.get(&receiver, "filter")?;
                                        let arg0 = h.position_callback(
                                            PositionCallback::HandleOrderUpdates0([]),
                                        )?;
                                        h.call(&function, &receiver, &[arg0])?
                                    };
                                    let function = h.get(&receiver, "slice")?;
                                    let arg0 = h.number(0.0);
                                    let arg1 = h.number(5.0);
                                    h.call(&function, &receiver, &[arg0, arg1])?
                                };
                                let function = h.get(&receiver, "join")?;
                                let arg0 = lit(h, ", ");
                                h.call(&function, &receiver, &[arg0])?
                            };
                            units.extend(h.template_string(&value)?.0);
                            units.extend_from_slice(&[46, 46, 46]);
                            h.literal(&JsString(units))
                        };
                        let arg1 = {
                            let object = h.global("LogSeverity")?;
                            h.get(&object, "Warning")?
                        };
                        h.call(&function, &receiver, &[arg0, arg1])?
                    };
                    return Ok(());
                }
                let order_id = {
                    let mut units: Vec<u16> = vec![];
                    let value = {
                        let object = order_data.clone();
                        h.get(&object, "oid")?
                    };
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[]);
                    h.literal(&JsString(units))
                };
                let status = {
                    let object = update.clone();
                    h.get(&object, "status")?
                };
                let _ = {
                    let receiver = {
                        let object = this_value.clone();
                        h.get(&object, "parent")?
                    };
                    let function = h.get(&receiver, "log")?;
                    let arg0 = {
                        let mut units: Vec<u16> = vec![
                            91, 111, 114, 100, 101, 114, 85, 112, 100, 97, 116, 101, 115, 93, 32,
                            80, 114, 111, 99, 101, 115, 115, 105, 110, 103, 32, 111, 114, 100, 101,
                            114, 32, 117, 112, 100, 97, 116, 101, 58, 32, 99, 111, 105, 110, 61,
                        ];
                        let value = {
                            let object = order_data.clone();
                            h.get(&object, "coin")?
                        };
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[44, 32, 115, 121, 109, 98, 111, 108, 61]);
                        let value = {
                            let object = market.clone();
                            h.get(&object, "symbol")?
                        };
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[44, 32, 111, 105, 100, 61]);
                        let value = order_id.clone();
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[44, 32, 115, 116, 97, 116, 117, 115, 61]);
                        let value = status.clone();
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[]);
                        h.literal(&JsString(units))
                    };
                    let arg1 = {
                        let object = h.global("LogSeverity")?;
                        h.get(&object, "Debug")?
                    };
                    h.call(&function, &receiver, &[arg0, arg1])?
                };
                if {
                    let condition = {
                        let left = {
                            let left = {
                                let left = {
                                    let left = {
                                        let left = {
                                            let left = status.clone();
                                            let right = lit(h, "filled");
                                            h.boolean(h.strict_equal(&left, &right))
                                        };
                                        if h.truthy(&left) {
                                            left
                                        } else {
                                            {
                                                let left = status.clone();
                                                let right = lit(h, "canceled");
                                                h.boolean(h.strict_equal(&left, &right))
                                            }
                                        }
                                    };
                                    if h.truthy(&left) {
                                        left
                                    } else {
                                        {
                                            let left = status.clone();
                                            let right = lit(h, "rejected");
                                            h.boolean(h.strict_equal(&left, &right))
                                        }
                                    }
                                };
                                if h.truthy(&left) {
                                    left
                                } else {
                                    {
                                        let left = status.clone();
                                        let right = lit(h, "marginCanceled");
                                        h.boolean(h.strict_equal(&left, &right))
                                    }
                                }
                            };
                            if h.truthy(&left) {
                                left
                            } else {
                                {
                                    let left = status.clone();
                                    let right = lit(h, "reduceOnlyCanceled");
                                    h.boolean(h.strict_equal(&left, &right))
                                }
                            }
                        };
                        if h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = status.clone();
                                let right = lit(h, "reduceOnlyRejected");
                                h.boolean(h.strict_equal(&left, &right))
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    let _ = {
                        let receiver = {
                            let object = this_value.clone();
                            h.get(&object, "parent")?
                        };
                        let function = h.get(&receiver, "suppressCancelledOrder")?;
                        let arg0 = order_id.clone();
                        h.call(&function, &receiver, &[arg0])?
                    };
                    let _ = {
                        let receiver = {
                            let object = {
                                let object = this_value.clone();
                                h.get(&object, "parent")?
                            };
                            h.get(&object, "store")?
                        };
                        let function = h.get(&receiver, "removeOrder")?;
                        let arg0 = {
                            let object = h.object()?;
                            let value = order_id.clone();
                            h.define_data(&object, "id", &value)?;
                            object
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                    let _ = {
                        let receiver = {
                            let object = this_value.clone();
                            h.get(&object, "parent")?
                        };
                        let function = h.get(&receiver, "forgetLimitOrderTimeInForce")?;
                        let arg0 = order_id.clone();
                        h.call(&function, &receiver, &[arg0])?
                    };
                    return Ok(());
                }
                if {
                    let condition = {
                        let receiver = {
                            let object = this_value.clone();
                            h.get(&object, "parent")?
                        };
                        let function = h.get(&receiver, "isOrderCancelled")?;
                        let arg0 = order_id.clone();
                        h.call(&function, &receiver, &[arg0])?
                    };
                    h.truthy(&condition)
                } {
                    let _ = {
                        let receiver = {
                            let object = this_value.clone();
                            h.get(&object, "parent")?
                        };
                        let function = h.get(&receiver, "log")?;
                        let arg0 = {
                            let mut units: Vec<u16> = vec![
                                91, 111, 114, 100, 101, 114, 85, 112, 100, 97, 116, 101, 115, 93,
                                32, 83, 117, 112, 112, 114, 101, 115, 115, 105, 110, 103, 32, 115,
                                116, 97, 108, 101, 32, 111, 112, 101, 110, 32, 115, 116, 97, 116,
                                117, 115, 32, 102, 111, 114, 32, 114, 101, 99, 101, 110, 116, 108,
                                121, 32, 99, 97, 110, 99, 101, 108, 108, 101, 100, 32, 111, 114,
                                100, 101, 114, 58, 32, 111, 105, 100, 61,
                            ];
                            let value = order_id.clone();
                            units.extend(h.template_string(&value)?.0);
                            units.extend_from_slice(&[]);
                            h.literal(&JsString(units))
                        };
                        let arg1 = {
                            let object = h.global("LogSeverity")?;
                            h.get(&object, "Debug")?
                        };
                        h.call(&function, &receiver, &[arg0, arg1])?
                    };
                    return Ok(());
                }
                if {
                    let condition = {
                        let left = {
                            let left = status.clone();
                            let right = lit(h, "open");
                            h.boolean(h.strict_equal(&left, &right))
                        };
                        if h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = status.clone();
                                let right = lit(h, "triggered");
                                h.boolean(h.strict_equal(&left, &right))
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    let inferred_type: H::Value;
                    let inferred_price: H::Value;
                    let inference_source: H::Value;
                    let is_hip3 = {
                        let receiver = {
                            let object = market.clone();
                            h.get(&object, "id")?
                        };
                        let function = h.get(&receiver, "includes")?;
                        let arg0 = lit(h, ":");
                        h.call(&function, &receiver, &[arg0])?
                    };
                    let pending_order = {
                        let condition = is_hip3.clone();
                        if h.truthy(&condition) {
                            {
                                let receiver = {
                                    let object = this_value.clone();
                                    h.get(&object, "parent")?
                                };
                                let function = h.get(&receiver, "getPendingHip3Order")?;
                                let arg0 = order_id.clone();
                                h.call(&function, &receiver, &[arg0])?
                            }
                        } else {
                            h.null_value()
                        }
                    };
                    if {
                        let condition = pending_order.clone();
                        h.truthy(&condition)
                    } {
                        inferred_type = {
                            let object = pending_order.clone();
                            h.get(&object, "type")?
                        };
                        inferred_price = {
                            let object = pending_order.clone();
                            h.get(&object, "price")?
                        };
                        inference_source = lit(h, "pending-cache");
                        let _ = {
                            let receiver = {
                                let object = this_value.clone();
                                h.get(&object, "parent")?
                            };
                            let function = h.get(&receiver, "removePendingHip3Order")?;
                            let arg0 = order_id.clone();
                            h.call(&function, &receiver, &[arg0])?
                        };
                        let side = {
                            let condition = {
                                let left = {
                                    let object = order_data.clone();
                                    h.get(&object, "side")?
                                };
                                let right = lit(h, "B");
                                h.boolean(h.strict_equal(&left, &right))
                            };
                            if h.truthy(&condition) {
                                {
                                    let object = h.global("OrderSide")?;
                                    h.get(&object, "Buy")?
                                }
                            } else {
                                {
                                    let object = h.global("OrderSide")?;
                                    h.get(&object, "Sell")?
                                }
                            }
                        };
                        let _ = {
                            let receiver = {
                                let object = this_value.clone();
                                h.get(&object, "parent")?
                            };
                            let function = h.get(&receiver, "matchPrePlacement")?;
                            let arg0 = {
                                let object = order_data.clone();
                                h.get(&object, "coin")?
                            };
                            let arg1 = side.clone();
                            h.call(&function, &receiver, &[arg0, arg1])?
                        };
                    } else {
                        if {
                            let condition = {
                                let left = {
                                    let left = {
                                        let left = is_hip3.clone();
                                        if !h.truthy(&left) {
                                            left
                                        } else {
                                            {
                                                let value = {
                                                    let object = order_data.clone();
                                                    h.get(&object, "orderType")?
                                                };
                                                h.boolean(!h.truthy(&value))
                                            }
                                        }
                                    };
                                    if !h.truthy(&left) {
                                        left
                                    } else {
                                        {
                                            let value = {
                                                let object = order_data.clone();
                                                h.get(&object, "triggerPx")?
                                            };
                                            h.boolean(!h.truthy(&value))
                                        }
                                    }
                                };
                                if !h.truthy(&left) {
                                    left
                                } else {
                                    {
                                        let object = order_data.clone();
                                        h.get(&object, "limitPx")?
                                    }
                                }
                            };
                            h.truthy(&condition)
                        } {
                            let existing_order = {
                                let object = existing_orders_by_id.clone();
                                {
                                    let key = order_id.clone();
                                    h.index(&object, &key)?
                                }
                            };
                            if {
                                let condition = existing_order.clone();
                                h.truthy(&condition)
                            } {
                                inferred_type = {
                                    let object = existing_order.clone();
                                    h.get(&object, "type")?
                                };
                                inferred_price = {
                                    let object = existing_order.clone();
                                    h.get(&object, "price")?
                                };
                                if {
                                    let condition = {
                                        let left = {
                                            let left = {
                                                let object = existing_order.clone();
                                                h.get(&object, "status")?
                                            };
                                            let right = {
                                                let object = h.global("OrderStatus")?;
                                                h.get(&object, "Creating")?
                                            };
                                            h.boolean(h.strict_equal(&left, &right))
                                        };
                                        if h.truthy(&left) {
                                            left
                                        } else {
                                            {
                                                let left = {
                                                    let object = existing_order.clone();
                                                    h.get(&object, "status")?
                                                };
                                                let right = {
                                                    let object = h.global("OrderStatus")?;
                                                    h.get(&object, "Updating")?
                                                };
                                                h.boolean(h.strict_equal(&left, &right))
                                            }
                                        }
                                    };
                                    h.truthy(&condition)
                                } {
                                    inference_source = lit(h, "optimistic-preserved");
                                } else {
                                    inference_source = lit(h, "store-preserved");
                                }
                            } else {
                                let side = {
                                    let condition = {
                                        let left = {
                                            let object = order_data.clone();
                                            h.get(&object, "side")?
                                        };
                                        let right = lit(h, "B");
                                        h.boolean(h.strict_equal(&left, &right))
                                    };
                                    if h.truthy(&condition) {
                                        {
                                            let object = h.global("OrderSide")?;
                                            h.get(&object, "Buy")?
                                        }
                                    } else {
                                        {
                                            let object = h.global("OrderSide")?;
                                            h.get(&object, "Sell")?
                                        }
                                    }
                                };
                                let ws_limit_px = {
                                    let left = {
                                        let function = h.global("parseFloat")?;
                                        let receiver = h.undefined();
                                        let arg0 = {
                                            let object = order_data.clone();
                                            h.get(&object, "limitPx")?
                                        };
                                        h.call(&function, &receiver, &[arg0])?
                                    };
                                    if h.truthy(&left) { left } else { h.undefined() }
                                };
                                let ws_amount = {
                                    let condition = {
                                        let object = order_data.clone();
                                        h.get(&object, "origSz")?
                                    };
                                    if h.truthy(&condition) {
                                        {
                                            let value = {
                                                let object = order_data.clone();
                                                h.get(&object, "origSz")?
                                            };
                                            h.position_unary("plus", &value)?
                                        }
                                    } else {
                                        h.undefined()
                                    }
                                };
                                let pre_placement = {
                                    let receiver = {
                                        let object = this_value.clone();
                                        h.get(&object, "parent")?
                                    };
                                    let function = h.get(&receiver, "matchPrePlacement")?;
                                    let arg0 = {
                                        let object = order_data.clone();
                                        h.get(&object, "coin")?
                                    };
                                    let arg1 = side.clone();
                                    let arg2 = ws_limit_px.clone();
                                    let arg3 = ws_amount.clone();
                                    h.call(&function, &receiver, &[arg0, arg1, arg2, arg3])?
                                };
                                if {
                                    let condition = pre_placement.clone();
                                    h.truthy(&condition)
                                } {
                                    inferred_type = {
                                        let object = pre_placement.clone();
                                        h.get(&object, "type")?
                                    };
                                    inferred_price = {
                                        let object = pre_placement.clone();
                                        h.get(&object, "price")?
                                    };
                                    inference_source = lit(h, "pre-placement-cache");
                                } else {
                                    inferred_type = {
                                        let object = h.global("OrderType")?;
                                        h.get(&object, "Limit")?
                                    };
                                    inferred_price = {
                                        let left = {
                                            let function = h.global("parseFloat")?;
                                            let receiver = h.undefined();
                                            let arg0 = {
                                                let object = order_data.clone();
                                                h.get(&object, "limitPx")?
                                            };
                                            h.call(&function, &receiver, &[arg0])?
                                        };
                                        if h.truthy(&left) { left } else { h.number(0.0) }
                                    };
                                    inference_source = lit(h, "hip3-unconfirmed-placeholder");
                                    let _ = {
                                        let receiver = {
                                            let object = this_value.clone();
                                            h.get(&object, "parent")?
                                        };
                                        let function = h.get(&receiver, "scheduleOrderRefresh")?;
                                        h.call(&function, &receiver, &[])?
                                    };
                                }
                            }
                        } else {
                            if {
                                let condition = {
                                    let left = {
                                        let left = {
                                            let left = {
                                                let left = {
                                                    let left = {
                                                        let value = is_hip3.clone();
                                                        h.boolean(!h.truthy(&value))
                                                    };
                                                    if !h.truthy(&left) {
                                                        left
                                                    } else {
                                                        {
                                                            let function = h.global("Boolean")?;
                                                            let receiver = h.undefined();
                                                            let arg0 = {
                                                                let object = order_data.clone();
                                                                h.get(&object, "reduceOnly")?
                                                            };
                                                            h.call(&function, &receiver, &[arg0])?
                                                        }
                                                    }
                                                };
                                                if !h.truthy(&left) {
                                                    left
                                                } else {
                                                    {
                                                        let left = {
                                                            let value = {
                                                                let object = order_data.clone();
                                                                h.get(&object, "origSz")?
                                                            };
                                                            h.boolean(!h.truthy(&value))
                                                        };
                                                        if h.truthy(&left) {
                                                            left
                                                        } else {
                                                            {
                                                                let left = {
                                                                    let function =
                                                                        h.global("Number")?;
                                                                    let receiver = h.undefined();
                                                                    let arg0 = {
                                                                        let object =
                                                                            order_data.clone();
                                                                        h.get(&object, "origSz")?
                                                                    };
                                                                    h.call(
                                                                        &function,
                                                                        &receiver,
                                                                        &[arg0],
                                                                    )?
                                                                };
                                                                let right = h.number(0.0);
                                                                h.boolean(
                                                                    h.strict_equal(&left, &right),
                                                                )
                                                            }
                                                        }
                                                    }
                                                }
                                            };
                                            if !h.truthy(&left) {
                                                left
                                            } else {
                                                {
                                                    let value = {
                                                        let object = order_data.clone();
                                                        h.get(&object, "orderType")?
                                                    };
                                                    h.boolean(!h.truthy(&value))
                                                }
                                            }
                                        };
                                        if !h.truthy(&left) {
                                            left
                                        } else {
                                            {
                                                let value = {
                                                    let object = order_data.clone();
                                                    h.get(&object, "triggerPx")?
                                                };
                                                h.boolean(!h.truthy(&value))
                                            }
                                        }
                                    };
                                    if !h.truthy(&left) {
                                        left
                                    } else {
                                        {
                                            let value = {
                                                let object = order_data.clone();
                                                h.get(&object, "isTrigger")?
                                            };
                                            h.boolean(!h.truthy(&value))
                                        }
                                    }
                                };
                                h.truthy(&condition)
                            } {
                                let existing_order = {
                                    let object = existing_orders_by_id.clone();
                                    {
                                        let key = order_id.clone();
                                        h.index(&object, &key)?
                                    }
                                };
                                if {
                                    let condition = existing_order.clone();
                                    h.truthy(&condition)
                                } {
                                    inferred_type = {
                                        let object = existing_order.clone();
                                        h.get(&object, "type")?
                                    };
                                    inferred_price = {
                                        let object = existing_order.clone();
                                        h.get(&object, "price")?
                                    };
                                    inference_source = {
                                        let condition = {
                                            let left = {
                                                let left = {
                                                    let object = existing_order.clone();
                                                    h.get(&object, "status")?
                                                };
                                                let right = {
                                                    let object = h.global("OrderStatus")?;
                                                    h.get(&object, "Creating")?
                                                };
                                                h.boolean(h.strict_equal(&left, &right))
                                            };
                                            if h.truthy(&left) {
                                                left
                                            } else {
                                                {
                                                    let left = {
                                                        let object = existing_order.clone();
                                                        h.get(&object, "status")?
                                                    };
                                                    let right = {
                                                        let object = h.global("OrderStatus")?;
                                                        h.get(&object, "Updating")?
                                                    };
                                                    h.boolean(h.strict_equal(&left, &right))
                                                }
                                            }
                                        };
                                        if h.truthy(&condition) {
                                            lit(h, "optimistic-preserved")
                                        } else {
                                            lit(h, "store-preserved")
                                        }
                                    };
                                } else {
                                    let _ = {
                                        let receiver = {
                                            let object = this_value.clone();
                                            h.get(&object, "parent")?
                                        };
                                        let function = h.get(&receiver, "log")?;
                                        let arg0 = {
                                            let left = lit(
                                                h,
                                                "[orderUpdates] Deferring positional TP/SL insert (core-perp WS lacks orderType/triggerPx/isTrigger): ",
                                            );
                                            let right = {
                                                let mut units: Vec<u16> = vec![111, 105, 100, 61];
                                                let value = order_id.clone();
                                                units.extend(h.template_string(&value)?.0);
                                                units.extend_from_slice(&[
                                                    32, 99, 111, 105, 110, 61,
                                                ]);
                                                let value = {
                                                    let object = order_data.clone();
                                                    h.get(&object, "coin")?
                                                };
                                                units.extend(h.template_string(&value)?.0);
                                                units.extend_from_slice(&[
                                                    32, 108, 105, 109, 105, 116, 80, 120, 61,
                                                ]);
                                                let value = {
                                                    let object = order_data.clone();
                                                    h.get(&object, "limitPx")?
                                                };
                                                units.extend(h.template_string(&value)?.0);
                                                units.extend_from_slice(&[
                                                    46, 32, 83, 99, 104, 101, 100, 117, 108, 105,
                                                    110, 103, 32, 82, 69, 83, 84, 32, 114, 101,
                                                    102, 114, 101, 115, 104, 46,
                                                ]);
                                                h.literal(&JsString(units))
                                            };
                                            h.position_operator("add", &left, &right)?
                                        };
                                        let arg1 = {
                                            let object = h.global("LogSeverity")?;
                                            h.get(&object, "Debug")?
                                        };
                                        h.call(&function, &receiver, &[arg0, arg1])?
                                    };
                                    let _ = {
                                        let receiver = {
                                            let object = this_value.clone();
                                            h.get(&object, "parent")?
                                        };
                                        let function = h.get(&receiver, "scheduleOrderRefresh")?;
                                        h.call(&function, &receiver, &[])?
                                    };
                                    return Ok(());
                                }
                            } else {
                                let inferred = {
                                    let function = h.global("inferHyperliquidOrderTypeAndPrice")?;
                                    let receiver = h.undefined();
                                    let arg0 = order_data.clone();
                                    h.call(&function, &receiver, &[arg0])?
                                };
                                inferred_type = {
                                    let object = inferred.clone();
                                    h.get(&object, "type")?
                                };
                                inferred_price = {
                                    let object = inferred.clone();
                                    h.get(&object, "price")?
                                };
                                inference_source = {
                                    let object = inferred.clone();
                                    h.get(&object, "source")?
                                };
                            }
                        }
                    }
                    if {
                        let condition = is_hip3.clone();
                        h.truthy(&condition)
                    } {
                        let _ = {
                            let receiver = {
                                let object = this_value.clone();
                                h.get(&object, "parent")?
                            };
                            let function = h.get(&receiver, "log")?;
                            let arg0 = {
                                let mut units: Vec<u16> = vec![
                                    91, 111, 114, 100, 101, 114, 85, 112, 100, 97, 116, 101, 115,
                                    93, 32, 72, 73, 80, 45, 51, 32, 111, 114, 100, 101, 114, 32,
                                    116, 121, 112, 101, 32, 105, 110, 102, 101, 114, 101, 110, 99,
                                    101, 58, 32, 111, 105, 100, 61,
                                ];
                                let value = order_id.clone();
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[44, 32, 115, 111, 117, 114, 99, 101, 61]);
                                let value = inference_source.clone();
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[44, 32, 116, 121, 112, 101, 61]);
                                let value = inferred_type.clone();
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[44, 32, 112, 114, 105, 99, 101, 61]);
                                let value = inferred_price.clone();
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[
                                    44, 32, 119, 115, 76, 105, 109, 105, 116, 80, 120, 61,
                                ]);
                                let value = {
                                    let object = order_data.clone();
                                    h.get(&object, "limitPx")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[]);
                                h.literal(&JsString(units))
                            };
                            let arg1 = {
                                let object = h.global("LogSeverity")?;
                                h.get(&object, "Debug")?
                            };
                            h.call(&function, &receiver, &[arg0, arg1])?
                        };
                    }
                    let is_unconfirmed_placeholder = {
                        let left = inference_source.clone();
                        let right = lit(h, "hip3-unconfirmed-placeholder");
                        h.boolean(h.strict_equal(&left, &right))
                    };
                    let order_status = {
                        let condition = is_unconfirmed_placeholder.clone();
                        if h.truthy(&condition) {
                            {
                                let object = h.global("OrderStatus")?;
                                h.get(&object, "Creating")?
                            }
                        } else {
                            {
                                let object = h.global("OrderStatus")?;
                                h.get(&object, "Open")?
                            }
                        }
                    };
                    let mut inferred_time_in_force = {
                        let function = h.global("parseHyperliquidOrderTimeInForce")?;
                        let receiver = h.undefined();
                        let arg0 = {
                            let object = order_data.clone();
                            h.get(&object, "tif")?
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                    if {
                        let condition = {
                            let value = inferred_time_in_force.clone();
                            h.boolean(!h.truthy(&value))
                        };
                        h.truthy(&condition)
                    } {
                        inferred_time_in_force = {
                            let left = {
                                'optional: {
                                    let mut object = existing_orders_by_id.clone();
                                    object = {
                                        let key = order_id.clone();
                                        h.index(&object, &key)?
                                    };
                                    if h.is_nullish(&object) {
                                        break 'optional h.undefined();
                                    }
                                    object = h.get(&object, "timeInForce")?;
                                    object
                                }
                            };
                            if !h.is_nullish(&left) {
                                left
                            } else {
                                {
                                    let receiver = {
                                        let object = this_value.clone();
                                        h.get(&object, "parent")?
                                    };
                                    let function =
                                        h.get(&receiver, "getPlacedLimitOrderTimeInForce")?;
                                    let arg0 = order_id.clone();
                                    h.call(&function, &receiver, &[arg0])?
                                }
                            }
                        };
                    }
                    if {
                        let condition = {
                            let left = {
                                let value = inferred_time_in_force.clone();
                                h.boolean(!h.truthy(&value))
                            };
                            if !h.truthy(&left) {
                                left
                            } else {
                                {
                                    let left = inferred_type.clone();
                                    let right = {
                                        let object = h.global("OrderType")?;
                                        h.get(&object, "Limit")?
                                    };
                                    h.boolean(h.strict_equal(&left, &right))
                                }
                            }
                        };
                        h.truthy(&condition)
                    } {
                        let _ = {
                            let receiver = {
                                let object = this_value.clone();
                                h.get(&object, "parent")?
                            };
                            let function = h.get(&receiver, "scheduleOrderRefresh")?;
                            h.call(&function, &receiver, &[])?
                        };
                    }
                    let order = {
                        let object = h.object()?;
                        let value = order_id.clone();
                        h.define_data(&object, "id", &value)?;
                        let value = order_status.clone();
                        h.define_data(&object, "status", &value)?;
                        let value = {
                            let object = market.clone();
                            h.get(&object, "symbol")?
                        };
                        h.define_data(&object, "symbol", &value)?;
                        let value = inferred_type.clone();
                        h.define_data(&object, "type", &value)?;
                        let value = {
                            let condition = {
                                let left = {
                                    let object = order_data.clone();
                                    h.get(&object, "side")?
                                };
                                let right = lit(h, "B");
                                h.boolean(h.strict_equal(&left, &right))
                            };
                            if h.truthy(&condition) {
                                {
                                    let object = h.global("OrderSide")?;
                                    h.get(&object, "Buy")?
                                }
                            } else {
                                {
                                    let object = h.global("OrderSide")?;
                                    h.get(&object, "Sell")?
                                }
                            }
                        };
                        h.define_data(&object, "side", &value)?;
                        let value = inferred_price.clone();
                        h.define_data(&object, "price", &value)?;
                        let value = {
                            let value = {
                                let object = order_data.clone();
                                h.get(&object, "origSz")?
                            };
                            h.position_unary("plus", &value)?
                        };
                        h.define_data(&object, "amount", &value)?;
                        let value = {
                            let left = {
                                let value = {
                                    let object = order_data.clone();
                                    h.get(&object, "origSz")?
                                };
                                h.position_unary("plus", &value)?
                            };
                            let right = {
                                let value = {
                                    let object = order_data.clone();
                                    h.get(&object, "sz")?
                                };
                                h.position_unary("plus", &value)?
                            };
                            h.position_operator("subtract", &left, &right)?
                        };
                        h.define_data(&object, "filled", &value)?;
                        let value = {
                            let value = {
                                let object = order_data.clone();
                                h.get(&object, "sz")?
                            };
                            h.position_unary("plus", &value)?
                        };
                        h.define_data(&object, "remaining", &value)?;
                        let value = {
                            let function = h.global("Boolean")?;
                            let receiver = h.undefined();
                            let arg0 = {
                                let object = order_data.clone();
                                h.get(&object, "reduceOnly")?
                            };
                            h.call(&function, &receiver, &[arg0])?
                        };
                        h.define_data(&object, "reduceOnly", &value)?;
                        let value = {
                            let receiver = h.global("virtualClock")?;
                            let function = h.get(&receiver, "getCurrentTimeMs")?;
                            h.call(&function, &receiver, &[])?
                        };
                        h.define_data(&object, "lastUpdate", &value)?;
                        let value = inferred_time_in_force.clone();
                        h.define_data(&object, "timeInForce", &value)?;
                        let value = order_data.clone();
                        h.define_data(&object, "info", &value)?;
                        object
                    };
                    let _ = {
                        let receiver = {
                            let object = {
                                let object = this_value.clone();
                                h.get(&object, "parent")?
                            };
                            h.get(&object, "store")?
                        };
                        let function = h.get(&receiver, "addOrUpdateOrder")?;
                        let arg0 = order.clone();
                        h.call(&function, &receiver, &[arg0])?
                    };
                }
                Ok(())
            })();
            if let Err(error) = step {
                h.close_after_throw(&iterator);
                return Err(error);
            }
        }
    }
    Ok(h.undefined())
}
/// Source: hyperliquid.ws-private.ts::applyRealizedPnlToCurrentPosition
pub fn apply_realized_pnl_to_current_position<H: PositionOrderHost>(
    h: &mut H,
    this_value: &H::Value,
    coin: &H::Value,
    side: &H::Value,
    markets_by_id: &H::Value,
) -> Result<H::Value, H::Error> {
    let coin = coin.clone();
    let side = side.clone();
    let markets_by_id = markets_by_id.clone();
    let symbol = {
        let receiver = this_value.clone();
        let function = h.get(&receiver, "resolveSymbolFromCoin")?;
        let arg0 = coin.clone();
        let arg1 = markets_by_id.clone();
        h.call(&function, &receiver, &[arg0, arg1])?
    };
    let realized_pnl = {
        let receiver = this_value.clone();
        let function = h.get(&receiver, "getRealizedPnlForPosition")?;
        let arg0 = symbol.clone();
        let arg1 = side.clone();
        h.call(&function, &receiver, &[arg0, arg1])?
    };
    if {
        let condition = {
            let left = realized_pnl.clone();
            let right = h.undefined();
            h.boolean(h.strict_equal(&left, &right))
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    let current_position = {
        let receiver = {
            let object = {
                let object = this_value.clone();
                h.get(&object, "store")?
            };
            h.get(&object, "positions")?
        };
        let function = h.get(&receiver, "find")?;
        let arg0 = h.position_callback(PositionCallback::ApplyRealizedPnlToCurrentPosition0([
            symbol.clone(),
            side.clone(),
        ]))?;
        h.call(&function, &receiver, &[arg0])?
    };
    if {
        let condition = {
            let value = current_position.clone();
            h.boolean(!h.truthy(&value))
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    let _ = {
        let receiver = {
            let object = {
                let object = this_value.clone();
                h.get(&object, "parent")?
            };
            h.get(&object, "store")?
        };
        let function = h.get(&receiver, "updatePosition")?;
        let arg0 = current_position.clone();
        let arg1 = {
            let object = h.object()?;
            let value = realized_pnl.clone();
            h.define_data(&object, "realizedPnl", &value)?;
            object
        };
        h.call(&function, &receiver, &[arg0, arg1])?
    };
    Ok(h.undefined())
}
/// Source: hyperliquid.exchange.ts::generateFakePositionsFromMarkets
pub fn generate_fake_positions_from_markets<H: PositionOrderHost>(
    h: &mut H,
    this_value: &H::Value,
    markets: &H::Value,
) -> Result<H::Value, H::Error> {
    let markets = markets.clone();
    let fake_positions = {
        let receiver = markets.clone();
        let function = h.get(&receiver, "map")?;
        let arg0 = h.position_callback(PositionCallback::GenerateFakePositionsFromMarkets0([
            this_value.clone(),
        ]))?;
        h.call(&function, &receiver, &[arg0])?
    };
    return Ok({
        let receiver = fake_positions.clone();
        let function = h.get(&receiver, "flat")?;
        h.call(&function, &receiver, &[])?
    });
}
/// Source: hyperliquid.exchange.ts::syncBalanceFromLivePositions
pub fn sync_balance_from_live_positions<H: PositionOrderHost>(
    h: &mut H,
    this_value: &H::Value,
    updated_symbols: &H::Value,
) -> Result<H::Value, H::Error> {
    let updated_symbols = updated_symbols.clone();
    if {
        let condition = {
            let object = this_value.clone();
            h.get(&object, "isPublicDataOnly")?
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    let balance = {
        let object = {
            let object = this_value.clone();
            h.get(&object, "store")?
        };
        h.get(&object, "balance")?
    };
    if {
        let condition = {
            let value = balance.clone();
            h.boolean(!h.truthy(&value))
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    let wallet_balance = {
        let function = h.global("Number")?;
        let receiver = h.undefined();
        let arg0 = {
            let object = balance.clone();
            h.get(&object, "total")?
        };
        h.call(&function, &receiver, &[arg0])?
    };
    let used_margin = {
        let function = h.global("Number")?;
        let receiver = h.undefined();
        let arg0 = {
            let object = balance.clone();
            h.get(&object, "used")?
        };
        h.call(&function, &receiver, &[arg0])?
    };
    if {
        let condition = {
            let left = {
                let value = {
                    let receiver = h.global("Number")?;
                    let function = h.get(&receiver, "isFinite")?;
                    let arg0 = wallet_balance.clone();
                    h.call(&function, &receiver, &[arg0])?
                };
                h.boolean(!h.truthy(&value))
            };
            if h.truthy(&left) {
                left
            } else {
                {
                    let value = {
                        let receiver = h.global("Number")?;
                        let function = h.get(&receiver, "isFinite")?;
                        let arg0 = used_margin.clone();
                        h.call(&function, &receiver, &[arg0])?
                    };
                    h.boolean(!h.truthy(&value))
                }
            }
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    let positions = {
        let object = {
            let object = this_value.clone();
            h.get(&object, "store")?
        };
        h.get(&object, "positions")?
    };
    let mut has_open_positions = h.boolean(false);
    let mut has_updated_open_position = {
        let left = updated_symbols.clone();
        let right = h.undefined();
        h.boolean(h.strict_equal(&left, &right))
    };
    {
        let value = positions.clone();
        let iterator = h.iterator(&value)?;
        while let Some(position) = h.next(&iterator)? {
            let step = (|| -> Result<(), H::Error> {
                let contracts = {
                    let function = h.global("Number")?;
                    let receiver = h.undefined();
                    let arg0 = {
                        let object = position.clone();
                        h.get(&object, "contracts")?
                    };
                    h.call(&function, &receiver, &[arg0])?
                };
                if {
                    let condition = {
                        let left = {
                            let value = {
                                let receiver = h.global("Number")?;
                                let function = h.get(&receiver, "isFinite")?;
                                let arg0 = contracts.clone();
                                h.call(&function, &receiver, &[arg0])?
                            };
                            h.boolean(!h.truthy(&value))
                        };
                        if h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = contracts.clone();
                                let right = h.number(0.0);
                                h.boolean(h.strict_equal(&left, &right))
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    return Ok(());
                }
                has_open_positions = h.boolean(true);
                if {
                    let condition = {
                        let left = updated_symbols.clone();
                        if !h.truthy(&left) {
                            left
                        } else {
                            {
                                let receiver = updated_symbols.clone();
                                let function = h.get(&receiver, "has")?;
                                let arg0 = {
                                    let object = position.clone();
                                    h.get(&object, "symbol")?
                                };
                                h.call(&function, &receiver, &[arg0])?
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    has_updated_open_position = h.boolean(true);
                }
                Ok(())
            })();
            if let Err(error) = step {
                h.close_after_throw(&iterator);
                return Err(error);
            }
        }
    }
    if {
        let condition = {
            let left = updated_symbols.clone();
            if !h.truthy(&left) {
                left
            } else {
                {
                    let value = has_updated_open_position.clone();
                    h.boolean(!h.truthy(&value))
                }
            }
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    if {
        let condition = {
            let left = {
                let value = has_open_positions.clone();
                h.boolean(!h.truthy(&value))
            };
            if !h.truthy(&left) {
                left
            } else {
                {
                    let left = {
                        let function = h.global("Number")?;
                        let receiver = h.undefined();
                        let arg0 = {
                            let object = balance.clone();
                            h.get(&object, "upnl")?
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                    let right = h.number(0.0);
                    h.boolean(h.strict_equal(&left, &right))
                }
            }
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    let mut next_upnl = h.number(0.0);
    {
        let value = positions.clone();
        let iterator = h.iterator(&value)?;
        while let Some(position) = h.next(&iterator)? {
            let step = (|| -> Result<(), H::Error> {
                let contracts = {
                    let function = h.global("Number")?;
                    let receiver = h.undefined();
                    let arg0 = {
                        let object = position.clone();
                        h.get(&object, "contracts")?
                    };
                    h.call(&function, &receiver, &[arg0])?
                };
                if {
                    let condition = {
                        let left = {
                            let value = {
                                let receiver = h.global("Number")?;
                                let function = h.get(&receiver, "isFinite")?;
                                let arg0 = contracts.clone();
                                h.call(&function, &receiver, &[arg0])?
                            };
                            h.boolean(!h.truthy(&value))
                        };
                        if h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = contracts.clone();
                                let right = h.number(0.0);
                                h.boolean(h.strict_equal(&left, &right))
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    return Ok(());
                }
                let abs_contracts = {
                    let receiver = h.global("Math")?;
                    let function = h.get(&receiver, "abs")?;
                    let arg0 = contracts.clone();
                    h.call(&function, &receiver, &[arg0])?
                };
                let ticker = {
                    let object = {
                        let object = {
                            let object = this_value.clone();
                            h.get(&object, "store")?
                        };
                        h.get(&object, "tickersBySymbol")?
                    };
                    {
                        let key = {
                            let object = position.clone();
                            h.get(&object, "symbol")?
                        };
                        h.index(&object, &key)?
                    }
                };
                let last_price = {
                    let function = h.global("Number")?;
                    let receiver = h.undefined();
                    let arg0 = {
                        'optional: {
                            let mut object = ticker.clone();
                            if h.is_nullish(&object) {
                                break 'optional h.undefined();
                            }
                            object = h.get(&object, "last")?;
                            object
                        }
                    };
                    h.call(&function, &receiver, &[arg0])?
                };
                let entry_price = {
                    let function = h.global("Number")?;
                    let receiver = h.undefined();
                    let arg0 = {
                        let object = position.clone();
                        h.get(&object, "entryPrice")?
                    };
                    h.call(&function, &receiver, &[arg0])?
                };
                if {
                    let condition = {
                        let left = {
                            let left = {
                                let left = {
                                    let receiver = h.global("Number")?;
                                    let function = h.get(&receiver, "isFinite")?;
                                    let arg0 = last_price.clone();
                                    h.call(&function, &receiver, &[arg0])?
                                };
                                if !h.truthy(&left) {
                                    left
                                } else {
                                    {
                                        let left = last_price.clone();
                                        let right = h.number(0.0);
                                        h.position_operator("greater", &left, &right)?
                                    }
                                }
                            };
                            if !h.truthy(&left) {
                                left
                            } else {
                                {
                                    let receiver = h.global("Number")?;
                                    let function = h.get(&receiver, "isFinite")?;
                                    let arg0 = entry_price.clone();
                                    h.call(&function, &receiver, &[arg0])?
                                }
                            }
                        };
                        if !h.truthy(&left) {
                            left
                        } else {
                            {
                                let left = entry_price.clone();
                                let right = h.number(0.0);
                                h.position_operator("greater", &left, &right)?
                            }
                        }
                    };
                    h.truthy(&condition)
                } {
                    next_upnl = {
                        let left = next_upnl.clone();
                        let right = {
                            let left = {
                                let condition = {
                                    let left = {
                                        let object = position.clone();
                                        h.get(&object, "side")?
                                    };
                                    let right = {
                                        let object = h.global("PositionSide")?;
                                        h.get(&object, "Long")?
                                    };
                                    h.boolean(h.strict_equal(&left, &right))
                                };
                                if h.truthy(&condition) {
                                    {
                                        let left = last_price.clone();
                                        let right = entry_price.clone();
                                        h.position_operator("subtract", &left, &right)?
                                    }
                                } else {
                                    {
                                        let left = entry_price.clone();
                                        let right = last_price.clone();
                                        h.position_operator("subtract", &left, &right)?
                                    }
                                }
                            };
                            let right = abs_contracts.clone();
                            h.position_operator("multiply", &left, &right)?
                        };
                        h.position_operator("add", &left, &right)?
                    };
                } else {
                    let fallback_upnl = {
                        let function = h.global("Number")?;
                        let receiver = h.undefined();
                        let arg0 = {
                            let object = position.clone();
                            h.get(&object, "unrealizedPnl")?
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                    if {
                        let condition = {
                            let receiver = h.global("Number")?;
                            let function = h.get(&receiver, "isFinite")?;
                            let arg0 = fallback_upnl.clone();
                            h.call(&function, &receiver, &[arg0])?
                        };
                        h.truthy(&condition)
                    } {
                        next_upnl = {
                            let left = next_upnl.clone();
                            let right = fallback_upnl.clone();
                            h.position_operator("add", &left, &right)?
                        };
                    }
                }
                Ok(())
            })();
            if let Err(error) = step {
                h.close_after_throw(&iterator);
                return Err(error);
            }
        }
    }
    let next_free = {
        let left = {
            let left = wallet_balance.clone();
            let right = next_upnl.clone();
            h.position_operator("add", &left, &right)?
        };
        let right = used_margin.clone();
        h.position_operator("subtract", &left, &right)?
    };
    let prev_upnl = {
        let function = h.global("Number")?;
        let receiver = h.undefined();
        let arg0 = {
            let object = balance.clone();
            h.get(&object, "upnl")?
        };
        h.call(&function, &receiver, &[arg0])?
    };
    let prev_free = {
        let function = h.global("Number")?;
        let receiver = h.undefined();
        let arg0 = {
            let object = balance.clone();
            h.get(&object, "free")?
        };
        h.call(&function, &receiver, &[arg0])?
    };
    let epsilon = h.number(1e-8);
    if {
        let condition = {
            let left = {
                let left = {
                    let left = {
                        let receiver = h.global("Number")?;
                        let function = h.get(&receiver, "isFinite")?;
                        let arg0 = prev_upnl.clone();
                        h.call(&function, &receiver, &[arg0])?
                    };
                    if !h.truthy(&left) {
                        left
                    } else {
                        {
                            let receiver = h.global("Number")?;
                            let function = h.get(&receiver, "isFinite")?;
                            let arg0 = prev_free.clone();
                            h.call(&function, &receiver, &[arg0])?
                        }
                    }
                };
                if !h.truthy(&left) {
                    left
                } else {
                    {
                        let left = {
                            let receiver = h.global("Math")?;
                            let function = h.get(&receiver, "abs")?;
                            let arg0 = {
                                let left = prev_upnl.clone();
                                let right = next_upnl.clone();
                                h.position_operator("subtract", &left, &right)?
                            };
                            h.call(&function, &receiver, &[arg0])?
                        };
                        let right = epsilon.clone();
                        h.position_operator("less", &left, &right)?
                    }
                }
            };
            if !h.truthy(&left) {
                left
            } else {
                {
                    let left = {
                        let receiver = h.global("Math")?;
                        let function = h.get(&receiver, "abs")?;
                        let arg0 = {
                            let left = prev_free.clone();
                            let right = next_free.clone();
                            h.position_operator("subtract", &left, &right)?
                        };
                        h.call(&function, &receiver, &[arg0])?
                    };
                    let right = epsilon.clone();
                    h.position_operator("less", &left, &right)?
                }
            }
        };
        h.truthy(&condition)
    } {
        return Ok(h.undefined());
    }
    if {
        let condition = {
            let left = {
                let receiver = this_value.clone();
                let function = h.get(&receiver, "shouldUseSpotCollateralBalance")?;
                h.call(&function, &receiver, &[])?
            };
            if !h.truthy(&left) {
                left
            } else {
                {
                    let object = this_value.clone();
                    h.get(&object, "hasSpotCollateralSnapshot")?
                }
            }
        };
        h.truthy(&condition)
    } {
        {
            let object = this_value.clone();
            let value = next_upnl.clone();
            h.set(&object, "latestCorePerpsUpnl", &value)?;
        }
        let _ = {
            let receiver = this_value.clone();
            let function = h.get(&receiver, "applyBalanceFromCurrentMode")?;
            h.call(&function, &receiver, &[])?
        };
        return Ok(h.undefined());
    }
    let _ = {
        let receiver = {
            let object = this_value.clone();
            h.get(&object, "store")?
        };
        let function = h.get(&receiver, "update")?;
        let arg0 = {
            let object = h.object()?;
            let value = {
                let object = h.object()?;
                let value = balance.clone();
                h.spread_into(&object, &value)?;
                let value = next_upnl.clone();
                h.define_data(&object, "upnl", &value)?;
                let value = next_free.clone();
                h.define_data(&object, "free", &value)?;
                object
            };
            h.define_data(&object, "balance", &value)?;
            object
        };
        h.call(&function, &receiver, &[arg0])?
    };
    Ok(h.undefined())
}
/// Source: hyperliquid.exchange.ts::fetchHip3Positions
pub async fn fetch_hip3_positions<H: PositionOrderHost>(
    h: &mut H,
    this_value: &H::Value,
    dex_name: &H::Value,
) -> Result<H::Value, H::Error> {
    let dex_name = dex_name.clone();
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
    {
        let outcome: Result<H::Value, H::Error> = async {
            let data = {
                let object = {
                    let value = {
                        let receiver = {
                            let object = this_value.clone();
                            h.get(&object, "xhr")?
                        };
                        let function = h.get(&receiver, "post")?;
                        let arg0 = {
                            let object = h.global("ENDPOINTS")?;
                            h.get(&object, "INFO")?
                        };
                        let arg1 = {
                            let object = h.object()?;
                            let value = {
                                let object = h.global("INFO_TYPES")?;
                                h.get(&object, "PERPS_CLEARINGHOUSE_STATE")?
                            };
                            h.define_data(&object, "type", &value)?;
                            let value = user.clone();
                            h.define_data(&object, "user", &value)?;
                            let value = dex_name.clone();
                            h.define_data(&object, "dex", &value)?;
                            object
                        };
                        let arg2 = {
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
                        h.call(&function, &receiver, &[arg0, arg1, arg2])?
                    };
                    h.await_value(value).await?
                };
                h.get(&object, "data")?
            };
            let markets_by_id = {
                let object = {
                    let object = this_value.clone();
                    h.get(&object, "store")?
                };
                h.get(&object, "marketsById")?
            };
            let new_positions = {
                let array = h.array()?;
                array
            };
            if {
                let condition = {
                    let left = {
                        let value = {
                            'optional: {
                                let mut object = data.clone();
                                if h.is_nullish(&object) {
                                    break 'optional h.undefined();
                                }
                                object = h.get(&object, "assetPositions")?;
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
                                let object = {
                                    let object = data.clone();
                                    h.get(&object, "assetPositions")?
                                };
                                h.get(&object, "length")?
                            };
                            let right = h.number(0.0);
                            h.boolean(h.strict_equal(&left, &right))
                        }
                    }
                };
                h.truthy(&condition)
            } {
                let had_positions_for_dex = {
                    let receiver = {
                        let object = {
                            let object = this_value.clone();
                            h.get(&object, "store")?
                        };
                        h.get(&object, "positions")?
                    };
                    let function = h.get(&receiver, "some")?;
                    let arg0 = h.position_callback(PositionCallback::FetchHip3Positions0([
                        this_value.clone(),
                        dex_name.clone(),
                    ]))?;
                    h.call(&function, &receiver, &[arg0])?
                };
                let current_positions = {
                    let receiver = {
                        let object = {
                            let object = this_value.clone();
                            h.get(&object, "store")?
                        };
                        h.get(&object, "positions")?
                    };
                    let function = h.get(&receiver, "filter")?;
                    let arg0 = h.position_callback(PositionCallback::FetchHip3Positions1([
                        this_value.clone(),
                        dex_name.clone(),
                    ]))?;
                    h.call(&function, &receiver, &[arg0])?
                };
                let all_positions = {
                    let function = h.global("uniqBy")?;
                    let receiver = h.undefined();
                    let arg0 = {
                        let array = h.array()?;
                        let value = current_positions.clone();
                        h.append_position_spread(&array, &value)?;
                        let value = {
                            let receiver = this_value.clone();
                            let function = h.get(&receiver, "getFakePositions")?;
                            h.call(&function, &receiver, &[])?
                        };
                        h.append_position_spread(&array, &value)?;
                        array
                    };
                    let arg1 = lit(h, "id");
                    h.call(&function, &receiver, &[arg0, arg1])?
                };
                let _ = {
                    let receiver = {
                        let object = this_value.clone();
                        h.get(&object, "store")?
                    };
                    let function = h.get(&receiver, "addOrUpdatePositionsFull")?;
                    let arg0 = all_positions.clone();
                    h.call(&function, &receiver, &[arg0])?
                };
                let _ = {
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "log")?;
                    let arg0 = {
                        let mut units: Vec<u16> = vec![
                            67, 108, 101, 97, 114, 101, 100, 32, 72, 73, 80, 45, 51, 32, 112, 111,
                            115, 105, 116, 105, 111, 110, 115, 32, 102, 111, 114, 32, 100, 101,
                            120, 58, 32,
                        ];
                        let value = dex_name.clone();
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[
                            32, 40, 101, 109, 112, 116, 121, 32, 114, 101, 115, 112, 111, 110, 115,
                            101, 41,
                        ]);
                        h.literal(&JsString(units))
                    };
                    let arg1 = {
                        let object = h.global("LogSeverity")?;
                        h.get(&object, "Info")?
                    };
                    h.call(&function, &receiver, &[arg0, arg1])?
                };
                if {
                    let condition = had_positions_for_dex.clone();
                    h.truthy(&condition)
                } {
                    let _ = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "scheduleOrderRefresh")?;
                        h.call(&function, &receiver, &[])?
                    };
                }
                return Ok(h.undefined());
            }
            {
                let value = {
                    let object = data.clone();
                    h.get(&object, "assetPositions")?
                };
                let iterator = h.iterator(&value)?;
                while let Some(asset_position) = h.next(&iterator)? {
                    let step = (|| -> Result<(), H::Error> {
                        let pos = {
                            let object = asset_position.clone();
                            h.get(&object, "position")?
                        };
                        let szi = {
                            let function = h.global("parseFloat")?;
                            let receiver = h.undefined();
                            let arg0 = {
                                let object = pos.clone();
                                h.get(&object, "szi")?
                            };
                            h.call(&function, &receiver, &[arg0])?
                        };
                        let _ = {
                            let receiver = this_value.clone();
                            let function = h.get(&receiver, "log")?;
                            let arg0 = {
                                let mut units: Vec<u16> = vec![
                                    80, 114, 111, 99, 101, 115, 115, 105, 110, 103, 32, 72, 73, 80,
                                    45, 51, 32, 112, 111, 115, 105, 116, 105, 111, 110, 58, 32, 99,
                                    111, 105, 110, 61,
                                ];
                                let value = {
                                    let object = pos.clone();
                                    h.get(&object, "coin")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[44, 32, 115, 122, 105, 61]);
                                let value = szi.clone();
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[
                                    44, 32, 101, 110, 116, 114, 121, 80, 120, 61,
                                ]);
                                let value = {
                                    let object = pos.clone();
                                    h.get(&object, "entryPx")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[]);
                                h.literal(&JsString(units))
                            };
                            let arg1 = {
                                let object = h.global("LogSeverity")?;
                                h.get(&object, "Debug")?
                            };
                            h.call(&function, &receiver, &[arg0, arg1])?
                        };
                        if {
                            let condition = {
                                let left = szi.clone();
                                let right = h.number(0.0);
                                h.boolean(h.strict_equal(&left, &right))
                            };
                            h.truthy(&condition)
                        } {
                            let _ = {
                                let receiver = this_value.clone();
                                let function = h.get(&receiver, "log")?;
                                let arg0 = {
                                    let mut units: Vec<u16> = vec![
                                        83, 107, 105, 112, 112, 105, 110, 103, 32, 122, 101, 114,
                                        111, 32, 112, 111, 115, 105, 116, 105, 111, 110, 32, 102,
                                        111, 114, 32,
                                    ];
                                    let value = {
                                        let object = pos.clone();
                                        h.get(&object, "coin")?
                                    };
                                    units.extend(h.template_string(&value)?.0);
                                    units.extend_from_slice(&[]);
                                    h.literal(&JsString(units))
                                };
                                let arg1 = {
                                    let object = h.global("LogSeverity")?;
                                    h.get(&object, "Debug")?
                                };
                                h.call(&function, &receiver, &[arg0, arg1])?
                            };
                            return Ok(());
                        }
                        let mut market = {
                            let object = markets_by_id.clone();
                            {
                                let key = {
                                    let object = pos.clone();
                                    h.get(&object, "coin")?
                                };
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
                            market = {
                                let receiver = {
                                    let object = this_value.clone();
                                    h.get(&object, "marketsLowerCaseMap")?
                                };
                                let function = h.get(&receiver, "get")?;
                                let arg0 = {
                                    let receiver = {
                                        let object = pos.clone();
                                        h.get(&object, "coin")?
                                    };
                                    let function = h.get(&receiver, "toLowerCase")?;
                                    h.call(&function, &receiver, &[])?
                                };
                                h.call(&function, &receiver, &[arg0])?
                            };
                            if {
                                let condition = market.clone();
                                h.truthy(&condition)
                            } {
                                let _ = {
                                    let receiver = this_value.clone();
                                    let function = h.get(&receiver, "log")?;
                                    let arg0 = {
                                        let mut units: Vec<u16> = vec![
                                            70, 111, 117, 110, 100, 32, 109, 97, 114, 107, 101,
                                            116, 32, 118, 105, 97, 32, 99, 97, 115, 101, 45, 105,
                                            110, 115, 101, 110, 115, 105, 116, 105, 118, 101, 32,
                                            108, 111, 111, 107, 117, 112, 58, 32, 112, 111, 115,
                                            46, 99, 111, 105, 110, 61, 34,
                                        ];
                                        let value = {
                                            let object = pos.clone();
                                            h.get(&object, "coin")?
                                        };
                                        units.extend(h.template_string(&value)?.0);
                                        units.extend_from_slice(&[
                                            34, 32, 109, 97, 116, 99, 104, 101, 100, 32, 109, 97,
                                            114, 107, 101, 116, 46, 105, 100, 61, 34,
                                        ]);
                                        let value = {
                                            let object = market.clone();
                                            h.get(&object, "id")?
                                        };
                                        units.extend(h.template_string(&value)?.0);
                                        units.extend_from_slice(&[34]);
                                        h.literal(&JsString(units))
                                    };
                                    let arg1 = {
                                        let object = h.global("LogSeverity")?;
                                        h.get(&object, "Info")?
                                    };
                                    h.call(&function, &receiver, &[arg0, arg1])?
                                };
                            }
                        }
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
                                let arg0 = {
                                    let mut units: Vec<u16> = vec![
                                        72, 73, 80, 45, 51, 32, 112, 111, 115, 105, 116, 105, 111,
                                        110, 32, 102, 111, 114, 32, 117, 110, 107, 110, 111, 119,
                                        110, 32, 109, 97, 114, 107, 101, 116, 58, 32,
                                    ];
                                    let value = {
                                        let object = pos.clone();
                                        h.get(&object, "coin")?
                                    };
                                    units.extend(h.template_string(&value)?.0);
                                    units.extend_from_slice(&[
                                        46, 32, 65, 118, 97, 105, 108, 97, 98, 108, 101, 32, 72,
                                        73, 80, 45, 51, 32, 109, 97, 114, 107, 101, 116, 115, 58,
                                        32,
                                    ]);
                                    let value = {
                                        let receiver = {
                                            let receiver = {
                                                let receiver = h.global("Object")?;
                                                let function = h.get(&receiver, "keys")?;
                                                let arg0 = markets_by_id.clone();
                                                h.call(&function, &receiver, &[arg0])?
                                            };
                                            let function = h.get(&receiver, "filter")?;
                                            let arg0 = h.position_callback(
                                                PositionCallback::FetchHip3Positions2([]),
                                            )?;
                                            h.call(&function, &receiver, &[arg0])?
                                        };
                                        let function = h.get(&receiver, "join")?;
                                        let arg0 = lit(h, ", ");
                                        h.call(&function, &receiver, &[arg0])?
                                    };
                                    units.extend(h.template_string(&value)?.0);
                                    units.extend_from_slice(&[]);
                                    h.literal(&JsString(units))
                                };
                                let arg1 = {
                                    let object = h.global("LogSeverity")?;
                                    h.get(&object, "Warning")?
                                };
                                h.call(&function, &receiver, &[arg0, arg1])?
                            };
                            return Ok(());
                        }
                        let side = {
                            let condition = {
                                let left = szi.clone();
                                let right = h.number(0.0);
                                h.position_operator("greater", &left, &right)?
                            };
                            if h.truthy(&condition) {
                                {
                                    let object = h.global("PositionSide")?;
                                    h.get(&object, "Long")?
                                }
                            } else {
                                {
                                    let object = h.global("PositionSide")?;
                                    h.get(&object, "Short")?
                                }
                            }
                        };
                        let position = {
                            let object = h.object()?;
                            let value = {
                                let receiver = this_value.clone();
                                let function =
                                    h.get(&receiver, "_getSymbolConfigurationFromAssetData")?;
                                let arg0 = {
                                    let object = market.clone();
                                    h.get(&object, "id")?
                                };
                                h.call(&function, &receiver, &[arg0])?
                            };
                            h.spread_into(&object, &value)?;
                            let value = {
                                let function = h.global("createPositionId")?;
                                let receiver = h.undefined();
                                let arg0 = {
                                    let object = market.clone();
                                    h.get(&object, "symbol")?
                                };
                                let arg1 = side.clone();
                                let arg2 = {
                                    let object = h.global("PositionMode")?;
                                    h.get(&object, "OneWay")?
                                };
                                h.call(&function, &receiver, &[arg0, arg1, arg2])?
                            };
                            h.define_data(&object, "id", &value)?;
                            let value = {
                                let object = market.clone();
                                h.get(&object, "symbol")?
                            };
                            h.define_data(&object, "symbol", &value)?;
                            let value = side.clone();
                            h.define_data(&object, "side", &value)?;
                            let value = {
                                let function = h.global("parseFloat")?;
                                let receiver = h.undefined();
                                let arg0 = {
                                    let object = pos.clone();
                                    h.get(&object, "entryPx")?
                                };
                                h.call(&function, &receiver, &[arg0])?
                            };
                            h.define_data(&object, "entryPrice", &value)?;
                            let value = {
                                let function = h.global("parseFloat")?;
                                let receiver = h.undefined();
                                let arg0 = {
                                    let object = pos.clone();
                                    h.get(&object, "positionValue")?
                                };
                                h.call(&function, &receiver, &[arg0])?
                            };
                            h.define_data(&object, "notional", &value)?;
                            let value = {
                                let object = {
                                    let object = pos.clone();
                                    h.get(&object, "leverage")?
                                };
                                h.get(&object, "value")?
                            };
                            h.define_data(&object, "leverage", &value)?;
                            let value = {
                                let function = h.global("parseFloat")?;
                                let receiver = h.undefined();
                                let arg0 = {
                                    let object = pos.clone();
                                    h.get(&object, "unrealizedPnl")?
                                };
                                h.call(&function, &receiver, &[arg0])?
                            };
                            h.define_data(&object, "unrealizedPnl", &value)?;
                            let value = {
                                let receiver = h.global("Math")?;
                                let function = h.get(&receiver, "abs")?;
                                let arg0 = szi.clone();
                                h.call(&function, &receiver, &[arg0])?
                            };
                            h.define_data(&object, "contracts", &value)?;
                            let value = {
                                let condition = {
                                    let object = pos.clone();
                                    h.get(&object, "liquidationPx")?
                                };
                                if h.truthy(&condition) {
                                    {
                                        let function = h.global("parseFloat")?;
                                        let receiver = h.undefined();
                                        let arg0 = {
                                            let object = pos.clone();
                                            h.get(&object, "liquidationPx")?
                                        };
                                        h.call(&function, &receiver, &[arg0])?
                                    }
                                } else {
                                    h.number(0.0)
                                }
                            };
                            h.define_data(&object, "liquidationPrice", &value)?;
                            let value = {
                                let condition = {
                                    let object = pos.clone();
                                    h.get(&object, "marginUsed")?
                                };
                                if h.truthy(&condition) {
                                    {
                                        let function = h.global("parseFloat")?;
                                        let receiver = h.undefined();
                                        let arg0 = {
                                            let object = pos.clone();
                                            h.get(&object, "marginUsed")?
                                        };
                                        h.call(&function, &receiver, &[arg0])?
                                    }
                                } else {
                                    h.undefined()
                                }
                            };
                            h.define_data(&object, "marginSize", &value)?;
                            let value = {
                                let object = h.global("PositionMode")?;
                                h.get(&object, "OneWay")?
                            };
                            h.define_data(&object, "positionMode", &value)?;
                            let value = {
                                let condition = {
                                    let left = {
                                        let object = {
                                            let object = pos.clone();
                                            h.get(&object, "leverage")?
                                        };
                                        h.get(&object, "type")?
                                    };
                                    let right = lit(h, "cross");
                                    h.boolean(h.strict_equal(&left, &right))
                                };
                                if h.truthy(&condition) {
                                    {
                                        let object = h.global("MarginType")?;
                                        h.get(&object, "Cross")?
                                    }
                                } else {
                                    {
                                        let object = h.global("MarginType")?;
                                        h.get(&object, "Isolated")?
                                    }
                                }
                            };
                            h.define_data(&object, "marginType", &value)?;
                            object
                        };
                        let _ = {
                            let receiver = this_value.clone();
                            let function = h.get(&receiver, "log")?;
                            let arg0 = {
                                let mut units: Vec<u16> = vec![
                                    67, 114, 101, 97, 116, 101, 100, 32, 112, 111, 115, 105, 116,
                                    105, 111, 110, 32, 111, 98, 106, 101, 99, 116, 58, 32, 105,
                                    100, 61,
                                ];
                                let value = {
                                    let object = position.clone();
                                    h.get(&object, "id")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[44, 32, 115, 121, 109, 98, 111, 108, 61]);
                                let value = {
                                    let object = position.clone();
                                    h.get(&object, "symbol")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[44, 32, 115, 105, 100, 101, 61]);
                                let value = {
                                    let object = position.clone();
                                    h.get(&object, "side")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[
                                    44, 32, 99, 111, 110, 116, 114, 97, 99, 116, 115, 61,
                                ]);
                                let value = {
                                    let object = position.clone();
                                    h.get(&object, "contracts")?
                                };
                                units.extend(h.template_string(&value)?.0);
                                units.extend_from_slice(&[]);
                                h.literal(&JsString(units))
                            };
                            let arg1 = {
                                let object = h.global("LogSeverity")?;
                                h.get(&object, "Info")?
                            };
                            h.call(&function, &receiver, &[arg0, arg1])?
                        };
                        let _ = {
                            let receiver = new_positions.clone();
                            let function = h.get(&receiver, "push")?;
                            let arg0 = position.clone();
                            h.call(&function, &receiver, &[arg0])?
                        };
                        Ok(())
                    })();
                    if let Err(error) = step {
                        h.close_after_throw(&iterator);
                        return Err(error);
                    }
                }
            }
            let had_positions_for_dex = {
                let receiver = {
                    let object = {
                        let object = this_value.clone();
                        h.get(&object, "store")?
                    };
                    h.get(&object, "positions")?
                };
                let function = h.get(&receiver, "some")?;
                let arg0 = h.position_callback(PositionCallback::FetchHip3Positions3([
                    this_value.clone(),
                    dex_name.clone(),
                ]))?;
                h.call(&function, &receiver, &[arg0])?
            };
            let current_positions = {
                let receiver = {
                    let object = {
                        let object = this_value.clone();
                        h.get(&object, "store")?
                    };
                    h.get(&object, "positions")?
                };
                let function = h.get(&receiver, "filter")?;
                let arg0 = h.position_callback(PositionCallback::FetchHip3Positions4([
                    this_value.clone(),
                    dex_name.clone(),
                ]))?;
                h.call(&function, &receiver, &[arg0])?
            };
            let _ = {
                let receiver = this_value.clone();
                let function = h.get(&receiver, "log")?;
                let arg0 = {
                    let mut units: Vec<u16> = vec![
                        85, 112, 100, 97, 116, 105, 110, 103, 32, 115, 116, 111, 114, 101, 58, 32,
                    ];
                    let value = {
                        let object = current_positions.clone();
                        h.get(&object, "length")?
                    };
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[
                        32, 101, 120, 105, 115, 116, 105, 110, 103, 32, 112, 111, 115, 105, 116,
                        105, 111, 110, 115, 32, 43, 32,
                    ]);
                    let value = {
                        let object = new_positions.clone();
                        h.get(&object, "length")?
                    };
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[
                        32, 110, 101, 119, 32, 72, 73, 80, 45, 51, 32, 112, 111, 115, 105, 116,
                        105, 111, 110, 115, 32, 102, 111, 114, 32,
                    ]);
                    let value = dex_name.clone();
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[]);
                    h.literal(&JsString(units))
                };
                let arg1 = {
                    let object = h.global("LogSeverity")?;
                    h.get(&object, "Info")?
                };
                h.call(&function, &receiver, &[arg0, arg1])?
            };
            let all_positions = {
                let function = h.global("uniqBy")?;
                let receiver = h.undefined();
                let arg0 = {
                    let array = h.array()?;
                    let value = current_positions.clone();
                    h.append_position_spread(&array, &value)?;
                    let value = new_positions.clone();
                    h.append_position_spread(&array, &value)?;
                    let value = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "getFakePositions")?;
                        h.call(&function, &receiver, &[])?
                    };
                    h.append_position_spread(&array, &value)?;
                    array
                };
                let arg1 = lit(h, "id");
                h.call(&function, &receiver, &[arg0, arg1])?
            };
            let _ = {
                let receiver = {
                    let object = this_value.clone();
                    h.get(&object, "store")?
                };
                let function = h.get(&receiver, "addOrUpdatePositionsFull")?;
                let arg0 = all_positions.clone();
                h.call(&function, &receiver, &[arg0])?
            };
            if {
                let condition = {
                    let left = {
                        let object = new_positions.clone();
                        h.get(&object, "length")?
                    };
                    let right = h.number(0.0);
                    h.position_operator("greater", &left, &right)?
                };
                h.truthy(&condition)
            } {
                let _ = {
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "log")?;
                    let arg0 = {
                        let mut units: Vec<u16> = vec![
                            83, 117, 99, 99, 101, 115, 115, 102, 117, 108, 108, 121, 32, 117, 112,
                            100, 97, 116, 101, 100, 32,
                        ];
                        let value = {
                            let object = new_positions.clone();
                            h.get(&object, "length")?
                        };
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[
                            32, 72, 73, 80, 45, 51, 32, 112, 111, 115, 105, 116, 105, 111, 110,
                            115, 32, 102, 111, 114, 32, 100, 101, 120, 58, 32,
                        ]);
                        let value = dex_name.clone();
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[]);
                        h.literal(&JsString(units))
                    };
                    let arg1 = {
                        let object = h.global("LogSeverity")?;
                        h.get(&object, "Info")?
                    };
                    h.call(&function, &receiver, &[arg0, arg1])?
                };
            } else {
                let _ = {
                    let receiver = this_value.clone();
                    let function = h.get(&receiver, "log")?;
                    let arg0 = {
                        let mut units: Vec<u16> = vec![
                            67, 108, 101, 97, 114, 101, 100, 32, 72, 73, 80, 45, 51, 32, 112, 111,
                            115, 105, 116, 105, 111, 110, 115, 32, 102, 111, 114, 32, 100, 101,
                            120, 58, 32,
                        ];
                        let value = dex_name.clone();
                        units.extend(h.template_string(&value)?.0);
                        units.extend_from_slice(&[
                            32, 40, 97, 108, 108, 32, 112, 111, 115, 105, 116, 105, 111, 110, 115,
                            32, 99, 108, 111, 115, 101, 100, 41,
                        ]);
                        h.literal(&JsString(units))
                    };
                    let arg1 = {
                        let object = h.global("LogSeverity")?;
                        h.get(&object, "Info")?
                    };
                    h.call(&function, &receiver, &[arg0, arg1])?
                };
                if {
                    let condition = had_positions_for_dex.clone();
                    h.truthy(&condition)
                } {
                    let _ = {
                        let receiver = this_value.clone();
                        let function = h.get(&receiver, "scheduleOrderRefresh")?;
                        h.call(&function, &receiver, &[])?
                    };
                }
            }
            Ok(h.undefined())
        }
        .await;
        if let Err(error) = outcome {
            let error = h.error_value(&error);
            let _ = {
                let receiver = this_value.clone();
                let function = h.get(&receiver, "log")?;
                let arg0 = {
                    let mut units: Vec<u16> = vec![
                        70, 97, 105, 108, 101, 100, 32, 116, 111, 32, 102, 101, 116, 99, 104, 32,
                        72, 73, 80, 45, 51, 32, 112, 111, 115, 105, 116, 105, 111, 110, 115, 32,
                        102, 111, 114, 32,
                    ];
                    let value = dex_name.clone();
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[58, 32]);
                    let value = error.clone();
                    units.extend(h.template_string(&value)?.0);
                    units.extend_from_slice(&[]);
                    h.literal(&JsString(units))
                };
                let arg1 = {
                    let object = h.global("LogSeverity")?;
                    h.get(&object, "Warning")?
                };
                h.call(&function, &receiver, &[arg0, arg1])?
            };
        }
    }
    Ok(h.undefined())
}
/// Source: hyperliquid.exchange.ts::parseOpenOrders
pub fn parse_open_orders<H: PositionOrderHost>(
    h: &mut H,
    this_value: &H::Value,
    open_orders: &H::Value,
) -> Result<H::Value, H::Error> {
    let open_orders = open_orders.clone();
    let markets_by_id = {
        let object = {
            let object = this_value.clone();
            h.get(&object, "store")?
        };
        h.get(&object, "marketsById")?
    };
    return Ok({
        let receiver = {
            let receiver = {
                let receiver = open_orders.clone();
                let function = h.get(&receiver, "filter")?;
                let arg0 =
                    h.position_callback(PositionCallback::ParseOpenOrders0([this_value.clone()]))?;
                h.call(&function, &receiver, &[arg0])?
            };
            let function = h.get(&receiver, "map")?;
            let arg0 = h.position_callback(PositionCallback::ParseOpenOrders1([
                markets_by_id.clone(),
                this_value.clone(),
            ]))?;
            h.call(&function, &receiver, &[arg0])?
        };
        let function = h.get(&receiver, "filter")?;
        let arg0 = h.position_callback(PositionCallback::ParseOpenOrders2([]))?;
        h.call(&function, &receiver, &[arg0])?
    });
}
