include!("trading.rs");

fn ready() -> (Engine<Kraken>, safe_cex_engine::SocketId) {
    support::ready_provider()
}
