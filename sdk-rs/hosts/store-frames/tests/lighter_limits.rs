#[path = "../../../venues/lighter/tests/support/mod.rs"]
mod support;
use safe_cex_engine::{
    Action,
    wire::{self, FrameWriter},
};
use safe_cex_store_frames::{Emitter, Inputs, decode_section};

#[test]
fn lighter_account_limits_reach_balance_info_in_real_native_frames() {
    let (mut engine, _, mut timers) = support::ready_tracked(support::options());
    let request = support::advance(&mut engine, &mut timers, support::NOW + 2001.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("accountLimits") => Some(request),
            _ => None,
        })
        .expect("private account poll reads its account limits");
    let limits =
        r#"{"code":200,"user_tier_name":"premium","max_leverage":20,"daily_limit":"100000"}"#;
    support::reply_at(&mut engine, support::NOW + 2001.0, &request, 200, limits);
    let store = engine.store();
    let mut writer = FrameWriter::new(Vec::new(), support::NOW + 2001.0);
    store.write(&store.everything(), &mut writer);
    let frame = Emitter::default().frame(
        writer.finish(),
        store,
        &Inputs::new("acct", "lighter", false),
        true,
    );
    let stats = wire::sections(&frame)
        .find(|(kind, _)| *kind == 35)
        .unwrap()
        .1;
    assert!(
        u32::from_le_bytes(stats[..4].try_into().unwrap()) > 0,
        "real physical HTTP completions must update requestStats"
    );
    let payload = wire::sections(&frame)
        .find(|(kind, _)| *kind == 32)
        .unwrap()
        .1;
    let info = decode_section(payload).unwrap();
    assert_eq!(info["lighterAccountLimits"]["user_tier_name"], "premium");
    assert_eq!(info["lighterAccountLimits"]["max_leverage"], 20.0);
    assert_eq!(info["lighterAccountLimits"]["daily_limit"], "100000");
}
