fn dispatch(row: &Value) {
    match row["name"].as_str().unwrap() {
        "WsFill" => check::<WsFill>(row),
        "WsUserFunding" => check::<WsUserFunding>(row),
        "WsLiquidation" => check::<WsLiquidation>(row),
        "WsNonUserCancel" => check::<WsNonUserCancel>(row),
        other => panic!("unknown spot root {other}"),
    }
}
