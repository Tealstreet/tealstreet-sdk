//! Typed fields consumed by the four native closed-position projections.
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

#[derive(Default)]
pub(super) struct Row {
    pub symbol: Field,
    pub created_time: Field,
    pub updated_time: Field,
    pub closed_size: Field,
    pub avg_entry_price: Field,
    pub avg_exit_price: Field,
    pub closed_pnl: Field,
    pub leverage: Field,
    pub inst_id: Field,
    pub c_time: Field,
    pub u_time: Field,
    pub close_total_pos: Field,
    pub open_avg_px: Field,
    pub close_avg_px: Field,
    pub realized_pnl: Field,
    pub lever: Field,
    pub create_time: Field,
    pub update_time: Field,
    pub close_positions: Field,
    pub open_average_price: Field,
    pub close_average_price: Field,
    pub ctime: Field,
    pub mtime: Field,
    pub qty: Field,
    pub entry_price: Field,
    pub close_price: Field,
    pub realized_pnl_bitunix: Field,
    pub side: Field,
    pub direction: Field,
    pub position_side: Field,
    pub cum_exit_value: Field,
    pub open_fee: Field,
    pub close_fee: Field,
    pub fee: Field,
}

impl<'de> Deserialize<'de> for Row {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(RowVisitor)
    }
}

struct RowVisitor;
impl<'de> Visitor<'de> for RowVisitor {
    type Value = Row;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a native closed-position record")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut fields: A) -> Result<Row, A::Error> {
        let mut row = Row::default();
        while let Some(key) = fields.next_key::<String>()? {
            match key.as_str() {
                "symbol" => row.symbol = fields.next_value()?,
                "createdTime" => row.created_time = fields.next_value()?,
                "updatedTime" => row.updated_time = fields.next_value()?,
                "closedSize" => row.closed_size = fields.next_value()?,
                "avgEntryPrice" => row.avg_entry_price = fields.next_value()?,
                "avgExitPrice" => row.avg_exit_price = fields.next_value()?,
                "closedPnl" => row.closed_pnl = fields.next_value()?,
                "leverage" => row.leverage = fields.next_value()?,
                "instId" => row.inst_id = fields.next_value()?,
                "cTime" => row.c_time = fields.next_value()?,
                "uTime" => row.u_time = fields.next_value()?,
                "closeTotalPos" => row.close_total_pos = fields.next_value()?,
                "openAvgPx" => row.open_avg_px = fields.next_value()?,
                "closeAvgPx" => row.close_avg_px = fields.next_value()?,
                "realizedPnl" => row.realized_pnl = fields.next_value()?,
                "lever" => row.lever = fields.next_value()?,
                "createTime" => row.create_time = fields.next_value()?,
                "updateTime" => row.update_time = fields.next_value()?,
                "closePositions" => row.close_positions = fields.next_value()?,
                "openAveragePrice" => row.open_average_price = fields.next_value()?,
                "closeAveragePrice" => row.close_average_price = fields.next_value()?,
                "ctime" => row.ctime = fields.next_value()?,
                "mtime" => row.mtime = fields.next_value()?,
                "qty" => row.qty = fields.next_value()?,
                "entryPrice" => row.entry_price = fields.next_value()?,
                "closePrice" => row.close_price = fields.next_value()?,
                "realizedPNL" => row.realized_pnl_bitunix = fields.next_value()?,
                "side" => row.side = fields.next_value()?,
                "direction" => row.direction = fields.next_value()?,
                "positionSide" => row.position_side = fields.next_value()?,
                "cumExitValue" => row.cum_exit_value = fields.next_value()?,
                "openFee" => row.open_fee = fields.next_value()?,
                "closeFee" => row.close_fee = fields.next_value()?,
                "fee" => row.fee = fields.next_value()?,
                _ => {
                    fields.next_value::<ValidatedIgnored>()?;
                }
            }
        }
        Ok(row)
    }
}

#[derive(Default)]
pub(super) struct Field {
    pub text: String,
    pub zero_or_false: bool,
    pub sell_literal: bool,
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(FieldVisitor)
    }
}

struct FieldVisitor;
impl FieldVisitor {
    fn number(value: f64) -> Field {
        Field {
            text: if value == 0. {
                "0".into()
            } else if (1e-6..1e21).contains(&value.abs()) {
                value.to_string()
            } else {
                format!("{value:e}")
            },
            zero_or_false: value == 0.,
            sell_literal: false,
        }
    }
}

impl<'de> Visitor<'de> for FieldVisitor {
    type Value = Field;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a reported closed-position field")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Field, E> {
        Ok(Field::default())
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Field, E> {
        Ok(Field {
            text: value.into(),
            sell_literal: value == "Sell",
            ..Field::default()
        })
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Field, E> {
        Ok(Field {
            text: value.to_string(),
            zero_or_false: !value,
            sell_literal: false,
        })
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Field, E> {
        Ok(FieldVisitor::number(value as f64))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Field, E> {
        Ok(FieldVisitor::number(value as f64))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Field, E> {
        Ok(FieldVisitor::number(value))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> Result<Field, A::Error> {
        let mut text = String::new();
        let mut first = true;
        while let Some(value) = values.next_element::<Field>()? {
            if !first {
                text.push(',');
            }
            first = false;
            text.push_str(&value.text);
        }
        Ok(Field {
            text,
            ..Field::default()
        })
    }
    fn visit_map<A: MapAccess<'de>>(self, mut fields: A) -> Result<Field, A::Error> {
        while fields.next_entry::<String, ValidatedIgnored>()?.is_some() {}
        Ok(Field {
            text: "[object Object]".into(),
            ..Field::default()
        })
    }
}

// Unlike IgnoredAny, deserialize_any preserves serde_json's numeric and depth checks.
pub(super) struct ValidatedIgnored;

impl<'de> Deserialize<'de> for ValidatedIgnored {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(Self)
    }
}

impl<'de> Visitor<'de> for ValidatedIgnored {
    type Value = Self;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a valid native JSON value")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self, E> {
        Ok(self)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> Result<Self, A::Error> {
        while values.next_element::<Self>()?.is_some() {}
        Ok(self)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut fields: A) -> Result<Self, A::Error> {
        while fields.next_entry::<String, Self>()?.is_some() {}
        Ok(self)
    }
}
