use std::collections::HashSet;

pub fn mark_price_loaded(market_ids: &mut HashSet<Vec<u16>>, market_id: &[u16]) {
    market_ids.insert(market_id.to_vec());
}

pub fn index_price_loaded(market_ids: &mut HashSet<Vec<u16>>, market_id: &[u16]) {
    market_ids.insert(market_id.to_vec());
}

pub fn has_mark_price_loaded(market_ids: &HashSet<Vec<u16>>, market_id: &[u16]) -> bool {
    market_ids.contains(market_id)
}

pub fn has_index_price_loaded(market_ids: &HashSet<Vec<u16>>, market_id: &[u16]) -> bool {
    market_ids.contains(market_id)
}
