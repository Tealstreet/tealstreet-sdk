use std::collections::{BTreeMap, BTreeSet};
use syn::{GenericArgument, Item, PathArguments, Type, visit::Visit};

// Reviewed reachable graph of the three numeric response roots. Unknown owners
// still fail classification; private remote helpers never become public ingress.
const RETAINED_NUMERIC_STRUCTS: [&str; 21] = [
    "MarketResponseProductsItemFcmTradingSessionDetails",
    "MarketResponseProductsItemFutureProductDetailsPerpetualDetails",
    "MarketResponseProductsItemFutureProductDetails",
    "MarketResponseProductsItem",
    "MarketResponse",
    "IntxInstrumentsResponseItemQuote",
    "IntxInstrumentsResponseItem",
    "PortfolioBreakdownResponseBreakdownPortfolio",
    "PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance",
    "PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance",
    "PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance",
    "PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance",
    "PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl",
    "PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl",
    "PortfolioBreakdownResponseBreakdownPortfolioBalances",
    "PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis",
    "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
    "PortfolioBreakdownResponseBreakdownPerpPositionsItem",
    "PortfolioBreakdownResponseBreakdownFuturesPositionsItem",
    "PortfolioBreakdownResponseBreakdown",
    "PortfolioBreakdownResponse",
];
const RETAINED_NUMERIC_ENUMS: [&str; 2] = [
    "PortfolioBreakdownResponseBreakdownPerpPositionsItemSide",
    "PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide",
];
fn retained_numeric_struct(file: &str, name: &str) -> bool {
    file == "types" && RETAINED_NUMERIC_STRUCTS.contains(&name)
}
fn remote_target(attributes: &[syn::Attribute]) -> Option<String> {
    let mut targets = Vec::new();
    for attribute in attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("serde"))
    {
        let values = attribute
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .unwrap();
        for value in values {
            if let syn::Meta::NameValue(value) = value {
                if value.path.is_ident("remote") {
                    let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(value),
                        ..
                    }) = value.value
                    else {
                        panic!("remote target must be a literal native owner")
                    };
                    targets.push(value.value());
                }
            }
        }
    }
    assert!(targets.len() <= 1, "one remote target per helper");
    targets.pop()
}
#[derive(Default)]
struct RetainedCalls {
    functions: BTreeSet<String>,
    methods: BTreeSet<String>,
}
impl<'ast> Visit<'ast> for RetainedCalls {
    fn visit_expr_call(&mut self, expression: &'ast syn::ExprCall) {
        if let syn::Expr::Path(function) = expression.func.as_ref() {
            self.functions.insert(
                function
                    .path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
            );
        }
        syn::visit::visit_expr_call(self, expression);
    }
    fn visit_expr_method_call(&mut self, expression: &'ast syn::ExprMethodCall) {
        self.methods.insert(expression.method.to_string());
        syn::visit::visit_expr_method_call(self, expression);
    }
}

#[derive(Default)]
struct Implementations<'ast>(Vec<&'ast syn::ItemImpl>);
impl<'ast> Visit<'ast> for Implementations<'ast> {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if item.attrs.iter().any(|attribute| {
            attribute.path().is_ident("cfg")
                && attribute
                    .parse_args::<syn::Path>()
                    .is_ok_and(|path| path.is_ident("test"))
        }) {
            return;
        }
        syn::visit::visit_item_mod(self, item);
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        self.0.push(item);
        syn::visit::visit_item_impl(self, item);
    }
}
fn path_name(ty: &Type) -> String {
    let Type::Path(p) = ty else {
        panic!("unclassified decoder type")
    };
    assert!(p.qself.is_none());
    p.path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}
