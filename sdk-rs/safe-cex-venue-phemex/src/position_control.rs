//! Position-query and order-side decisions over live opaque host projections.
use safe_cex_types::PositionMode;
use std::future::Future;
pub trait PositionQueryHost {
    type Value;
    type Response;
    type Rows;
    type Record;
    type Position;
    type Mapper;
    type Error;
    fn ends_usdc(&mut self, symbol: &Self::Value) -> Result<bool, Self::Error>;
    fn format_out(&mut self, symbol: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn get(
        &mut self,
        endpoint: &'static str,
        currency: &'static str,
        symbol: Self::Value,
    ) -> impl Future<Output = Result<Self::Response, Self::Error>>;
    fn response_data(
        &mut self,
        response: Self::Response,
    ) -> Result<Option<Self::Rows>, Self::Error>;
    fn length(&mut self, rows: &Self::Rows) -> Result<usize, Self::Error>;
    fn row(&mut self, rows: &Self::Rows, index: usize)
    -> Result<Option<Self::Record>, Self::Error>;
    fn bind_mapper(&mut self) -> Result<Self::Mapper, Self::Error>;
    fn map_position(
        &mut self,
        mapper: &Self::Mapper,
        record: Self::Record,
    ) -> Result<Option<Self::Position>, Self::Error>;
    fn formatted_row_symbol(
        &mut self,
        row: Option<&Self::Record>,
    ) -> Result<Self::Value, Self::Error>;
    fn same_symbol(&mut self, left: &Self::Value, right: &Self::Value)
    -> Result<bool, Self::Error>;
    fn mode_is(&mut self, row: &Self::Record, mode: &'static str) -> Result<bool, Self::Error>;
    fn report(&mut self, error: &Self::Error) -> Result<(), Self::Error>;
}
fn endpoint() -> &'static str {
    crate::constants::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == "POSITIONS")
        .unwrap()
        .1
}
pub async fn fetch_position_for_symbol<H: PositionQueryHost>(
    host: &mut H,
    symbol: H::Value,
) -> Result<Vec<H::Position>, H::Error> {
    let result = async {
        let currency = if host.ends_usdc(&symbol)? {
            "USDC"
        } else {
            "USDT"
        };
        let formatted = host.format_out(&symbol)?;
        let response = host.get(endpoint(), currency, formatted).await?;
        let Some(rows) = host.response_data(response)? else {
            return Ok(Vec::new());
        };
        let mapper = host.bind_mapper()?;
        let length = host.length(&rows)?;
        let mut positions = Vec::new();
        for index in 0..length {
            if let Some(row) = host.row(&rows, index)? {
                positions.push(host.map_position(&mapper, row)?);
            }
        }
        Ok(positions.into_iter().flatten().collect())
    }
    .await;
    match result {
        Ok(positions) => Ok(positions),
        Err(error) => {
            host.report(&error)?;
            Ok(Vec::new())
        }
    }
}
pub async fn fetch_raw_position_mode<H: PositionQueryHost>(
    host: &mut H,
    symbol: H::Value,
) -> Result<Option<PositionMode>, H::Error> {
    let result = async {
        let currency = if host.ends_usdc(&symbol)? {
            "USDC"
        } else {
            "USDT"
        };
        let formatted = host.format_out(&symbol)?;
        let response = host.get(endpoint(), currency, formatted).await?;
        let Some(rows) = host.response_data(response)? else {
            return Ok(None);
        };
        let length = host.length(&rows)?;
        for index in 0..length {
            let row = host.row(&rows, index)?;
            let formatted = host.formatted_row_symbol(row.as_ref())?;
            if host.same_symbol(&formatted, &symbol)? {
                return match row {
                    Some(row) => {
                        if host.mode_is(&row, "OneWay")? {
                            Ok(Some(PositionMode::OneWay))
                        } else if host.mode_is(&row, "Hedged")? {
                            Ok(Some(PositionMode::Hedged))
                        } else {
                            Ok(None)
                        }
                    }
                    None => Ok(None),
                };
            }
        }
        Ok(None)
    }
    .await;
    match result {
        Ok(mode) => Ok(mode),
        Err(error) => {
            host.report(&error)?;
            Ok(None)
        }
    }
}
pub trait OrderSideHost {
    type Options;
    type Value;
    type Positions;
    type Error;
    fn info_side(
        &mut self,
        options: &Self::Options,
        optional: bool,
    ) -> Result<Self::Value, Self::Error>;
    fn truthy(&mut self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn symbol(&mut self, options: &Self::Options) -> Result<Self::Value, Self::Error>;
    fn positions(
        &mut self,
        symbol: Self::Value,
    ) -> impl Future<Output = Result<Self::Positions, Self::Error>>;
    fn length(&mut self, positions: &Self::Positions) -> Result<usize, Self::Error>;
    fn position_oneway(
        &mut self,
        positions: &Self::Positions,
        index: usize,
    ) -> Result<Option<bool>, Self::Error>;
    fn raw_mode(
        &mut self,
        symbol: Self::Value,
    ) -> impl Future<Output = Result<Option<PositionMode>, Self::Error>>;
    fn is_buy(&mut self, options: &Self::Options) -> Result<bool, Self::Error>;
    fn is_type(&mut self, options: &Self::Options, kind: &'static str)
    -> Result<bool, Self::Error>;
    fn reduce_only(&mut self, options: &Self::Options) -> Result<bool, Self::Error>;
    fn value(&mut self, side: &'static str) -> Result<Self::Value, Self::Error>;
}
pub async fn get_order_position_side<H: OrderSideHost>(
    host: &mut H,
    options: H::Options,
) -> Result<H::Value, H::Error> {
    let info = host.info_side(&options, true)?;
    if host.truthy(&info)? {
        return host.info_side(&options, false);
    }
    let symbol = host.symbol(&options)?;
    let positions = host.positions(symbol).await?;
    let length = host.length(&positions)?;
    let mut oneway = false;
    for index in 0..length {
        if host.position_oneway(&positions, index)? == Some(true) {
            oneway = true;
            break;
        }
    }
    if host.length(&positions)? == 0 {
        let symbol = host.symbol(&options)?;
        oneway = host.raw_mode(symbol).await? == Some(PositionMode::OneWay);
    }
    if oneway {
        return host.value("Merged");
    }
    let mut side = if host.is_buy(&options)? {
        "Long"
    } else {
        "Short"
    };
    if host.is_type(&options, "stop_market")?
        || host.is_type(&options, "take_profit_market")?
        || host.is_type(&options, "trailing_stop_market")?
        || host.reduce_only(&options)?
    {
        side = if side == "Long" { "Short" } else { "Long" };
    }
    host.value(side)
}
