use super::*;

const SHAPE: &str = "bingxWriteShape";
const REFUSED: &str = r#"{"code":109400,"msg":"Invalid parameters, the request you constructed does not meet the requirements"}"#;

fn shape(h: &Harness) -> serde_json::Value {
    let wire = serde_json::to_value(&h.failures[0]).unwrap();
    assert!(wire[SHAPE].is_object(), "closed shape missing: {wire}");
    wire[SHAPE].clone()
}

#[test]
fn invalid_input_cancel_has_closed_shape_without_identity_or_parameter_values() {
    let mut target = order();
    target.kind = OrderType::TakeProfit;
    target.side = OrderSide::Sell;
    target.price = 110.;
    let mut h = Harness::new(Some((target.clone(), "TAKE_PROFIT_MARKET")));
    h.command(Command::CancelOrders { command: 1, ids: vec![target.id.clone()] });
    h.reply(REFUSED);
    assert_eq!(shape(&h), serde_json::json!({
        "endpointFamily":"order","method":"DELETE", "parameterNames":["orderId","signature","symbol","timestamp"],
        "hasOtherParameters":false,"idClass":"numeric","idDigits":19,"targetKind":"takeProfit",
        "symbolFormat":"hyphenated","replacement":"unobserved"
    }));
    let encoded = shape(&h).to_string();
    for private in [target.id.as_str(), "BTC-USDT", "110", "secret", "key"] {
        assert!(!encoded.contains(private), "shape retained a value: {encoded}");
    }
    assert_eq!(h.failures[0].kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
}

#[test]
fn every_invalid_input_code_on_amend_has_its_actual_body_parameter_names() {
    for code in [109400, 100400, 80014] {
        let mut h = Harness::new(Some((order(), "LIMIT")));
        h.command(Command::AmendOrder { command: 1, id: order().id, amount: Some(0.8), price: None });
        let sent: serde_json::Value = serde_json::from_str(h.requests[0].body.as_deref().unwrap()).unwrap();
        h.reply(&serde_json::json!({"code":code,"msg":"fixture invalid parameters"}).to_string());
        let actual=shape(&h);
        assert_eq!(actual["endpointFamily"], "amend");
        assert_eq!(actual["method"], "POST");
        assert_eq!(actual["parameterNames"], serde_json::json!(sent.as_object().unwrap().keys().collect::<Vec<_>>()));
    }
}

#[test]
fn cancel_replace_refusal_has_target_kind_and_wire_names() {
    let mut h=Harness::new(Some((order(),"LIMIT")));
    h.command(Command::AmendOrder {command:1,id:order().id,price:Some(101.),amount:None});
    let sent:serde_json::Value=serde_json::from_str(h.requests[0].body.as_deref().unwrap()).unwrap();
    h.reply(REFUSED);
    let actual=shape(&h);
    assert_eq!(actual["endpointFamily"],"cancelReplace");
    assert_eq!(actual["targetKind"],"limit");
    assert_eq!(actual["parameterNames"],serde_json::json!(sent.as_object().unwrap().keys().collect::<Vec<_>>()));
}

#[test]
fn acknowledged_replacement_is_distinguished_from_an_unobserved_origin() {
    let mut h=Harness::new(Some((order(),"LIMIT")));
    h.command(Command::AmendOrder {command:1,id:order().id,price:Some(101.),amount:None});
    let new_id="8389765519993909017";
    h.reply(&serde_json::json!({"code":0,"data":{"cancelResult":"SUCCESS","newOrderResult":"SUCCESS","newOrderId":new_id}}).to_string());
    assert!(h.done[0].is_ok());
    let mut current=order();current.id=new_id.into();current.price=101.;
    h.engine.store_mut().set_orders(vec![current]);
    h.command(Command::CancelOrders {command:2,ids:vec![new_id.into()]});
    h.reply(REFUSED);
    assert_eq!(shape(&h)["replacement"],"observed");
}

#[test]
fn success_auth_network_and_before_send_refusals_do_not_gain_shape() {
    let mut ok=Harness::new(Some((order(),"LIMIT")));
    ok.command(Command::CancelOrders {command:1,ids:vec![order().id]});
    ok.reply(r#"{"code":0,"data":{}}"#);
    assert!(ok.failures.is_empty());assert!(ok.done[0].is_ok());
    for network in [false,true] {
        let mut h=Harness::new(Some((order(),"LIMIT")));
        h.command(Command::CancelOrders {command:1,ids:vec![order().id]});
        if network {h.failed();} else {h.reply(r#"{"code":100413,"msg":"fixture auth refusal"}"#);}
        assert!(serde_json::to_value(&h.failures[0]).unwrap().get(SHAPE).is_none());
    }
    let mut h=Harness::new(Some((order(),"LIMIT")));
    h.command(Command::AmendOrder {command:1,id:order().id,price:None,amount:None});
    assert!(h.requests.is_empty());
    assert!(serde_json::to_value(&h.failures[0]).unwrap().get(SHAPE).is_none());
}