fn argument(path: &syn::Path) -> &Type {
    let PathArguments::AngleBracketed(args) = &path.segments.last().unwrap().arguments else {
        panic!("missing conversion input")
    };
    assert_eq!(args.args.len(), 1);
    let GenericArgument::Type(ty) = &args.args[0] else {
        panic!("non-type conversion input")
    };
    ty
}
fn map_input(ty: &Type) {
    assert_eq!(
        path_name(ty),
        "serde_json::Map",
        "record conversion must accept only an object Map"
    );
    let Type::Path(p) = ty else { unreachable!() };
    let PathArguments::AngleBracketed(args) = &p.path.segments.last().unwrap().arguments else {
        panic!("missing Map parameters")
    };
    let names: Vec<_> = args
        .args
        .iter()
        .map(|arg| {
            let GenericArgument::Type(ty) = arg else {
                panic!("non-type Map parameter")
            };
            path_name(ty)
        })
        .collect();
    assert_eq!(names, ["String", "serde_json::Value"]);
}
// Exact production adapter census. These implementations supply imported host
// capabilities to already classified bodies. Raw parsing remains delegation:
// SocketHost's two json_parse methods call shared JSON.parse; CryptoHost imports
// PKCS8 through the independently observed safe-cex-crypto Es256SigningKey owner.
fn reviewed_binding_adapter(file: &str, imp: &syn::ItemImpl) {
    let owner = path_name(&imp.self_ty);
    let trait_name = imp
        .trait_
        .as_ref()
        .map(|(path, _)| path.segments.last().unwrap().ident.to_string())
        .unwrap_or_default();
    let expected = match (file, owner.as_str(), trait_name.as_str()) {
        ("api", "Host", "CoinbaseApiHost") => {
            "safe_create serialize_repeat network_error_on_read retry install_interceptor install_transport_adapter replace_url_params mark_skip_remote_signing proxy_prefix headers sign"
        }
        ("classes", "Host", "CoinbaseConstructionHost") => {
            "construct_base pattern_client_order_id_rule uuid create_secondary rate_limit construct_public_base construct_private_base"
        }
        ("crypto", "CryptoHost", "CdpSigningHost") => {
            "prepare_pkcs8 import_key current_time_ms uuid sign_ed25519 sign_ecdsa"
        }
        ("exchange", "CoinbaseExchange", "") => "new fetch_balance",
        ("exchange", "CoinbaseExchange", "Exchange") => {
            "base capabilities is_public_data_only dispose place_order start get_account validate_account fetch_positions fetch_orders fetch_markets fetch_tickers fetch_symbol_max_leverage set_leverage change_position_mode change_margin_type update_order cancel_orders fetch_ohlcv listen_ohlcv listen_order_book sign_ws_auth class_name"
        }
        ("exchange", "Host", "ExchangeWsAuthHost") => "options sign_local",
        ("lifecycle", "Host", "") => {
            "async_js loaded publish tick market_values ticker_values position_values balance_value snapshot_values socket"
        }
        ("lifecycle", "Host", "LifecycleHost") => {
            "is_disposed public_only init_external_public_data find_selected_portfolio fetch_markets fetch_tickers_intx fetch_balance fetch_open_orders_snapshot tick get_markets loaded update connect_public connect_private dispose_base dispose_public dispose_private clear_timeout set_timeout last_private_user_stream_message_at mark_private_user_stream_degraded merge_rest_orders_with_fresh_ws_state emit_orders_fetch_error emit_and_log_error log"
        }
        ("lifecycle", "Host", "TickHost") => {
            "is_disposed fetch_tickers fetch_balance fetch_positions fetch_open_orders_snapshot loaded update public_message_handlers should_poll_orders merge_rest_orders_with_fresh_ws_state emit_orders_fetch_error emit_and_log_error get_tick_interval loop_again"
        }
        ("orders", "OrderHost", "") => "new",
        ("orders", "Host", "") => {
            "order_snapshot_receiver order_host schedule_reconcile schedule_write_check synced_order native_orders emit_orders_error should_poll merge_rest place cancel"
        }
        ("orders", "OrderHost", "CoinbaseOrderHost") => {
            "now_ms orders_loaded orders market_by_symbol markets_by_id resolve_caller_client_order_id uuid post_create_order post_cancel_orders add_or_update_order update_orders_status schedule_open_orders_rest_reconciliation schedule_private_user_stream_write_check log_warning emit_and_log_error"
        }
        ("orders", "Host", "OrderSnapshotHost") => {
            "public_only supported_types get_orders resolve_market_by_product map_rest_order now_ms emit_and_log_error emit_orders_fetch_error"
        }
        ("orders", "Host", "OrderEditHost") => {
            "selected_portfolio_type orders market_by_symbol cancel_orders place_order post_edit schedule_reconciliation schedule_write_check emit_and_log_error"
        }
        ("reads", "Host", "") => {
            "read_get read_markets read_market public_read_receiver emit_args cached_balance dayjs daily"
        }
        ("reads", "Host", "PortfolioHost") => {
            "get emit_and_log_error cached_balance position_upnls"
        }
        ("reads", "Host", "MarketPositionHost") => {
            "get error_response_items get_supported_types markets market_by_id cached_tickers cached_positions emit_and_log_error local_wall_time_to_utc"
        }
        ("reads", "Host", "PublicMarketReadHost") => {
            "public_only get_supported_types get error_response_items cached_markets emit_and_log_error dayjs_value_of dayjs_subtract daily_candle set_daily_candle"
        }
        ("rt", "Host", "") => {
            "new base receiver class promise register_client client xhr request emit options json_error runtime global public_only now"
        }
        ("sockets", "SocketHost", "") => {
            "new call open handle listener stringify emit log_console order_values"
        }
        ("sockets", "SocketHost", "PublicStreamHost") => {
            "is_disposed testnet reconnecting_enabled create_websocket add_listener reset_orderbook emit_ws_reconnected markets_symbols send_subscription track_last_received json_parse emit_error console_error_text console_error_frame handle_pong_event record_message_handler_stats"
        }
        ("sockets", "SocketHost", "PublicMarketListenerHost") => {
            "is_connected public_data_only_option timeframe_seconds daily_candle set_daily_candle utc_end_of_day_ms fetch_ohlcv_detached dayjs_unix add warn set_timeout cleanup_message_handler_stats nanoid update_order_book sort_order_book calc_order_book_total"
        }
        ("sockets", "SocketHost", "PrivateStreamHost") => {
            "is_disposed options exchange_name testnet resolve_private_route create_websocket add_listener reset_order_snapshot_assembly close_socket send_with_log sign_local track_last_received emit_ws_diagnostic_receive json_parse mark_private_user_stream_auth_failed mark_private_user_stream_message_received emit_and_log_error handle_user_event_orders handle_pong_event"
        }
        ("sockets", "SocketHost", "CoinbasePrivateClassHost") => {
            "private_order_markets resolve_private_order_market private_order_console_error private_order_now_ms private_orders_loaded publish_private_order_snapshot add_private_orders emit_private_order_fill"
        }
        _ => panic!("unclassified binding adapter {file}::{owner}::{trait_name}"),
    };
    let actual: BTreeSet<_> = imp
        .items
        .iter()
        .filter_map(|item| match item {
            syn::ImplItem::Fn(method) => Some(method.sig.ident.to_string()),
            _ => None,
        })
        .collect();
    let expected: BTreeSet<_> = expected.split_whitespace().map(str::to_owned).collect();
    assert_eq!(
        actual, expected,
        "reviewed adapter API {file}::{owner}::{trait_name}"
    );
    if (file, owner.as_str(), trait_name.as_str()) == ("crypto", "CryptoHost", "CdpSigningHost") {
        let mut calls = RetainedCalls::default();
        calls.visit_item_impl(imp);
        assert!(
            calls.functions.contains("Es256SigningKey::from_pkcs8_der"),
            "raw key import delegates to observed primitive"
        );
        assert!(
            calls
                .functions
                .contains("crate::utils::pem_to_pkcs8_der_with"),
            "PEM import delegates to classified helper"
        );
    }
    if file == "sockets"
        && matches!(
            trait_name.as_str(),
            "PublicStreamHost" | "PrivateStreamHost"
        )
    {
        struct Parser {
            shared: bool,
        }
        impl<'ast> Visit<'ast> for Parser {
            fn visit_expr_lit(&mut self, value: &'ast syn::ExprLit) {
                if matches!(&value.lit, syn::Lit::Str(text) if text.value() == "JSON.parse") {
                    self.shared = true;
                }
                syn::visit::visit_expr_lit(self, value);
            }
        }
        let method = imp
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) if method.sig.ident == "json_parse" => Some(method),
                _ => None,
            })
            .unwrap();
        let mut parser = Parser { shared: false };
        parser.visit_impl_item_fn(method);
        assert!(
            parser.shared,
            "websocket decoder delegates to shared JSON.parse"
        );
    }
}

