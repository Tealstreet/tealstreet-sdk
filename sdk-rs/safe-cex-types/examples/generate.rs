use safe_cex_types::*;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use syn::{Item, ItemEnum};
use ts_rs::{Config, TS};

#[derive(Serialize)]
struct EnumMember {
    name: String,
    value: String,
}

fn derives_ts(item: &ItemEnum) -> syn::Result<bool> {
    for attr in &item.attrs {
        if attr.path().is_ident("derive") {
            let paths = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            )?;
            if paths
                .iter()
                .any(|path| path.segments.last().is_some_and(|s| s.ident == "TS"))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// The literal of a `#[path = "..."]` attribute, if the module carries one.
fn explicit_module_path(module: &syn::ItemMod) -> Option<String> {
    module.attrs.iter().find_map(|attribute| {
        if !attribute.path().is_ident("path") {
            return None;
        }
        match &attribute.meta {
            syn::Meta::NameValue(value) => match &value.value {
                syn::Expr::Lit(literal) => match &literal.lit {
                    syn::Lit::Str(value) => Some(value.value()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    })
}

/// `module_dir` is where ordinary child modules resolve; `declaring_dir` is the
/// directory of the FILE these items came from, which is what Rust resolves a
/// `#[path]` against. They differ as soon as a module is loaded from a file.
fn collect_enums(
    items: &[Item],
    module_dir: &Path,
    declaring_dir: &Path,
    enums: &mut BTreeMap<String, ItemEnum>,
) -> Result<(), Box<dyn std::error::Error>> {
    for item in items {
        match item {
            Item::Enum(item) if derives_ts(item)? => {
                if enums.insert(item.ident.to_string(), item.clone()).is_some() {
                    return Err(syn::Error::new_spanned(item, "duplicate TS enum name").into());
                }
            }
            Item::Mod(module) => {
                let child_dir = module_dir.join(module.ident.to_string());
                if let Some((_, items)) = &module.content {
                    // An inline module nests both resolutions under its own name.
                    collect_enums(items, &child_dir, &child_dir, enums)?;
                } else {
                    // `#[path]` used to be refused outright, which made the whole
                    // generator fail the moment src/exchange_metadata.rs declared
                    // one. Refusing it is only safe while nothing uses it: an
                    // unresolvable module is a module whose TS enums are silently
                    // absent, so resolve it the way rustc does instead - relative
                    // to the declaring file's directory.
                    let file = if let Some(custom) = explicit_module_path(module) {
                        let path = Path::new(&custom);
                        if path
                            .components()
                            .any(|part| part == std::path::Component::ParentDir)
                            || path.is_absolute()
                        {
                            return Err(syn::Error::new_spanned(
                                module,
                                "#[path] must stay inside the crate directory",
                            )
                            .into());
                        }
                        let file = declaring_dir.join(path);
                        if !file.is_file() {
                            return Err(syn::Error::new_spanned(
                                module,
                                "#[path] module source is missing",
                            )
                            .into());
                        }
                        file
                    } else {
                        let flat = module_dir.join(format!("{}.rs", module.ident));
                        let nested = child_dir.join("mod.rs");
                        match (flat.is_file(), nested.is_file()) {
                            (true, false) => flat,
                            (false, true) => nested,
                            _ => {
                                return Err(syn::Error::new_spanned(
                                    module,
                                    "missing or ambiguous Rust module source",
                                )
                                .into());
                            }
                        }
                    };
                    let next_declaring_dir = file
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| module_dir.to_path_buf());
                    let parsed = syn::parse_file(&fs::read_to_string(&file)?)?;
                    collect_enums(&parsed.items, &child_dir, &next_declaring_dir, enums)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn enum_members(item: &ItemEnum) -> Result<Vec<EnumMember>, Box<dyn std::error::Error>> {
    let mut members = Vec::new();
    let mut values = BTreeSet::new();
    for variant in &item.variants {
        if !matches!(variant.fields, syn::Fields::Unit) || variant.discriminant.is_some() {
            return Err(syn::Error::new_spanned(variant, "expected a unit string variant").into());
        }
        let mut value = None;
        for attr in &variant.attrs {
            if attr.path().is_ident("serde") {
                attr.parse_nested_meta(|meta| {
                    if !meta.path.is_ident("rename") || value.is_some() {
                        return Err(meta.error("expected exactly one serde rename"));
                    }
                    value = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                    Ok(())
                })?;
            }
        }
        let value = value.ok_or_else(|| {
            syn::Error::new_spanned(variant, "missing explicit serde string rename")
        })?;
        if !values.insert(value.clone()) {
            return Err(syn::Error::new_spanned(variant, "duplicate serde enum literal").into());
        }
        members.push(EnumMember {
            name: variant.ident.to_string(),
            value,
        });
    }
    Ok(members)
}

/// String unions have wire literals but no oracle member identifiers.
#[derive(Debug, Serialize, PartialEq, Eq)]
struct UnionName {
    current: String,
    name: String,
    value: String,
    fallback: Option<String>,
}

fn literal_identifier(value: &str) -> Result<String, &'static str> {
    if let Some((&unit, digits)) = value.as_bytes().split_last()
        && !digits.is_empty()
        && digits.iter().all(u8::is_ascii_digit)
    {
        // Case matters: 1m is a minute and 1M is a month, so never case-fold units.
        let prefix = match unit {
            b's' => Some("S"),
            b'm' => Some("M"),
            b'h' => Some("H"),
            b'd' => Some("D"),
            b'w' => Some("W"),
            b'M' => Some("MN"),
            _ => None,
        };
        if let Some(prefix) = prefix {
            return Ok(format!("{prefix}{}", &value[..value.len() - 1]));
        }
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("literal contains unsupported punctuation or non-ASCII characters");
    }
    let mut words = Vec::new();
    for part in value.split(['_', '-']).filter(|part| !part.is_empty()) {
        let chars: Vec<_> = part.chars().collect();
        let mut start = 0;
        for i in 1..chars.len() {
            if chars[i].is_ascii_uppercase()
                && (chars[i - 1].is_ascii_lowercase()
                    || chars[i - 1].is_ascii_digit()
                    || (chars[i - 1].is_ascii_uppercase()
                        && chars.get(i + 1).is_some_and(char::is_ascii_lowercase)))
            {
                words.push(chars[start..i].iter().collect::<String>());
                start = i;
            }
        }
        words.push(chars[start..].iter().collect::<String>());
    }
    let name: String = words
        .iter()
        .map(|word| {
            let mut chars = word.chars();
            let first = chars.next().unwrap().to_ascii_uppercase();
            format!("{first}{}", chars.as_str().to_ascii_lowercase())
        })
        .collect();
    // Check the derived spelling: Static is legal, while Self cannot even be r#Self.
    // Raw identifiers can escape other genuine keyword collisions when Rust permits.
    let keywords = "as async await break const continue crate dyn else enum extern false fn for gen if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while abstract become box do final macro override priv typeof unsized virtual yield try union macro_rules safe raw";
    if keywords.split_whitespace().any(|word| word == name) {
        return Err("derived identifier is a reserved Rust word");
    }
    if name.is_empty() || syn::parse_str::<syn::Ident>(&name).is_err() {
        return Err("camel-cased literal is not a Rust identifier");
    }
    if name
        .strip_prefix("Value")
        .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()))
    {
        return Err("literal normalizes to the reserved ValueN fallback namespace");
    }
    Ok(name)
}

fn union_names(item: &ItemEnum) -> Result<Vec<UnionName>, Box<dyn std::error::Error>> {
    let members = enum_members(item)?;
    let candidates: Vec<_> = members
        .iter()
        .map(|member| literal_identifier(&member.value))
        .collect();
    let mut counts = BTreeMap::new();
    for name in candidates.iter().flatten() {
        *counts.entry(name.clone()).or_insert(0) += 1;
    }
    Ok(members
        .into_iter()
        .zip(candidates)
        .enumerate()
        .map(|(i, (member, candidate))| {
            let (name, fallback) = match candidate {
                Ok(name) if counts[&name] == 1 => (name, None),
                Ok(name) => (
                    format!("Value{i}"),
                    Some(format!("literal collides after normalization as {name}")),
                ),
                Err(reason) => (format!("Value{i}"), Some(reason.to_owned())),
            };
            UnionName {
                current: member.name,
                name,
                value: member.value,
                fallback,
            }
        })
        .collect())
}

fn is_union(item: &ItemEnum) -> bool {
    item.attrs.iter().any(|attr| match &attr.meta {
        syn::Meta::NameValue(meta) if meta.path.is_ident("doc") => match &meta.value {
            syn::Expr::Lit(expr) => {
                matches!(&expr.lit, syn::Lit::Str(text) if text.value().contains("@ts-union"))
            }
            _ => false,
        },
        _ => false,
    })
}

fn validate_union_names(item: &ItemEnum) -> Result<(), Box<dyn std::error::Error>> {
    for member in union_names(item)? {
        if member.current != member.name {
            return Err(format!(
                "{}::{} ({:?}): expected {}; run generate --union-names for derived identifiers",
                item.ident, member.current, member.value, member.name
            )
            .into());
        }
    }
    Ok(())
}

fn reset_bindings(bindings: &Path) -> std::io::Result<()> {
    if bindings.exists() {
        fs::remove_dir_all(bindings)?;
    }
    fs::create_dir_all(bindings.join("raw"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_dir = crate_dir.join("src");
    let parsed = syn::parse_file(&fs::read_to_string(source_dir.join("lib.rs"))?)?;
    let mut enums = BTreeMap::new();
    collect_enums(&parsed.items, &source_dir, &source_dir, &mut enums)?;
    let unions: Vec<_> = enums.values().filter(|item| is_union(item)).collect();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--union-names"] {
        let mut names = BTreeMap::new();
        for item in unions {
            names.insert(item.ident.to_string(), union_names(item)?);
        }
        println!("{}", serde_json::to_string_pretty(&names)?);
        return Ok(());
    }
    if !args.is_empty() {
        return Err("expected no arguments or --union-names".into());
    }
    // Validate before reset so a naming failure cannot remove existing bindings.
    for item in unions {
        validate_union_names(item)?;
    }
    let bindings = crate_dir.join("bindings");
    reset_bindings(&bindings)?;
    let raw = bindings.join("raw");
    let config = Config::new().with_out_dir(&raw);
    Market::export_all(&config)?;
    Order::export_all(&config)?;
    Position::export_all(&config)?;
    CoinBalance::export_all(&config)?;
    Balance::export_all(&config)?;
    Ticker::export_all(&config)?;
    PublicTrade::export_all(&config)?;
    OptionRight::export_all(&config)?;
    OptionTerms::export_all(&config)?;
    OptionGreeks::export_all(&config)?;
    OptionQuote::export_all(&config)?;
    OptionChainQuery::export_all(&config)?;
    OptionChainRow::export_all(&config)?;
    AccountGreeks::export_all(&config)?;
    Candle::export_all(&config)?;
    Execution::export_all(&config)?;
    OrderBookOrders::export_all(&config)?;
    OrderBook::export_all(&config)?;
    PlaceOrderOpts::export_all(&config)?;
    HistoricalTrade::export_all(&config)?;
    HistoricalTradeOptions::export_all(&config)?;
    OHLCVOptions::export_all(&config)?;
    OHLCVSOptions::export_all(&config)?;
    ExecutionsOptions::export_all(&config)?;
    OrderHistoryOptions::export_all(&config)?;
    HistoricalOrder::export_all(&config)?;
    StoreDataLoaded::export_all(&config)?;
    StoreOptions::export_all(&config)?;
    FetchedLeverage::export_all(&config)?;
    DisplayedMargin::export_all(&config)?;
    IncomeHistoryOptions::export_all(&config)?;
    ExchangeName::export_all(&config)?;
    ExchangeAccountConfig::export_all(&config)?;
    ExchangeOptions::export_all(&config)?;
    ValidateAccountResult::export_all(&config)?;
    UpdateOrderOpts::export_all(&config)?;
    ExchangeAccount::export_all(&config)?;
    DataProvenance::export_all(&config)?;
    ClosedPnlEntry::export_all(&config)?;
    IncomeCategory::export_all(&config)?;
    IncomeRecord::export_all(&config)?;
    OrderRiskProfile::export_all(&config)?;
    RequestOutcome::export_all(&config)?;
    RequestOutcomeCounts::export_all(&config)?;
    MessageHandlerStatsType::export_all(&config)?;
    SecondsTimeframe::export_all(&config)?;
    ExtendedTimeframe::export_all(&config)?;
    AnyTimeframe::export_all(&config)?;
    StoreUpdateIds::export_all(&config)?;
    PublicExchangeOptions::export_all(&config)?;
    let mut metadata = BTreeMap::new();
    let mut unions = BTreeMap::new();
    for entry in fs::read_dir(&raw)? {
        let entry = entry?;
        let name = entry
            .path()
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        if let Some(item) = enums.get(&name) {
            let docs = item
                .attrs
                .iter()
                .filter(|a| a.path().is_ident("doc"))
                .filter_map(|a| match &a.meta {
                    syn::Meta::NameValue(n) => match &n.value {
                        syn::Expr::Lit(l) => match &l.lit {
                            syn::Lit::Str(s) => Some(s.value()),
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<String>();
            if docs.contains("@ts-shape") {
                continue;
            }
            if docs.contains("@ts-union") {
                unions.insert(name, enum_members(item)?);
            } else {
                metadata.insert(name, enum_members(item)?);
            }
        }
    }
    fs::write(
        bindings.join("enum-members.json"),
        serde_json::to_string_pretty(&metadata)? + "\n",
    )?;
    fs::write(
        bindings.join("string-unions.json"),
        serde_json::to_string_pretty(&unions)? + "\n",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn enums(source: &str, dir: &Path) -> BTreeMap<String, ItemEnum> {
        let mut found = BTreeMap::new();
        collect_enums(
            &syn::parse_file(source).unwrap().items,
            dir,
            dir,
            &mut found,
        )
        .unwrap();
        found
    }

    #[test]
    fn literal_names_preserve_word_boundaries_and_reject_unnamable_literals() {
        for (literal, expected) in [
            ("ok", "Ok"),
            ("rateLimit", "RateLimit"),
            ("auth", "Auth"),
            ("realized_pnl", "RealizedPnl"),
            ("BRA_BTL", "BraBtl"),
            ("HTTPError", "HttpError"),
            ("rate__limit", "RateLimit"),
            ("lighter-robinhood", "LighterRobinhood"),
            ("tealstreet-dex", "TealstreetDex"),
            ("in-process", "InProcess"),
            ("-rate__--limit_", "RateLimit"),
            // Keyword-shaped literals derive legal Pascal identifiers.
            ("static", "Static"),
            ("type", "Type"),
            ("match", "Match"),
            ("move", "Move"),
            ("box", "Box"),
            ("ref", "Ref"),
            ("Crate", "Crate"),
            ("gen", "Gen"),
            ("macro_rules", "MacroRules"),
        ] {
            assert_eq!(literal_identifier(literal), Ok(expected.to_owned()));
        }
        for literal in [
            // Timeframes now derive names; retain a genuinely unsupported unit probe.
            "1q", "42", "x.perp", "---___", "", "_", "é", "Self", "Value0",
        ] {
            assert!(literal_identifier(literal).is_err(), "{literal}");
        }
    }

    #[test]
    fn separator_collisions_and_genuine_reserved_identifiers_still_fall_back() {
        let item: ItemEnum = syn::parse_str(
            r#"enum Collision {
            #[serde(rename = "rate-limit")] Value0,
            #[serde(rename = "rate_limit")] Value1,
            #[serde(rename = "rateLimit")] Value2,
            #[serde(rename = "Self")] Value3,
            #[serde(rename = "static")] Static,
        }"#,
        )
        .unwrap();
        validate_union_names(&item).unwrap();
        let names = union_names(&item).unwrap();
        assert!(names[..3].iter().all(|member| {
            member
                .fallback
                .as_ref()
                .is_some_and(|reason| reason.contains("collides"))
        }));
        assert!(names[3].fallback.as_ref().unwrap().contains("reserved"));
        assert!(names[4].fallback.is_none());
    }

    #[test]
    fn timeframe_units_are_case_sensitive_and_share_normalization_collision_checks() {
        for (literal, expected) in [
            ("1s", "S1"),
            ("15m", "M15"),
            ("4h", "H4"),
            ("1d", "D1"),
            ("1w", "W1"),
            ("1M", "MN1"),
            ("01m", "M01"),
        ] {
            assert_eq!(literal_identifier(literal), Ok(expected.to_owned()));
        }
        for literal in [
            "1q", "1H", "1S", "1D", "1W", "1mm", "1.5m", "-1h", "+1h", " 1h", "1h ", "1", "",
        ] {
            assert!(literal_identifier(literal).is_err(), "{literal}");
        }
        let combined: ItemEnum = syn::parse_str(
            r#"enum Combined {
            #[serde(rename = "1m")] M1,
            #[serde(rename = "1M")] MN1,
        }"#,
        )
        .unwrap();
        validate_union_names(&combined).unwrap();
        let names = union_names(&combined).unwrap();
        assert_eq!(names[0].name, "M1");
        assert_eq!(names[1].name, "MN1");
        assert!(names.iter().all(|member| member.fallback.is_none()));
        let collision: ItemEnum = syn::parse_str(
            r#"enum Collision {
            #[serde(rename = "1m")] Value0,
            #[serde(rename = "m1")] Value1,
        }"#,
        )
        .unwrap();
        validate_union_names(&collision).unwrap();
        assert!(union_names(&collision).unwrap().iter().all(|member| {
            member
                .fallback
                .as_ref()
                .is_some_and(|reason| reason.contains("collides"))
        }));
    }

    #[test]
    fn collisions_fall_back_per_variant_and_stale_positional_names_fail() {
        let parsed: ItemEnum = syn::parse_str(
            r#"enum Outcome {
            #[serde(rename = "ok")] Ok,
            #[serde(rename = "rateLimit")] Value1,
            #[serde(rename = "rate_limit")] Value2,
            #[serde(rename = "1q")] Value3,
            #[serde(rename = "auth")] Auth,
        }"#,
        )
        .unwrap();
        let names = union_names(&parsed).unwrap();
        assert_eq!(
            names
                .iter()
                .map(|member| member.name.as_str())
                .collect::<Vec<_>>(),
            ["Ok", "Value1", "Value2", "Value3", "Auth"]
        );
        assert!(names[0].fallback.is_none());
        assert!(names[1].fallback.as_ref().unwrap().contains("collides"));
        assert!(names[2].fallback.as_ref().unwrap().contains("collides"));
        assert!(names[3].fallback.as_ref().unwrap().contains("identifier"));
        validate_union_names(&parsed).unwrap();
        let stale: ItemEnum = syn::parse_str(
            r#"enum Outcome {
            #[serde(rename = "auth")] Value0,
        }"#,
        )
        .unwrap();
        assert!(
            validate_union_names(&stale)
                .unwrap_err()
                .to_string()
                .contains("expected Auth")
        );
        let reserved: ItemEnum = syn::parse_str(
            r#"enum Reserved {
            #[serde(rename = "Value1")] Value0,
            #[serde(rename = "1q")] Value1,
            #[serde(rename = "Type")] Type,
        }"#,
        )
        .unwrap();
        validate_union_names(&reserved).unwrap();
    }

    #[test]
    fn nested_modules_keep_ts_enums_and_ignore_internal_rust_enums() {
        let found = enums(
            r#"pub mod margin { pub mod inner {
            #[derive(ts_rs::TS)] pub enum MarginType {
                #[serde(rename = "isolated")] Isolated, #[serde(rename = "cross")] Cross,
            }
        }} pub use margin::inner::MarginType;
        pub enum InternalError { Parse(String) }"#,
            Path::new("unused"),
        );
        assert_eq!(found.len(), 1);
        let members = enum_members(&found["MarginType"]).unwrap();
        assert_eq!(members[0].name, "Isolated");
        assert_eq!(members[1].value, "cross");
    }

    #[test]
    fn unsupported_exported_enums_and_duplicate_literals_fail() {
        for source in [
            "#[derive(TS)] pub enum Side { Buy }",
            r#"#[derive(TS)] pub enum Side { #[serde(rename = "buy")] Buy(u8) }"#,
            r#"#[derive(TS)] pub enum Side { #[serde(rename = "buy", alias = "BUY")] Buy }"#,
            r#"#[derive(TS)] pub enum Side { #[serde(rename = "buy")] Buy, #[serde(rename = "buy")] Sell }"#,
        ] {
            assert!(enum_members(&enums(source, Path::new("unused"))["Side"]).is_err());
        }
        let mut found = BTreeMap::new();
        let parsed = syn::parse_file(
            "mod a { #[derive(TS)] enum Side {} } mod b { #[derive(TS)] enum Side {} }",
        )
        .unwrap();
        assert!(
            collect_enums(
                &parsed.items,
                Path::new("unused"),
                Path::new("unused"),
                &mut found
            )
            .is_err()
        );
    }

    #[test]
    fn explicit_path_modules_resolve_against_the_declaring_file() {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "safe-cex-bindings-path-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("side_generated.rs"),
            r#"#[derive(TS)] pub enum Side { #[serde(rename = "buy")] Buy }"#,
        )
        .unwrap();

        // The case that used to fail the whole generator: src/exchange_metadata.rs
        // declares exactly this shape for its generated sibling.
        let found = enums(
            r#"#[path = "side_generated.rs"] mod generated; pub use generated::Side;"#,
            &dir,
        );
        assert!(found.contains_key("Side"), "#[path] module was not walked");

        // Escaping the crate directory and naming a missing file both refuse,
        // because an unresolvable module is one whose TS enums go missing.
        for source in [
            r#"#[path = "../side_generated.rs"] mod generated;"#,
            r#"#[path = "/etc/passwd"] mod generated;"#,
            r#"#[path = "absent_generated.rs"] mod generated;"#,
        ] {
            let mut refused = BTreeMap::new();
            assert!(
                collect_enums(
                    &syn::parse_file(source).unwrap().items,
                    &dir,
                    &dir,
                    &mut refused,
                )
                .is_err(),
                "{source}"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_modules_and_generation_cleanup_preserve_the_export_graph() {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "safe-cex-bindings-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(dir.join("outer/inner")).unwrap();
        fs::write(dir.join("outer.rs"), "pub mod inner; pub use inner::Side;").unwrap();
        fs::write(
            dir.join("outer/inner/mod.rs"),
            r#"#[derive(TS)] pub enum Side { #[serde(rename = "buy")] Buy }"#,
        )
        .unwrap();
        assert!(enums("pub mod outer; pub use outer::Side;", &dir).contains_key("Side"));
        let bindings = dir.join("bindings");
        reset_bindings(&bindings).unwrap();
        fs::write(bindings.join("raw/Ghost.ts"), "ghost").unwrap();
        fs::write(bindings.join("Ghost.ts"), "ghost").unwrap();
        reset_bindings(&bindings).unwrap();
        assert!(!bindings.join("raw/Ghost.ts").exists());
        assert!(!bindings.join("Ghost.ts").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
