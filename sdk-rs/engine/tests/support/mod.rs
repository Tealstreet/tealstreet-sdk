use safe_cex_engine::{Engine, Venue};
pub fn seeded_engine<V: Venue>(venue: V) -> Engine<V> {
    fn source(mut state: u64) -> safe_cex_engine::entropy::Fill {
        Box::new(move |bytes| {
            for byte in bytes {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
            Ok(())
        })
    }
    let entropy = safe_cex_engine::entropy::Entropy::new(source(13), source(29));
    Engine::new(venue, Some(entropy)).unwrap()
}