#[test]
fn every_public_wire_decoder_entrypoint_is_classified() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![directory];
    while let Some(path) = files.pop() {
        if path.is_dir() {
            files.extend(
                std::fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let file = match path.file_stem().unwrap().to_str().unwrap() {
            "starbase_cutover" => "cutover",
            "starbase" => "protocol",
            "starbase_session" => "session",
            name => name,
        };
        let source = std::fs::read_to_string(&path).unwrap();
        let parsed = syn::parse_file(&source).unwrap();
        let types: BTreeMap<_, _> = parsed
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Struct(s) => Some((s.ident.to_string(), true)),
                Item::Enum(e) => Some((e.ident.to_string(), false)),
                _ => None,
            })
            .collect();
        for item in &parsed.items {
            let (name, attrs) = match item {
                Item::Struct(s) => (s.ident.to_string(), &s.attrs),
                Item::Enum(e) => (e.ident.to_string(), &e.attrs),
                _ => continue,
            };
            let derived = attrs
                .iter()
                .filter(|a| a.path().is_ident("derive"))
                .any(|a| {
                    a.parse_args_with(
                        syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
                    )
                    .unwrap()
                    .iter()
                    .any(|p| p.segments.last().unwrap().ident == "Deserialize")
                });
            // PortfolioState uses the separately classified retained object decoder.
            let expected = matches!(file, "types" | "cutover")
                && (types[&name] || name == "CoinbaseStarbaseFalse")
                && !retained_numeric_struct(file, &name);
            assert_eq!(
                derived, expected,
                "{file}::{name}: derived/strict/serialize-only classification"
            );
        }
        if file == "types" {
            for name in RETAINED_NUMERIC_STRUCTS {
                let public = parsed
                    .items
                    .iter()
                    .find_map(|item| match item {
                        Item::Struct(item) if item.ident == name => Some(item),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("missing reviewed retained owner {name}"));
                assert!(
                    matches!(public.vis, syn::Visibility::Public(_)),
                    "native owner stays public {name}"
                );
                let companion = format!("{name}Wire");
                let helper = parsed
                    .items
                    .iter()
                    .find_map(|item| match item {
                        Item::Struct(item) if item.ident == companion => Some(item),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("missing private remote companion {companion}"));
                assert!(
                    matches!(helper.vis, syn::Visibility::Inherited),
                    "remote companion stays private {companion}"
                );
                assert!(
                    helper
                        .fields
                        .iter()
                        .all(|field| matches!(field.vis, syn::Visibility::Inherited)),
                    "remote companion fields stay private {companion}"
                );
                assert_eq!(
                    remote_target(&helper.attrs).as_deref(),
                    Some(name),
                    "remote native owner {companion}"
                );
                assert_eq!(
                    public
                        .fields
                        .iter()
                        .map(|field| field.ident.as_ref().unwrap().to_string())
                        .collect::<Vec<_>>(),
                    helper
                        .fields
                        .iter()
                        .map(|field| field.ident.as_ref().unwrap().to_string())
                        .collect::<Vec<_>>(),
                    "remote field mapping {companion}"
                );
            }
            for item in &parsed.items {
                if let Item::Struct(item) = item {
                    if let Some(target) = remote_target(&item.attrs) {
                        assert!(
                            RETAINED_NUMERIC_STRUCTS.contains(&target.as_str()),
                            "unreviewed remote owner {target}"
                        );
                        assert_eq!(item.ident.to_string(), format!("{target}Wire"));
                    }
                }
            }
        }
        let mut outputs = BTreeSet::new();
        let mut manual = BTreeSet::new();
        let mut implementations = Implementations::default();
        implementations.visit_file(&parsed);
        for imp in implementations.0 {
            if path
                .parent()
                .is_some_and(|parent| parent.ends_with("binding"))
            {
                reviewed_binding_adapter(file, imp);
                continue;
            }
            let owner = path_name(&imp.self_ty);
            if !types.contains_key(&owner) {
                // Only these outbound serializers may target a non-local type.
                // Reject qualified owners and aliases too: a new decoder must
                // be explicitly reviewed, wherever its impl is nested.
                let Some((trait_path, _)) = &imp.trait_ else {
                    panic!("unclassified inherent owner {file}::{owner}")
                };
                let trait_name = trait_path.segments.last().unwrap().ident.to_string();
                // One more shape is allowed on a non-local type: a blanket
                // forwarding impl of the venue's own HOST trait, so that an
                // `Rc<H>` is itself a host. A host trait supplies callbacks and
                // carries no wire-decoding entrypoint - the decoders stay on the
                // local types, which this gate still inventories. Everything
                // else on a foreign owner is refused as before.
                // Reviewed native object/array/scalar access adapters. These
                // exact traits carry retained roots, not Serde entrypoints.
                if matches!(
                    (file, owner.as_str(), trait_name.as_str()),
                    ("classes", "Rc", "CoinbaseApiHost")
                        | ("portfolios_balances", "Rc", "PortfolioHost")
                        | ("order_sync", "Input", "PrivateOrderScalar")
                        | (
                            "order_sync",
                            "crate::starbase_public_frames::CoinbaseStarbaseFrameValue",
                            "PrivateOrderScalar" | "PrivateOrderInfo" | "PrivateOrderEventInput"
                        )
                        | ("order_sync", "ReadItem", "PrivateOrderInfo")
                        | ("order_sync", "Rc", "PrivateOrderArray")
                        | (
                            "order_sync",
                            "PrivateOrderEventRoot",
                            "PrivateOrderEventInput"
                        )
                        | ("private_order_snapshot", "Order", "CoinbaseSnapshotOrder")
                        | (
                            "private_order_snapshot",
                            "crate::order_sync::SyncedOrderRoot",
                            "CoinbaseSnapshotOrder"
                        )
                ) {
                    continue;
                }
                assert_eq!(
                    trait_name, "From",
                    "unclassified trait owner {file}::{owner}"
                );
                let input = path_name(argument(trait_path));
                assert!(
                    matches!(
                        (file, owner.as_str(), input.as_str()),
                        ("cutover", "bool", "CoinbaseStarbaseFalse")
                            | ("starbase_public", "u32", "CoinbaseStarbaseOrderBookDepth")
                            | ("session", "Map", "CoinbaseStarbaseCancelOnDisconnectParams")
                    ),
                    "unclassified outbound conversion {file}::{input} -> {owner}"
                );
                continue;
            }
            if let Some((trait_path, _)) = &imp.trait_ {
                let trait_name = trait_path.segments.last().unwrap().ident.to_string();
                match trait_name.as_str() {
                    "Deserialize" => {
                        assert!(manual.insert(owner.clone()), "duplicate decoder");
                        if (file, owner.as_str()) == ("portfolios_balances", "PortfolioState") {
                            let mut calls = RetainedCalls::default();
                            calls.visit_item_impl(imp);
                            assert!(
                                calls.functions.contains("Input::deserialize"),
                                "portfolio retained capture"
                            );
                            assert!(
                                calls.methods.contains("shift_remove"),
                                "portfolio consumed fields are selected separately"
                            );
                            assert!(
                                !calls.methods.contains("into_value"),
                                "portfolio metadata has no JSON projection"
                            );
                        }
                        if retained_numeric_struct(file, &owner) {
                            let mut calls = RetainedCalls::default();
                            calls.visit_item_impl(imp);
                            assert!(
                                calls.functions.contains(
                                    "safe_cex_num::numeric_ingress::JsNumberInput::deserialize"
                                ),
                                "retained capture for {owner}"
                            );
                            assert!(
                                calls.methods.contains("decode_with"),
                                "retained replay for {owner}"
                            );
                            assert!(
                                calls
                                    .functions
                                    .contains("safe_cex_types::strict_decode::object"),
                                "strict object boundary for {owner}"
                            );
                            assert!(
                                calls
                                    .functions
                                    .contains(&format!("{owner}Wire::deserialize")),
                                "private remote delegation for {owner}"
                            );
                        }
                    }
                    "PortfolioClassBinding" => {
                        assert_eq!((file, owner.as_str()), ("classes", "ClassPortfolioBinding"));
                    }
                    "BaseSigningOptionsView" => {
                        assert_eq!((file, owner.as_str()), ("classes", "ClassSigningView"));
                        assert_eq!(
                            trait_path
                                .segments
                                .iter()
                                .map(|segment| segment.ident.to_string())
                                .collect::<Vec<_>>(),
                            ["safe_cex_exchange", "exchange", "BaseSigningOptionsView"]
                        );
                    }
                    "PrivateOrderEventHost" => {
                        assert_eq!(
                            (file, owner.as_str()),
                            ("classes", "PrivateClassOrderAdapter")
                        );
                        assert_eq!(path_name(argument(trait_path)), "Frame");
                    }
                    "JsNumberOutput" => {
                        assert_eq!(file, "types", "unreviewed output registration file");
                        assert!(
                            RETAINED_NUMERIC_STRUCTS.contains(&owner.as_str())
                                || RETAINED_NUMERIC_ENUMS.contains(&owner.as_str()),
                            "unreviewed output owner {owner}"
                        );
                        assert!(
                            outputs.insert(owner.clone()),
                            "duplicate native output registration"
                        );
                    }
                    "TryFrom" => {
                        let input = argument(trait_path);
                        if owner == "CoinbaseStarbaseFalse" {
                            assert_eq!(path_name(input), "bool");
                        } else {
                            assert!(
                                types[&owner],
                                "unclassified enum conversion {file}::{owner}"
                            );
                            map_input(input);
                        }
                    }
                    "Clone" if file == "class_methods" => {
                        assert_eq!(owner, "BoundClassMethod", "only outward bound values clone");
                    }
                    "Clone" => assert!(matches!(
                        owner.as_str(),
                        "CoinbaseStarbasePromise"
                            | "CoinbaseStarbasePromiseResolver"
                            | "CoinbaseStarbaseAuthSession"
                            | "CoinbaseStarbaseWebsocketSessionController"
                    )),
                    "Future" => assert!(matches!(
                        owner.as_str(),
                        "CoinbaseStarbasePromise" | "MicrotaskJob"
                    )),
                    "Serialize" => assert!(
                        matches!(
                            owner.as_str(),
                            "CoinbaseStarbasePublicChannel"
                                | "CoinbaseStarbaseJsonRpcParams"
                                | "CoinbaseStarbaseRequestHandle"
                        ) || (file, owner.as_str()) == ("portfolios_balances", "PortfolioState")
                    ),
                    // The RPC sum only lifts an already-constructed protocol error.
                    "From" => {
                        let input = path_name(argument(trait_path));
                        assert!(
                            matches!(
                                (file, owner.as_str(), input.as_str()),
                                (
                                    "starbase_rpc",
                                    "CoinbaseStarbaseRequestError",
                                    "CoinbaseStarbaseProtocolError"
                                ) | (
                                    "portfolios_balances",
                                    "PortfolioThrown",
                                    "safe_cex_store::StoreMutationError"
                                )
                            ),
                            "only reviewed native error conversions: {file}::{owner} from {input}"
                        );
                    }
                    "Default" => assert!(
                        matches!(
                            owner.as_str(),
                            "CoinbaseStarbaseSequencedOrderBook" | "CoinbaseStarbaseReadOnlyState"
                        ) || (file, owner.as_str())
                            == ("private_order_snapshot", "CoinbasePrivateOrderSnapshot")
                    ),
                    "Display" | "Error" => assert!(matches!(
                        (file, owner.as_str()),
                        ("protocol", "CoinbaseStarbaseProtocolError")
                            | ("starbase_rpc", "CoinbaseStarbaseRpcError")
                            | ("portfolios_balances", "PortfolioThrown")
                    )),
                    // classes.rs: the spot exchange derefs to the base one so it
                    // inherits its surface. A Deref is delegation, not a decoder.
                    "Deref" => assert_eq!(owner, "CoinbaseSpotExchange"),
                    _ => panic!(
                        "unclassified decoder/conversion trait {file}::{owner}::{trait_name}"
                    ),
                }
            } else {
                for member in &imp.items {
                    if let syn::ImplItem::Fn(method) = member {
                        assert!(
                            matches!(
                                (file, owner.as_str(), method.sig.ident.to_string().as_str()),
                                // Native thrown-value construction/access, without wire decoding.
                                (
                                    "portfolios_balances",
                                    "PortfolioThrown",
                                    "from_value" | "store_mutation" | "from_native" | "native"
                                ) | (
                                    "class_methods",
                                    "ClassMethodSlot",
                                    "new" | "capture" | "assigned"
                                ) | (
                                    "class_methods",
                                    "BoundClassMethod",
                                    "call" | "same_function"
                                )
                            ) || matches!(
                                (owner.as_str(), method.sig.ident.to_string().as_str()),
                                (
                                    "CoinbasePrivateUserStreamHealth",
                                    "last_message_at"
                                        | "has_warned_degraded"
                                        | "mark_private_user_stream_message_received"
                                        | "mark_private_user_stream_degraded"
                                        | "mark_private_user_stream_auth_failed"
                                ) | (
                                    // Recovery 315e771e27 extracts these state operations; none is a wire decoder.
                                    "CoinbasePrivateOrderSnapshot",
                                    "merge_snapshot_orders"
                                        | "apply_order_snapshot_batch"
                                        | "reset_order_snapshot_assembly"
                                        | "pending_raw_order_count"
                                        | "has_pending_snapshot"
                                        | "pending_snapshot_orders"
                                        | "prepare_order_snapshot_batch"
                                 ) | ("CoinbaseStarbaseProtocolError", "new")
                                    // Initializes native host/field state only; accepts H, no raw bytes/JSON/record input.
                                    | ("PublicStreamReceiver", "new")
                                    // Object-record/native-candle view access, with no wire decoding.
                                    | ("PublicData", "get" | "fields")
                                    // Plain retained JSON map carrier, without typed wire field decoding.
                                    | ("CoinbaseOrderResponse", "from_json" | "from_retained" | "get" | "shallow_copy")
                                    // UTF-16 grammar scanner, not a Serde/record ingress path.
                                    | ("JsonDuplicateKeyScanner", "new" | "scan" | "scan_value" | "scan_object" | "scan_array" | "scan_string" | "scan_number" | "consume_literal" | "skip_whitespace")
                                    // Native parsed JSON operations; no Serde decoder or unguarded raw parser.
                                    | ("CoinbaseStarbaseFrameValue", "from_json" | "get" | "own_keys")
                                    | ("ParsedJson", "whitespace" | "string" | "value")
                                    // Local handles and inert callback operations; no wire decoding entrypoint.
                                    | ("CoinbaseStarbaseJsonRpcParams", "new" | "borrow" | "borrow_object_mut" | "shares_object")
                                    | ("CoinbaseStarbaseRequestHandle", "new" | "borrow" | "borrow_mut" | "shares_request")
                                    | ("CoinbaseStarbaseRpcError", "new")
                                    | ("CoinbaseStarbasePromise", "pending" | "resolved" | "same_promise" | "from_future" | "observe")
                                    | ("CoinbaseStarbasePromiseResolver", "resolve")
                                    | ("CoinbaseStarbaseAuthResultHandle", "new" | "borrow" | "borrow_mut" | "shares_result")
                                    | ("CoinbaseStarbaseAuthSession", "new" | "clock" | "create_cdp_jwt" | "authenticate" | "get_http_authorization")
                                    | ("CoinbaseStarbaseWebsocketSessionController", "new" | "state" | "set_state" | "replay_subscriptions" | "activate" | "maintain_authentication" | "handle_classified_heartbeat" | "record_disconnect" | "acknowledge_rate_limit_backoff" | "activate_generation" | "activation_checkpoint" | "replay_activation" | "apply_cancel_on_disconnect" | "assert_current_activation" | "assert_ready")
                                    | ("CoinbaseStarbaseJsonRpcClient", "new" | "transport" | "build_request" | "request" | "parse_response" | "request_promise")
                                    | ("CoinbaseStarbaseHeartbeatController", "new" | "enable" | "disable" | "handle_signal" | "enable_promise" | "handle_signal_promise")
                                    | (
                                        "CoinbaseStarbasePublicSubscriptions",
                                        "new"
                                            | "subscribe"
                                            | "unsubscribe"
                                            | "build_reconnect_request"
                                            | "validate_acknowledgement"
                                    )
                                    | (
                                        "CoinbaseStarbaseSequencedOrderBook",
                                        "new"
                                            | "order_book"
                                            | "apply_payload"
                                            | "on_reconnect"
                                            | "actions_match_book"
                                    )
                                    | (
                                        "CoinbaseStarbaseReadOnlyState",
                                        "apply_account_summary"
                                            | "apply_positions"
                                            | "apply_open_orders"
                                            | "apply_order_state"
                                            | "apply_order_history"
                                            | "apply_order_history_by_instrument"
                                            | "apply_trade_page"
                                    )
                                    // classes.rs: construction and receiver-wrapping
                                    // only. `new` takes a host plus already-built
                                    // Frame values; the three *_receiver methods wrap
                                    // a host in a receiver struct. No Deserialize, no
                                    // raw bytes, no JSON entry - none is a decoder.
                                    | (
                                        "CoinbaseExchange",
                                        "new"
                                            | "portfolio_receiver"
                                            | "market_receiver"
                                            | "lifecycle_receiver"
                                            | "capture_finder"
                                            | "capture_supported"
                                            // Assign already-built bound field values; no wire ingress.
                                            | "set_finder"
                                            | "set_supported"
                                            | "find_selected"
                                            | "supported_types"
                                            | "get_account"
                                            | "is_public_data_only"
                                    )
                                    | ("CoinbaseSpotExchange", "new" | "portfolio_receiver")
                                    // Socket lifecycle: dialling and frame ROUTING.
                                    // on_message hands the payload to the already
                                    // classified parsers rather than decoding it here.
                                    | (
                                        "CoinbasePublicWebsocket" | "CoinbasePrivateWebsocket",
                                        "new" | "connect_and_subscribe" | "on_open" | "on_message"
                                    )
                                    | ("CoinbasePublicWebsocket", "listen_ohlcv" | "listen_order_book")
                                    // Delegate into the classified native formatter,
                                    // and operate on this same keyed snapshot root.
                                    | ("CoinbasePrivateWebsocket", "handle_user_event_orders" | "reset_order_snapshot_assembly" | "merge_snapshot_orders" | "apply_order_snapshot_batch")
                            ),
                            "unclassified inherent decoder {file}::{owner}"
                        );
                    }
                }
            }
        }
        let mut expected: BTreeSet<_> = types
            .iter()
            .filter(|(name, is_struct)| {
                !**is_struct
                    && name.as_str() != "CoinbaseStarbaseFalse"
                    && name.as_str() != "CoinbaseStarbaseCancelOnDisconnectScope"
                    && !(file == "public_market_listeners"
                        && name.as_str() == "PublicListenerConnectivity")
                    && !matches!(
                        name.as_str(),
                        "CoinbaseStarbaseBaseAmountSource"
                            | "CoinbaseStarbaseBaseAmountUnit"
                            | "CoinbaseStarbaseContractAmountSource"
                            | "CoinbaseStarbaseContractAmountUnit"
                            | "CoinbaseStarbaseUnresolvedTimestampUnit"
                            | "CoinbaseStarbaseClosePositionType"
                            | "CoinbaseStarbaseOtocoLegType"
                            | "CoinbaseStarbasePlaceOrderMethod"
                            | "CoinbaseStarbaseEditMethod"
                            | "CoinbaseStarbaseWriteAmountIntent"
                            | "CoinbaseStarbaseWriteGate"
                            | "CoinbaseStarbaseWriteOrderType"
                            | "CoinbaseStarbaseLinkedOrderType"
                            | "CoinbaseStarbaseTriggerFillCondition"
                            | "CoinbaseStarbaseWriteSide"
                            | "CoinbaseStarbaseWriteParityBlocker"
                            | "CoinbaseStarbaseNever"
                            | "CoinbaseStarbaseOrderPrice"
                            | "CoinbaseStarbaseMarketPrice"
                            | "CoinbaseStarbaseTradeLiquidity"
                            | "CoinbaseStarbaseParityResult"
                            | "CoinbaseStarbaseAccountCurrency"
                            | "CoinbaseStarbasePositionDirection"
                            | "CoinbaseStarbaseMarginModel"
                            | "CoinbaseStarbaseOrderDirection"
                            | "CoinbaseStarbaseOrderState"
                            | "CoinbaseStarbaseOrderType"
                            | "CoinbaseStarbaseTimeInForce"
                            | "CoinbaseStarbaseTradeState"
                            | "CoinbaseStarbaseTradeType"
                            | "CoinbaseStarbaseTriggerType"
                            | "CoinbaseStarbaseFeeCurrency"
                            | "CoinbaseStarbaseParityBlocker"
                            | "CoinbaseStarbaseSubscriptionMethod"
                            | "CoinbaseStarbasePublicChannel"
                            | "CoinbaseStarbaseInstrumentStateCurrency"
                            | "CoinbaseStarbaseMarketParityStatus"
                            | "StarbaseBookAction"
                            | "CoinbaseStarbaseBookFrameType"
                            | "CoinbaseStarbaseOrderBookDepth"
                            | "CoinbaseStarbaseUsdc"
                            | "CoinbaseStarbaseFuture"
                            | "CoinbaseStarbaseLinear"
                            | "CoinbaseStarbasePerpetual"
                            | "CoinbaseStarbaseBaseCurrency"
                            | "CoinbaseStarbaseBookState"
                            | "CoinbaseStarbaseBookInterval"
                            | "CoinbaseStarbaseChartResolution"
                            | "CoinbaseStarbaseMarketParityBlocker"
                            | "CoinbaseStarbaseTokenType"
                            | "CoinbaseStarbaseTransportKind"
                            | "CoinbaseStarbaseHeartbeatSignal"
                            // 36f5fca044 adds an internal algorithm discriminator without Serde ingress.
                            | "CoinbaseSignatureAlgorithm"
                            // f3fd1bb45c adds the outbound-only RPC record's singleton literal.
                            | "CoinbaseStarbaseJsonRpcVersion"
                            | "CoinbaseStarbaseHttpMethod"
                            | "CoinbaseStarbaseJsonRpcResponse"
                            | "CoinbaseStarbaseRequestError"
                            | "JsonKeyScanResult"
                            | "CoinbaseStarbaseFrameValue"
                            | "CoinbaseStarbaseUnknownFrameReason"
                            | "CoinbaseStarbaseUnsupportedSubscriptionReason"
                            | "CoinbaseStarbaseFrameClassificationScope"
                            | "CoinbaseStarbasePublicTestOutcome"
                            | "CoinbaseStarbasePublicFrameClassification"

                            | "CoinbaseStarbaseSessionError"
                            | "CoinbaseStarbaseRefreshInterval"
                            | "PromiseReaction"
                            // Native listener/callback dispatch discriminators, with no Serde/wire decoding.
                            | "PublicSocketListener"
                            | "PublicStreamHandler"
                            | "PublicStreamMessage"
                            | "PublicData"
                            | "PublicMarketQuery"
                    )
            })
            .map(|(name, _)| name.clone())
            .collect();
        if file == "portfolios_balances" {
            expected.insert("PortfolioState".to_owned());
        }
        let expected_outputs = if file == "types" {
            expected.extend(RETAINED_NUMERIC_STRUCTS.map(str::to_owned));
            RETAINED_NUMERIC_STRUCTS
                .into_iter()
                .chain(RETAINED_NUMERIC_ENUMS)
                .map(str::to_owned)
                .collect()
        } else {
            BTreeSet::new()
        };
        assert_eq!(
            outputs, expected_outputs,
            "{file}: reviewed native numeric output registrations"
        );
        assert_eq!(manual, expected, "{file}: manual strict decoder inventory");
    }
}
