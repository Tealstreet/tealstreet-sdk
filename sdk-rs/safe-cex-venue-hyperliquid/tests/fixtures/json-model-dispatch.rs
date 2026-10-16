fn model_dispatch(row: &Value) {
    match row["name"].as_str().unwrap() {
        "AllMids" => check_model::<AllMids>(row),
        "UserRateLimit" => check_model::<UserRateLimit>(row),
        "OrderStatus" => check_model::<OrderStatus>(row),
        "HyperliquidFastAssetCtx" => check_model::<HyperliquidFastAssetCtx>(row),
        "HyperliquidFastAssetCtxs" => check_model::<HyperliquidFastAssetCtxs>(row),
        "PrePlacementEntry" => check_model::<PrePlacementEntry>(row),
        "PendingOrderEntry" => check_model::<PendingOrderEntry>(row),
        "AgentWalletCreationResult" => check_model::<AgentWalletCreationResult>(row),
        other => panic!("unknown JSON model {other}"),
    }
}
