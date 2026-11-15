//! The packed Balance producer; the TS frame reader only copies its contract.
use crate::model::{Balance, CoinBalance, CurrencyBalance};
use crate::wire::{FrameWriter, section};

pub(crate) fn write<'a>(
    w: &mut FrameWriter<'a>,
    b: &Balance,
    coins: Option<&'a [CoinBalance]>,
    currencies: Option<&'a [CurrencyBalance]>,
) {
    let coins_absent = coins.is_none();
    w.begin(section::BALANCE);
    w.f64s([b.used, b.free, b.total, b.upnl]);
    if coins.is_some() || currencies.is_some() {
        let coins = coins.unwrap_or_default();
        w.u32(coins.len() as u32);
        for coin in coins {
            let base = w.string(&coin.base);
            w.u32(base);
        }
        w.pad();
        w.f64s(coins.iter().map(|coin| coin.amount));
        w.f64s(coins.iter().map(|coin| coin.notional));
        if let Some(currencies) = currencies {
            w.u32(currencies.len() as u32);
            w.u32(u32::from(coins_absent));
            for currency in currencies {
                let name = w.string(&currency.currency);
                w.u32(name);
            }
            w.pad();
            w.f64s(currencies.iter().map(|row| row.balance.total));
            w.f64s(currencies.iter().map(|row| row.balance.free));
            w.f64s(currencies.iter().map(|row| row.balance.used));
            w.f64s(currencies.iter().map(|row| row.balance.upnl));
        }
    }
    w.end();
}
