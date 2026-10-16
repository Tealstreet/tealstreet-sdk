//! Direct transcription of `bitrue.utils.ts` over the shared UTF-8 string boundary.

pub fn reverse_ws_symbol(symbol: &str) -> String {
    let upper = symbol.to_uppercase();
    let raw = upper.strip_prefix('E').unwrap_or(&upper);
    // The source's optional prefix separator is also removed by its global pass.
    let raw: String = raw.chars().filter(|c| *c != '-' && *c != '_').collect();
    let quote = ["USDT", "USDC", "USD"]
        .into_iter()
        .find(|quote| raw.ends_with(quote));
    match quote {
        Some(quote) => format!("E-{}-{quote}", &raw[..raw.len() - quote.len()]),
        None => format!("E-{raw}"),
    }
}
