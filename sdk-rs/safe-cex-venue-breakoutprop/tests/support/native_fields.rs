// Direct native field observations: independent of Serialize field names.
impl Observe for LoginResponse {
    fn observe(&self) -> Value {
        json!({
        "sessionToken": self.session_token.observe(),
        "timeout": self.timeout.observe()
        })
    }
}
impl Observe for UserAccount {
    fn observe(&self) -> Value {
        json!({
        "owner": self.owner.observe(),
        "account": self.account.observe(),
        "version": self.version.observe(),
        "baseCurrency": self.base_currency.observe(),
        "registrationTime": self.registration_time.observe(),
        "accountStatus": self.account_status.observe(),
        "pricingStream": self.pricing_stream.observe(),
        "positionBased": self.position_based.observe()
        })
    }
}
impl Observe for UserDetail {
    fn observe(&self) -> Value {
        json!({
        "login": self.login.observe(),
        "domain": self.domain.observe(),
        "version": self.version.observe(),
        "fullName": self.full_name.observe(),
        "accounts": self.accounts.observe(),
        "username": self.username.observe()
        })
    }
}
impl Observe for UserResponse {
    fn observe(&self) -> Value {
        json!({
        "userDetails": self.user_details.observe()
        })
    }
}
impl Observe for PortfolioBalance {
    fn observe(&self) -> Value {
        json!({
        "account": self.account.observe(),
        "version": self.version.observe(),
        "value": self.value.observe(),
        "currency": self.currency.observe()
        })
    }
}
impl Observe for PortfolioPosition {
    fn observe(&self) -> Value {
        json!({
        "account": self.account.observe(),
        "version": self.version.observe(),
        "positionCode": self.position_code.observe(),
        "symbol": self.symbol.observe(),
        "quantity": self.quantity.observe(),
        "side": self.side.observe(),
        "quantityNotional": self.quantity_notional.observe(),
        "openTime": self.open_time.observe(),
        "openPrice": self.open_price.observe(),
        "lastUpdateTime": self.last_update_time.observe(),
        "marginRate": self.margin_rate.observe()
        })
    }
}
impl Observe for OrderLeg {
    fn observe(&self) -> Value {
        json!({
        "instrument": self.instrument.observe(),
        "positionEffect": self.position_effect.observe(),
        "positionCode": self.position_code.observe(),
        "side": self.side.observe(),
        "price": self.price.observe(),
        "legRatio": self.leg_ratio.observe(),
        "quantity": self.quantity.observe(),
        "filledQuantity": self.filled_quantity.observe(),
        "remainingQuantity": self.remaining_quantity.observe(),
        "averagePrice": self.average_price.observe()
        })
    }
}
impl Observe for OrderExecution {
    fn observe(&self) -> Value {
        json!({
        "account": self.account.observe(),
        "executionCode": self.execution_code.observe(),
        "orderCode": self.order_code.observe(),
        "updateOrderId": self.update_order_id.observe(),
        "version": self.version.observe(),
        "clientOrderId": self.client_order_id.observe(),
        "actionCode": self.action_code.observe(),
        "status": self.status.observe(),
        "finalStatus": self.final_status.observe(),
        "filledQuantity": self.filled_quantity.observe(),
        "lastQuantity": self.last_quantity.observe(),
        "filledQuantityNotional": self.filled_quantity_notional.observe(),
        "lastQuantityNotional": self.last_quantity_notional.observe(),
        "transactionTime": self.transaction_time.observe(),
        "marginRate": self.margin_rate.observe()
        })
    }
}
impl Observe for PortfolioOrder {
    fn observe(&self) -> Value {
        json!({
        "account": self.account.observe(),
        "version": self.version.observe(),
        "orderId": self.order_id.observe(),
        "orderCode": self.order_code.observe(),
        "clientOrderId": self.client_order_id.observe(),
        "actionCode": self.action_code.observe(),
        "legCount": self.leg_count.observe(),
        "type": self.r#type.observe(),
        "instrument": self.instrument.observe(),
        "status": self.status.observe(),
        "finalStatus": self.final_status.observe(),
        "legs": self.legs.observe(),
        "side": self.side.observe(),
        "tif": self.tif.observe(),
        "marginRate": self.margin_rate.observe(),
        "issueTime": self.issue_time.observe(),
        "transactionTime": self.transaction_time.observe(),
        "executions": self.executions.observe(),
        "cashTransactions": self.cash_transactions.observe()
        })
    }
}
impl Observe for Portfolio {
    fn observe(&self) -> Value {
        json!({
        "account": self.account.observe(),
        "version": self.version.observe(),
        "balances": self.balances.observe(),
        "positions": self.positions.observe(),
        "orders": self.orders.observe()
        })
    }
}
impl Observe for PortfolioResponse {
    fn observe(&self) -> Value {
        json!({
        "portfolios": self.portfolios.observe()
        })
    }
}
impl Observe for Instrument {
    fn observe(&self) -> Value {
        json!({
        "type": self.r#type.observe(),
        "symbol": self.symbol.observe(),
        "version": self.version.observe(),
        "description": self.description.observe(),
        "priceIncrement": self.price_increment.observe(),
        "pipSize": self.pip_size.observe(),
        "currency": self.currency.observe(),
        "lotSize": self.lot_size.observe(),
        "multiplier": self.multiplier.observe(),
        "firstCurrency": self.first_currency.observe()
        })
    }
}
impl Observe for InstrumentsResponse {
    fn observe(&self) -> Value {
        json!({
        "instruments": self.instruments.observe()
        })
    }
}
impl Observe for MaxOrderSize {
    fn observe(&self) -> Value {
        json!({
        "value": self.value.observe(),
        "limitType": self.limit_type.observe()
        })
    }
}
impl Observe for MarginTier {
    fn observe(&self) -> Value {
        json!({
        "endVolume": self.end_volume.observe(),
        "value": self.value.observe()
        })
    }
}
impl Observe for MarginRate {
    fn observe(&self) -> Value {
        json!({
        "rateType": self.rate_type.observe(),
        "tiers": self.tiers.observe()
        })
    }
}
impl Observe for InstrumentDetails {
    fn observe(&self) -> Value {
        json!({
        "account": self.account.observe(),
        "type": self.r#type.observe(),
        "symbol": self.symbol.observe(),
        "tradingStatus": self.trading_status.observe(),
        "minOrderSize": self.min_order_size.observe(),
        "maxOrderSize": self.max_order_size.observe(),
        "minOrderSizeIncrement": self.min_order_size_increment.observe(),
        "marginRate": self.margin_rate.observe()
        })
    }
}
impl Observe for InstrumentDetailsResponse {
    fn observe(&self) -> Value {
        json!({
        "instrumentDetails": self.instrument_details.observe()
        })
    }
}
impl Observe for QuoteEvent {
    fn observe(&self) -> Value {
        json!({
        "symbol": self.symbol.observe(),
        "type": self.r#type.observe(),
        "ask": self.ask.observe(),
        "bid": self.bid.observe(),
        "time": self.time.observe()
        })
    }
}
impl Observe for TickerResponse {
    fn observe(&self) -> Value {
        json!({
        "events": self.events.observe()
        })
    }
}
impl Observe for CandleEvent {
    fn observe(&self) -> Value {
        json!({
        "symbol": self.symbol.observe(),
        "type": self.r#type.observe(),
        "candleType": self.candle_type.observe(),
        "open": self.open.observe(),
        "close": self.close.observe(),
        "high": self.high.observe(),
        "low": self.low.observe(),
        "volume": self.volume.observe(),
        "time": self.time.observe()
        })
    }
}
impl Observe for CandleResponse {
    fn observe(&self) -> Value {
        json!({
        "events": self.events.observe()
        })
    }
}
