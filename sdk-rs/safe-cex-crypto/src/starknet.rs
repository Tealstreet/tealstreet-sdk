//! Stark-curve primitives and generic SNIP-12 typed-data hashing.
//! Venue/account handling and typed-data construction belong to callers.
use starknet_crypto::Felt;

// Validate the original declared values before Felt constructors can reduce them.
fn check_felt(value: &mut serde_json::Value) -> Result<(), String> {
    // SDK getHex tries BigInt first, retaining the original shortstring on syntax failure.
    if let Ok(number) = safe_cex_num::js_bigint(Some(value)) {
        *value = serde_json::Value::String(number.to_string());
    }
    let text = match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        _ => return Ok(()), // The typed encoder owns other value-kind refusals.
    };
    let (digits, radix, limit) = if let Some(hex) = text.strip_prefix("0x") {
        (
            hex,
            16,
            "800000000000011000000000000000000000000000000000000000000000001",
        )
    } else {
        (
            text.as_str(),
            10,
            "3618502788666131213697322783095070105623107215331596699973092056135872020481",
        )
    };
    if radix == 10
        && digits.starts_with('-')
        && !digits[1..].is_empty()
        && digits[1..].bytes().all(|byte| byte.is_ascii_digit())
    {
        if !digits[1..].is_empty() && digits[1..].bytes().all(|byte| byte == b'0') {
            *value = serde_json::Value::String("0".into());
            return Ok(());
        }
        return Err("Stark felt must satisfy 0 <= value < field prime".into());
    }
    if digits.is_empty() || !digits.chars().all(|digit| digit.is_digit(radix)) {
        return Ok(()); // Non-numeric shortstrings are encoded by the reference.
    }
    let digits = digits.trim_start_matches('0').to_ascii_lowercase();
    if digits.len() > limit.len() || (digits.len() == limit.len() && digits.as_str() >= limit) {
        return Err("Stark felt must satisfy 0 <= value < field prime".into());
    }
    Ok(())
}
fn sdk_is_hex(text: &str) -> bool {
    text.strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .is_some_and(|digits| digits.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn sdk_short_word(text: &str) -> Result<Felt, String> {
    if !text.is_ascii() || text.len() > 31 {
        return Err("Stark shortstring must be ASCII and at most 31 units".into());
    }
    // ref-bug: SDK shortstrings concatenate unpadded nibbles and leave LF/CR unchanged.
    let mut hex = String::from("0x");
    for byte in text.bytes() {
        if matches!(byte, b'\n' | b'\r') {
            hex.push(char::from(byte));
        } else {
            hex.push_str(&format!("{byte:x}"));
        }
    }
    let number = safe_cex_num::js_bigint(Some(&serde_json::Value::String(hex)))
        .map_err(|error| error.to_string())?;
    let (_, bytes) = number.to_bytes_be();
    Ok(Felt::from_bytes_be_slice(&bytes))
}

fn check_sdk_felt(value: &mut serde_json::Value) -> Result<(), String> {
    check_felt(value)?;
    if let Some(text) = value.as_str() {
        let numeric = !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
        if !numeric && !(text.len() > 2 && sdk_is_hex(text)) {
            *value = serde_json::Value::String(format!("{:#x}", sdk_short_word(text)?));
        }
    }
    Ok(())
}

fn sdk_presets() -> serde_json::Value {
    serde_json::json!({
        "u256": [{"name":"low","type":"u128"},{"name":"high","type":"u128"}],
        "TokenAmount": [{"name":"token_address","type":"ContractAddress"},{"name":"amount","type":"u256"}],
        "NftId": [{"name":"collection_address","type":"ContractAddress"},{"name":"token_id","type":"u256"}]
    })
}

fn check_fields(
    types: &serde_json::Value,
    fields: &[serde_json::Value],
    value: &mut serde_json::Value,
    depth: usize,
) -> Result<(), String> {
    let object = value
        .as_object_mut()
        .ok_or("Stark struct value must be an object")?;
    object.retain(|key, _| fields.iter().any(|field| field["name"] == key.as_str()));
    for field in fields {
        if let (Some(key), Some(kind)) = (field["name"].as_str(), field["type"].as_str())
            && let Some(value) = object.get_mut(key)
        {
            check_declared_fields(
                types,
                kind,
                value,
                field.get("contains").and_then(serde_json::Value::as_str),
                depth + 1,
            )?;
        }
    }
    Ok(())
}

fn check_declared_fields(
    types: &serde_json::Value,
    name: &str,
    value: &mut serde_json::Value,
    contains: Option<&str>,
    depth: usize,
) -> Result<(), String> {
    if depth > 128 {
        return Err("Stark typed-data nesting limit exceeded".into());
    }
    use starknet_core::types::typed_data::InlineTypeReference as Kind;
    let kind: Kind = name
        .parse()
        .map_err(|error: starknet_core::types::typed_data::TypedDataError| error.to_string())?;
    // ref-bug: a caller-defined felt recurses forever in the SDK; refuse reserved shadows.
    if types.get(name).is_some()
        && (!matches!(kind, Kind::Custom(_)) || matches!(name, "enum" | "merkletree"))
    {
        return Err(format!("Reserved Stark type cannot be redefined: {name}"));
    }
    if let Some(element) = name.strip_suffix('*') {
        if !value.is_array() {
            return Err(format!("Stark array required for {name}"));
        }
        if let Some(values) = value.as_array_mut() {
            for value in values {
                check_declared_fields(types, element, value, None, depth + 1)?;
            }
        }
    } else if let Some(fields) = types.get(name).and_then(serde_json::Value::as_array) {
        check_fields(types, fields, value, depth)?;
    } else if name == "merkletree" {
        if let (Some(kind), Some(values)) = (contains, value.as_array_mut()) {
            for value in values {
                check_declared_fields(types, kind, value, None, depth + 1)?;
            }
        }
    } else if name == "enum" {
        let (variant, payload) = match value {
            serde_json::Value::Object(object) => {
                safe_cex_num::normalize_object_order(object);
                object
                    .iter()
                    .next()
                    .map(|(key, value)| (key.clone(), value.clone()))
            }
            serde_json::Value::Array(values) => {
                values.first().cloned().map(|value| ("0".into(), value))
            }
            serde_json::Value::String(text) => text
                .encode_utf16()
                .next()
                .map(|unit| {
                    String::from_utf16(&[unit])
                        .map(|text| ("0".into(), serde_json::Value::String(text)))
                })
                .transpose()
                .map_err(|error| error.to_string())?,
            _ => None,
        }
        .ok_or("Stark enum requires a first enumerable entry")?;
        let tuple = contains
            .and_then(|name| types.get(name))
            .and_then(serde_json::Value::as_array)
            .and_then(|fields| fields.iter().find(|field| field["name"] == variant))
            .and_then(|field| field["type"].as_str())
            .and_then(sdk_tuple)
            .ok_or("Unsupported Stark enum tuple")?;
        let kinds: Vec<&str> = if tuple.is_empty() {
            Vec::new()
        } else {
            tuple.split(',').collect()
        };
        if payload
            .as_array()
            .is_some_and(|values| values.len() != kinds.len())
        {
            return Err("Stark enum tuple payload count does not match declared types".into());
        }
        let mut values = Vec::new();
        for (index, kind) in kinds.iter().enumerate() {
            let mut element = if kind.is_empty() {
                serde_json::json!("0")
            } else {
                match &payload {
                    serde_json::Value::Object(object) => object.get(&index.to_string()).cloned(),
                    serde_json::Value::Array(array) => array.get(index).cloned(),
                    serde_json::Value::String(text) => text
                        .encode_utf16()
                        .nth(index)
                        .map(|unit| String::from_utf16(&[unit]).map(serde_json::Value::String))
                        .transpose()
                        .map_err(|error| error.to_string())?,
                    _ => None,
                }
                .ok_or("Missing Stark enum tuple argument")?
            };
            if !kind.is_empty() {
                check_declared_fields(types, kind, &mut element, None, depth + 1)?;
            }
            values.push(element);
        }
        *value = serde_json::json!({ variant: values });
    } else {
        let revision_one = types.get("StarknetDomain").is_some();
        // Exempt encodings are bounded booleans, signed i128, byte arrays and hashed selector names.
        // Presets recurse through every leaf; no field-name or preset-name schema can skip a guard.
        match kind {
            Kind::Bool if revision_one => {
                if !value.is_boolean() {
                    return Err("Stark bool must be a boolean".into());
                }
            }
            Kind::String if revision_one => {
                if !value.is_string() {
                    return Err("Stark string must be a string".into());
                }
            }
            Kind::I128 if revision_one => {
                let number =
                    safe_cex_num::js_bigint(Some(value)).map_err(|error| error.to_string())?;
                let text = number.to_string();
                text.parse::<i128>()
                    .map_err(|_| "Stark i128 is out of range")?;
                *value = serde_json::Value::String(text);
            }
            Kind::Selector => {
                if value.is_null() {
                    return Err("Stark selector cannot be null".into());
                }
                let text =
                    safe_cex_num::js_string(Some(value)).map_err(|error| error.to_string())?;
                *value = serde_json::Value::String(text.clone());
                if sdk_is_hex(&text) && !revision_one {
                    check_felt(&mut value.clone())?;
                }
            }
            Kind::U256 | Kind::TokenAmount | Kind::NftId if revision_one => {
                let presets = sdk_presets();
                check_fields(
                    types,
                    presets[name].as_array().expect("known preset"),
                    value,
                    depth,
                )?;
            }
            Kind::Felt | Kind::ShortString => check_sdk_felt(value)?,
            Kind::ContractAddress | Kind::ClassHash | Kind::U128 | Kind::Timestamp
                if revision_one =>
            {
                check_felt(value)?
            }
            Kind::ContractAddress
            | Kind::ClassHash
            | Kind::U128
            | Kind::Timestamp
            | Kind::I128
            | Kind::String
            | Kind::Bool
            | Kind::U256
            | Kind::TokenAmount
            | Kind::NftId => check_sdk_felt(value)?,
            Kind::Custom(_) | Kind::Array(_) => {
                return Err(format!("Unsupported Stark type: {name}"));
            }
        }
    }
    Ok(())
}

fn normalize_declared_message(
    types: &serde_json::Value,
    name: &str,
    contains: Option<&str>,
    value: &mut starknet_core::types::typed_data::Value,
) -> Result<(), String> {
    use starknet_core::types::typed_data::Value as TypedValue;
    if let Some(element) = name.strip_suffix('*') {
        if let TypedValue::Array(values) = value {
            for value in &mut values.elements {
                normalize_declared_message(types, element, None, value)?;
            }
        }
    } else if let Some(fields) = types.get(name).and_then(serde_json::Value::as_array) {
        if let TypedValue::Object(object) = value {
            object
                .fields
                .retain(|key, _| fields.iter().any(|field| field["name"] == key.as_str()));
            for field in fields {
                if let (Some(key), Some(kind)) = (field["name"].as_str(), field["type"].as_str())
                    && let Some(value) = object.fields.get_mut(key)
                {
                    normalize_declared_message(
                        types,
                        kind,
                        field.get("contains").and_then(serde_json::Value::as_str),
                        value,
                    )?;
                }
            }
        }
    } else if name == "enum" {
        if let (Some(name), TypedValue::Object(object)) = (contains, value) {
            for (variant, values) in &mut object.fields {
                if let TypedValue::Array(values) = values {
                    let tuple = types[name]
                        .as_array()
                        .and_then(|fields| {
                            fields
                                .iter()
                                .find(|field| field["name"] == variant.as_str())
                        })
                        .and_then(|field| field["type"].as_str())
                        .and_then(sdk_tuple)
                        .ok_or("Unsupported Stark enum tuple")?;
                    for (kind, value) in tuple.split(',').zip(&mut values.elements) {
                        if !kind.is_empty() {
                            normalize_declared_message(types, kind, None, value)?;
                        }
                    }
                }
            }
        }
    } else if name == "merkletree" {
        if let (Some(kind), TypedValue::Array(values)) = (contains, value) {
            for value in &mut values.elements {
                normalize_declared_message(types, kind, None, value)?;
            }
        }
    } else if name == "i128" && types.get("StarknetDomain").is_some() {
        let signed = match value {
            TypedValue::String(text) => text
                .parse::<i128>()
                .map_err(|_| "Stark i128 is outside its signed range")?,
            TypedValue::UnsignedInteger(number) => {
                i128::try_from(*number).map_err(|_| "Stark i128 is outside its signed range")?
            }
            TypedValue::SignedInteger(number) => *number,
            _ => return Err("Stark i128 must be an integer".into()),
        };
        *value = TypedValue::SignedInteger(signed);
    }
    Ok(())
}

fn sdk_tuple(kind: &str) -> Option<&str> {
    if kind.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
        return None;
    }
    kind.strip_prefix('(')
        .and_then(|kind| kind.strip_suffix(')'))
}

fn sdk_type_hash(
    types: &serde_json::Value,
    primary: &str,
    revision: starknet_core::types::typed_data::Revision,
) -> Result<Felt, String> {
    use starknet_core::types::typed_data::Revision;
    let active = revision == Revision::V1;
    let mut types = types.clone();
    let presets = sdk_presets();
    if active {
        for (name, fields) in presets.as_object().expect("literal presets") {
            types[name] = fields.clone();
        }
    }
    fn visit(
        types: &serde_json::Value,
        name: &str,
        names: &mut Vec<String>,
        depth: usize,
        active: bool,
    ) -> Result<(), String> {
        if names.iter().any(|seen| seen == name) {
            return Ok(());
        }
        if depth > 128 {
            return Err("Stark type dependency limit exceeded".into());
        }
        if let Some(fields) = types.get(name).and_then(serde_json::Value::as_array) {
            names.push(name.into());
            for field in fields {
                let kind = field["type"].as_str().ok_or("Stark field type required")?;
                let kind = if active && kind == "enum" {
                    field["contains"]
                        .as_str()
                        .ok_or("Stark enum contains required")?
                } else {
                    kind
                };
                if let Some(tuple) = sdk_tuple(kind).filter(|_| active) {
                    for child in tuple.split(',') {
                        visit(
                            types,
                            child.strip_suffix('*').unwrap_or(child),
                            names,
                            depth + 1,
                            active,
                        )?;
                    }
                } else {
                    visit(
                        types,
                        kind.strip_suffix('*').unwrap_or(kind),
                        names,
                        depth + 1,
                        active,
                    )?;
                }
            }
        }
        Ok(())
    }
    // SDK revision 1 wraps raw names in quotes; both revisions sort UTF-16 dependencies.
    let quote = |text: &str| {
        if active {
            format!("\"{text}\"")
        } else {
            text.to_owned()
        }
    };
    let mut names = Vec::new();
    visit(&types, primary, &mut names, 0, active)?;
    if names.is_empty() {
        return Err(format!("Missing Stark type: {primary}"));
    }
    names[1..].sort_by(|left, right| left.encode_utf16().cmp(right.encode_utf16()));
    let mut signature = String::new();
    for name in names {
        let mut fields = Vec::new();
        for field in types[&name]
            .as_array()
            .ok_or("Stark type fields required")?
        {
            let kind = field["type"].as_str().ok_or("Stark field type required")?;
            let kind = if active && kind == "enum" {
                field["contains"]
                    .as_str()
                    .ok_or("Stark enum contains required")?
            } else {
                kind
            };
            let encoded = if let Some(tuple) = sdk_tuple(kind) {
                format!(
                    "({})",
                    tuple
                        .split(',')
                        .map(|token| if token.is_empty() {
                            String::new()
                        } else {
                            quote(token)
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                )
            } else {
                quote(kind)
            };
            fields.push(format!(
                "{}:{encoded}",
                quote(field["name"].as_str().ok_or("Stark field name required")?)
            ));
        }
        signature.push_str(&format!("{}({})", quote(&name), fields.join(",")));
    }
    Ok(starknet_core::utils::starknet_keccak(signature.as_bytes()))
}

fn encode_sdk_value<R: starknet_core::types::typed_data::TypeReference>(
    encoder: &starknet_core::types::typed_data::Encoder,
    raw_types: &serde_json::Value,
    reference: &R,
    value: &starknet_core::types::typed_data::Value,
    depth: usize,
) -> Result<Felt, String> {
    use starknet_core::types::typed_data::{
        ArrayValue, CommonTypeReference, ElementTypeReference, FieldDefinition, FullTypeReference,
        InlineTypeReference, ObjectValue, StructDefinition, TypeDefinition, Value,
    };
    if depth > 128 {
        return Err("Stark typed-data nesting limit exceeded".into());
    }
    let ordinary = || {
        encoder
            .encode_value(reference, value)
            .map_err(|error| error.to_string())
    };
    let word = |value: Felt| Value::String(format!("{value:#x}"));
    match reference.common() {
        CommonTypeReference::Custom(name) => {
            match (encoder.types().get_type(name), value) {
                (Some(TypeDefinition::Struct(definition)), Value::Object(object))
                    if !reference.must_be_enum() =>
                {
                    let mut fields = Vec::new();
                    let mut values = Vec::new();
                    for field in &definition.fields {
                        let Some(value) = object.fields.get(&field.name) else {
                            return ordinary();
                        };
                        values.push((
                            field.name.clone(),
                            word(encode_sdk_value(
                                encoder,
                                raw_types,
                                &field.r#type,
                                value,
                                depth + 1,
                            )?),
                        ));
                        fields.push(FieldDefinition::new(
                            field.name.clone(),
                            FullTypeReference::Felt,
                        ));
                    }
                    let original_hash = sdk_type_hash(raw_types, name, encoder.revision())?;
                    encoder
                        .encode_composite(
                            original_hash,
                            &StructDefinition { fields },
                            &ObjectValue {
                                fields: values.into_iter().collect(),
                            },
                        )
                        .map_err(|error| error.to_string())
                }
                (Some(TypeDefinition::Enum(definition)), Value::Object(object))
                    if !reference.must_be_struct() && object.fields.len() == 1 =>
                {
                    let (variant_name, value) = object.fields.iter().next().expect("one variant");
                    let Some(index) = definition
                        .variants
                        .iter()
                        .position(|variant| variant.name == *variant_name)
                    else {
                        return ordinary();
                    };
                    let Value::Array(values) = value else {
                        return ordinary();
                    };
                    let tuple = raw_types[name]
                        .as_array()
                        .and_then(|fields| {
                            fields
                                .iter()
                                .find(|field| field["name"] == variant_name.as_str())
                        })
                        .and_then(|field| field["type"].as_str())
                        .and_then(sdk_tuple)
                        .ok_or("Unsupported Stark enum tuple")?;
                    let kinds: Vec<&str> = if tuple.is_empty() {
                        Vec::new()
                    } else {
                        tuple.split(',').collect()
                    };
                    if kinds.len() != values.elements.len() {
                        return ordinary();
                    }
                    let mut encoded = Vec::new();
                    for (kind, value) in kinds.iter().zip(&values.elements) {
                        if kind.is_empty() {
                            encoded.push(word(
                                encoder
                                    .encode_value(&InlineTypeReference::Felt, value)
                                    .map_err(|error| error.to_string())?,
                            ));
                        } else {
                            let reference: InlineTypeReference = kind.parse().map_err(
                                |error: starknet_core::types::typed_data::TypedDataError| {
                                    error.to_string()
                                },
                            )?;
                            encoded.push(word(encode_sdk_value(
                                encoder,
                                raw_types,
                                &reference,
                                value,
                                depth + 1,
                            )?));
                        }
                    }
                    // ref-bug: the SDK's () parser contributes a zero word to the enum hash.
                    if kinds.is_empty() {
                        encoded.push(word(Felt::ZERO));
                    }
                    let mut synthetic = definition.clone();
                    synthetic.variants[index].tuple_types =
                        vec![InlineTypeReference::Felt; encoded.len()];
                    let object = ObjectValue {
                        fields: [(
                            variant_name.clone(),
                            Value::Array(ArrayValue { elements: encoded }),
                        )]
                        .into_iter()
                        .collect(),
                    };
                    encoder
                        .encode_enum(&synthetic, &object)
                        .map_err(|error| error.to_string())
                }
                _ => ordinary(),
            }
        }
        CommonTypeReference::Array(element) => {
            let Value::Array(values) = value else {
                return ordinary();
            };
            let mut encoded = Vec::new();
            for value in &values.elements {
                encoded.push(word(encode_sdk_value(
                    encoder,
                    raw_types,
                    element,
                    value,
                    depth + 1,
                )?));
            }
            encoder
                .encode_value(
                    &FullTypeReference::Array(ElementTypeReference::Felt),
                    &Value::Array(ArrayValue { elements: encoded }),
                )
                .map_err(|error| error.to_string())
        }
        CommonTypeReference::MerkleTree(element) => {
            let Value::Array(values) = value else {
                return ordinary();
            };
            if values.elements.len() == 1 {
                return encode_sdk_value(
                    encoder,
                    raw_types,
                    element,
                    &values.elements[0],
                    depth + 1,
                );
            }
            let mut encoded = Vec::new();
            for value in &values.elements {
                encoded.push(word(encode_sdk_value(
                    encoder,
                    raw_types,
                    element,
                    value,
                    depth + 1,
                )?));
            }
            encoder
                .encode_merkletree(
                    &InlineTypeReference::Felt,
                    &ArrayValue { elements: encoded },
                )
                .map_err(|error| error.to_string())
        }
        CommonTypeReference::String
            if encoder.revision() == starknet_core::types::typed_data::Revision::V1 =>
        {
            let Value::String(text) = value else {
                return ordinary();
            };
            if !text.is_ascii() {
                return Err("Stark SDK byte-array strings must be ASCII".into());
            }
            let full = text.len() / 31;
            let mut words = vec![Felt::from(full)];
            for index in 0..full {
                words.push(sdk_short_word(&text[index * 31..(index + 1) * 31])?);
            }
            let remainder = &text[full * 31..];
            words.push(if remainder.is_empty() {
                Felt::ZERO
            } else {
                sdk_short_word(remainder)?
            });
            words.push(Felt::from(remainder.len()));
            Ok(starknet_crypto::poseidon_hash_many(&words))
        }
        CommonTypeReference::Selector => {
            let Value::String(text) = value else {
                return ordinary();
            };
            if sdk_is_hex(text) {
                let number =
                    safe_cex_num::js_bigint(Some(&serde_json::Value::String(text.clone())))
                        .map_err(|error| error.to_string())?;
                let (_, bytes) = number.to_bytes_be();
                Ok(Felt::from_bytes_be_slice(&bytes))
            } else {
                Ok(starknet_core::utils::starknet_keccak(text.as_bytes()))
            }
        }
        _ if encoder.revision() == starknet_core::types::typed_data::Revision::V0 => encoder
            .encode_value(&InlineTypeReference::Felt, value)
            .map_err(|error| error.to_string()),
        _ => ordinary(),
    }
}

/// Parse an original Stark felt string without allowing field-modulus reduction.
pub fn parse_felt(input: &str) -> Result<Felt, String> {
    let mut value = serde_json::Value::String(input.into());
    check_felt(&mut value)?;
    Felt::from_dec_str(value.as_str().expect("string remains a string"))
        .map_err(|error| error.to_string())
}

/// Hash raw SNIP-12 typed data (revision 0 or 1) for the supplied account
/// address. This authenticates typed fields, not the original JSON formatting.
pub fn hash_typed_data(raw: &str, address: &Felt) -> Result<Felt, String> {
    let mut raw_value: serde_json::Value =
        serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if !raw_value.is_object() || !raw_value["types"].is_object() || !raw_value["domain"].is_object()
    {
        return Err("Stark typed data, types and domain must be objects".into());
    }
    for fields in raw_value["types"]
        .as_object()
        .into_iter()
        .flat_map(|types| types.values())
    {
        let fields = fields
            .as_array()
            .ok_or("Stark type declaration must be an array")?;
        for field in fields {
            if !field.is_object() || !field["name"].is_string() || !field["type"].is_string() {
                return Err("Stark field declaration must be a named object".into());
            }
        }
    }
    let revision = raw_value["domain"].get("revision");
    let active_revision = raw_value["types"].get("StarknetDomain").is_some()
        && revision
            .filter(|value| !value.is_null())
            .and_then(|value| safe_cex_num::js_string(Some(value)).ok())
            .is_some_and(|text| text == "1");
    let legacy_revision = raw_value["types"].get("StarkNetDomain").is_some()
        && revision.is_none_or(|value| value.is_null() || value.as_str() == Some("0"));
    if !(active_revision || legacy_revision) {
        return Err("Invalid Stark typed-data revision".into());
    }
    let types = raw_value["types"].clone();
    let primary = raw_value["primaryType"].as_str().unwrap_or("").to_owned();
    check_declared_fields(&types, &primary, &mut raw_value["message"], None, 0)?;
    for domain in ["StarkNetDomain", "StarknetDomain"] {
        if types.get(domain).is_some() {
            check_declared_fields(&types, domain, &mut raw_value["domain"], None, 0)?;
        }
    }
    // Native metadata requires canonical order; preserve the source definition for hashing.
    let canonical = serde_json::json!([
        {"name":"name","type":"felt"},
        {"name":"version","type":"felt"},
        {"name":"chainId","type":"felt"}
    ]);
    let mut domain_override = None;
    if let Some(original) = types
        .get("StarkNetDomain")
        .and_then(serde_json::Value::as_array)
    {
        let mut sorted = original.clone();
        sorted.sort_by_key(|field| field["name"].as_str().unwrap_or("").to_owned());
        let mut expected = canonical.as_array().expect("literal array").clone();
        expected.sort_by_key(|field| field["name"].as_str().expect("literal name").to_owned());
        if sorted == expected && types["StarkNetDomain"] != canonical {
            let definition: starknet_core::types::typed_data::TypeDefinition =
                serde_json::from_value(types["StarkNetDomain"].clone())
                    .map_err(|error| error.to_string())?;
            let domain_fields = raw_value["domain"]
                .as_object()
                .expect("validated domain object")
                .iter()
                .filter(|(key, _)| matches!(key.as_str(), "name" | "version" | "chainId"))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            let domain: starknet_core::types::typed_data::Value =
                serde_json::from_value(serde_json::Value::Object(domain_fields))
                    .map_err(|error| error.to_string())?;
            let starknet_core::types::typed_data::TypeDefinition::Struct(definition) = definition
            else {
                return Err("Stark domain must be a struct".into());
            };
            let starknet_core::types::typed_data::Value::Object(domain) = domain else {
                return Err("Stark domain must be an object".into());
            };
            let fields = original
                .iter()
                .map(|field| format!("{}:felt", field["name"].as_str().expect("validated name")))
                .collect::<Vec<_>>()
                .join(",");
            let type_hash = starknet_core::utils::starknet_keccak(
                format!("StarkNetDomain({fields})").as_bytes(),
            );
            domain_override = Some((definition, domain, type_hash));
            raw_value["types"]["StarkNetDomain"] = canonical;
        }
    }
    // Normalize only parser metadata; raw definitions still determine every authenticated type hash.
    let enum_names: std::collections::BTreeSet<&str> = types
        .as_object()
        .expect("validated types")
        .values()
        .filter_map(serde_json::Value::as_array)
        .flatten()
        .filter(|field| field["type"] == "enum")
        .filter_map(|field| field["contains"].as_str())
        .collect();
    for (name, fields) in raw_value["types"].as_object_mut().expect("validated types") {
        let fields = fields.as_array_mut().expect("validated fields");
        let enum_definition = enum_names.contains(name.as_str())
            || fields.iter().any(|field| {
                field["type"]
                    .as_str()
                    .is_some_and(|kind| kind.starts_with('('))
            });
        if enum_definition {
            for field in fields {
                let kind = field["type"].as_str().expect("validated type");
                let tuple = kind.strip_prefix('(').and_then(|v| v.strip_suffix(')'));
                let count = tuple.map_or(0, |tuple| {
                    if tuple.is_empty() {
                        0
                    } else {
                        tuple.split(',').count()
                    }
                });
                field["type"] =
                    serde_json::Value::String(format!("({})", vec!["felt"; count].join(",")));
            }
        }
    }
    let validated_raw = serde_json::to_string(&raw_value).map_err(|error| error.to_string())?;
    let data: starknet_core::types::TypedData =
        serde_json::from_str(&validated_raw).map_err(|error| error.to_string())?;
    let mut message = data.message().clone();
    normalize_declared_message(
        &types,
        raw_value["primaryType"].as_str().unwrap_or(""),
        None,
        &mut message,
    )?;
    let data = starknet_core::types::TypedData::new(
        data.encoder().types().clone(),
        data.encoder().domain(),
        data.primary_type().clone(),
        message,
    )
    .map_err(|error| error.to_string())?;
    {
        let domain_hash = if let Some((definition, domain, type_hash)) = domain_override {
            data.encoder()
                .encode_composite(type_hash, &definition, &domain)
                .map_err(|error| error.to_string())?
        } else {
            data.encoder().domain().encoded_hash()
        };
        let message_hash = encode_sdk_value(
            data.encoder(),
            &types,
            data.primary_type(),
            data.message(),
            0,
        )?;
        let prefix = starknet_core::utils::cairo_short_string_to_felt("StarkNet Message")
            .map_err(|error| error.to_string())?;
        let words = [prefix, domain_hash, *address, message_hash];
        Ok(match data.encoder().revision() {
            starknet_core::types::typed_data::Revision::V0 => {
                starknet_core::crypto::compute_hash_on_elements(&words)
            }
            starknet_core::types::typed_data::Revision::V1 => {
                starknet_crypto::poseidon_hash_many(&words)
            }
        })
    }
}

fn scalar(value: &Felt, name: &str) -> Result<(), String> {
    // Fixed curve order from Starknet, not the larger field modulus.
    let order = Felt::from_dec_str(
        "3618502788666131213697322783095070105526743751716087489154079457884512865583",
    )
    .expect("constant Stark curve order is a felt");
    if *value == Felt::ZERO || *value >= order {
        return Err(format!(
            "Stark {name} must be a nonzero scalar below curve order"
        ));
    }
    Ok(())
}

/// Derive the x-coordinate used as a Stark public key.
#[cfg(test)]
fn public_key(private_key: &Felt) -> Result<Felt, String> {
    scalar(private_key, "private key")?;
    Ok(starknet_crypto::get_public_key(private_key))
}

/// Deterministic RFC6979 signing compatible with the Starknet JS SDK.
pub fn sign(message_hash: &Felt, private_key: &Felt) -> Result<(Felt, Felt), String> {
    scalar(private_key, "private key")?;
    let mut seed = None;
    loop {
        let nonce = starknet_crypto::rfc6979_generate_k(message_hash, private_key, seed.as_ref());
        match starknet_crypto::sign(private_key, message_hash, &nonce) {
            Ok(signature) => return Ok((signature.r, signature.s)),
            Err(starknet_crypto::SignError::InvalidK) => {
                seed = Some(seed.unwrap_or(Felt::ZERO) + Felt::ONE)
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

/// Sign with an explicitly supplied ephemeral scalar. Never reuse a nonce
/// with the same private key: doing so exposes the private key.
#[cfg(test)]
fn sign_with_nonce(
    message_hash: &Felt,
    private_key: &Felt,
    nonce: &Felt,
) -> Result<(Felt, Felt), String> {
    scalar(private_key, "private key")?;
    scalar(nonce, "nonce")?;
    let signature = starknet_crypto::sign(private_key, message_hash, nonce)
        .map_err(|error| error.to_string())?;
    Ok((signature.r, signature.s))
}

/// Verify a Stark signature against a public-key x-coordinate.
#[cfg(test)]
fn verify(
    message_hash: &Felt,
    public_key: &Felt,
    signature: &(Felt, Felt),
) -> Result<bool, String> {
    starknet_crypto::verify(public_key, message_hash, &signature.0, &signature.1)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    #[derive(Deserialize)]
    struct Corpus {
        cases: Vec<Vector>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Vector {
        key: String,
        public_key: String,
        message_hash: String,
        nonce: String,
        r: String,
        s: String,
        changed_key: String,
        changed_payload: String,
        changed_nonce: String,
        changed_r: String,
        changed_s: String,
    }
    fn felt(hex: &str) -> Felt {
        Felt::from_hex(hex).unwrap()
    }
    #[test]
    fn sdk_raw_string_vectors_and_mutations() {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../tests/fixtures/starknet/sdk.json")).unwrap();
        assert!(
            corpus.cases.iter().any(|v| v.message_hash.len() == 65),
            "63-hex-digit RFC6979 edge coverage"
        );
        assert!(
            corpus.cases.iter().any(|v| v.message_hash == "0x1"),
            "small hash coverage"
        );
        for v in corpus.cases {
            let key = felt(&v.key);
            let hash = felt(&v.message_hash);
            let pk = felt(&v.public_key);
            let sig = (felt(&v.r), felt(&v.s));
            assert_eq!(public_key(&key).unwrap(), pk, "SDK Stark public key");
            assert_eq!(
                sign(&hash, &key).unwrap(),
                sig,
                "SDK Stark deterministic signature"
            );
            assert_eq!(
                sign_with_nonce(&hash, &key, &felt(&v.nonce)).unwrap(),
                sig,
                "SDK recovered nonce signature"
            );
            assert!(verify(&hash, &pk, &sig).unwrap());
            assert!(
                !verify(&hash, &public_key(&felt(&v.changed_key)).unwrap(), &sig).unwrap(),
                "mutated key must reject"
            );
            assert!(
                !verify(&felt(&v.changed_payload), &pk, &sig).unwrap(),
                "mutated payload must reject"
            );
            let changed = sign_with_nonce(&hash, &key, &felt(&v.changed_nonce)).unwrap();
            assert_ne!(changed, sig, "mutated nonce must change signature");
            assert_eq!(
                changed,
                (felt(&v.changed_r), felt(&v.changed_s)),
                "SDK extra-entropy signature"
            );
            assert!(
                verify(&hash, &pk, &changed).unwrap(),
                "changed nonce remains valid"
            );
        }
    }
    #[test]
    fn raw_prehashed_envelope_keeps_hash_and_signature_last() {
        // This primitive authenticates zHash only. alphaContext is transport
        // context, not authenticated JSON; key order is a wire-preservation gate.
        let raw = r#"{"zHash":"0x1234","alphaContext":"fixture"}"#;
        let mut envelope: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(
            serde_json::to_string(&envelope).unwrap(),
            raw,
            "prehashed envelope key order"
        );
        let hash_bytes = envelope["zHash"].as_str().unwrap().as_bytes().to_vec();
        let hash = felt(std::str::from_utf8(&hash_bytes).unwrap());
        let corpus: Corpus =
            serde_json::from_str(include_str!("../tests/fixtures/starknet/sdk.json")).unwrap();
        let vector = corpus
            .cases
            .iter()
            .find(|v| v.message_hash == "0x1234")
            .expect("SDK prehashed wire vector");
        let signature = sign(&hash, &felt(&vector.key)).unwrap();
        assert_eq!(
            signature,
            (felt(&vector.r), felt(&vector.s)),
            "SDK prehashed wire signature"
        );
        let signature_value = serde_json::Value::String(format!("{}:{}", vector.r, vector.s));
        envelope
            .as_object_mut()
            .unwrap()
            .insert("signature".into(), signature_value.clone());
        let wire = serde_json::to_string(&envelope).unwrap();
        let expected = format!(
            "{},\"signature\":{}}}",
            raw.strip_suffix('}').unwrap(),
            serde_json::to_string(&signature_value).unwrap()
        );
        assert_eq!(wire, expected, "prehashed signature-last wire order");
        let mut received: serde_json::Value = serde_json::from_str(&wire).unwrap();
        assert_eq!(
            received["zHash"].as_str().unwrap().as_bytes(),
            hash_bytes,
            "wire carries exact signed hash bytes"
        );
        assert!(
            verify(
                &felt(received["zHash"].as_str().unwrap()),
                &felt(&vector.public_key),
                &signature
            )
            .unwrap()
        );
        received.as_object_mut().unwrap().remove("signature");
        assert_eq!(
            serde_json::to_string(&received).unwrap(),
            raw,
            "transport context survives unchanged"
        );
    }
    #[derive(Deserialize)]
    struct TypedCorpus {
        cases: Vec<TypedVector>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TypedVector {
        revision: u8,
        raw: String,
        address: String,
        key: String,
        hash: String,
        public_key: String,
        r: String,
        s: String,
        changed_payload: String,
        changed_nonce: String,
        changed_payload_hash: String,
        changed_nonce_hash: String,
        changed_address_hash: String,
    }
    #[test]
    fn sdk_typed_data_raw_vectors_and_mutations() {
        let corpus: TypedCorpus =
            serde_json::from_str(include_str!("../tests/fixtures/starknet/typed-data.json"))
                .unwrap();
        let revisions: std::collections::BTreeSet<_> = corpus
            .cases
            .iter()
            .map(|v| {
                let data: starknet_core::types::TypedData = serde_json::from_str(&v.raw).unwrap();
                let actual = match data.revision() {
                    starknet_core::types::typed_data::Revision::V0 => 0,
                    starknet_core::types::typed_data::Revision::V1 => 1,
                };
                assert_eq!(
                    actual, v.revision,
                    "fixture revision matches raw typed data"
                );
                actual
            })
            .collect();
        assert_eq!(
            revisions,
            std::collections::BTreeSet::from([0, 1]),
            "SNIP-12 revision coverage"
        );
        for v in corpus.cases {
            let address = felt(&v.address);
            let key = felt(&v.key);
            let hash = hash_typed_data(&v.raw, &address).unwrap();
            let signature = (felt(&v.r), felt(&v.s));
            assert_eq!(hash, felt(&v.hash), "SDK typed-data hash");
            assert_eq!(
                sign(&hash, &key).unwrap(),
                signature,
                "SDK typed-data signature"
            );
            assert!(verify(&hash, &felt(&v.public_key), &signature).unwrap());
            let changed_payload = hash_typed_data(&v.changed_payload, &address).unwrap();
            let changed_nonce = hash_typed_data(&v.changed_nonce, &address).unwrap();
            assert_eq!(
                changed_payload,
                felt(&v.changed_payload_hash),
                "SDK changed payload hash"
            );
            assert_eq!(
                changed_nonce,
                felt(&v.changed_nonce_hash),
                "SDK changed nonce hash"
            );
            assert_ne!(changed_payload, hash, "typed payload participates in hash");
            assert_ne!(
                changed_nonce, hash,
                "typed application nonce participates in hash"
            );
            assert!(
                !verify(&changed_payload, &felt(&v.public_key), &signature).unwrap(),
                "typed payload mutation must reject"
            );
            assert!(
                !verify(&changed_nonce, &felt(&v.public_key), &signature).unwrap(),
                "typed nonce mutation must reject"
            );
            assert!(
                !verify(&hash, &public_key(&Felt::TWO).unwrap(), &signature).unwrap(),
                "typed key mutation must reject"
            );
            assert_eq!(
                hash_typed_data(&v.raw, &felt("0x1235")).unwrap(),
                felt(&v.changed_address_hash),
                "SDK typed address binding"
            );
        }
    }
    #[test]
    fn sdk_field_boundaries_refuse_colliding_declared_messages() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/starknet/typed-boundaries.json"
        ))
        .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        for revision in [0, 1] {
            for ty in ["felt", "shortstring", "ContractAddress", "ClassHash"] {
                for edge in [
                    "p",
                    "p-plus-one",
                    "hex-p",
                    "negative",
                    "nested-p",
                    "array-p",
                    "zero",
                    "one",
                    "p-minus-one",
                ] {
                    let name = format!("rev{revision}/{ty}/{edge}");
                    assert!(
                        cases.iter().any(|case| case["name"] == name),
                        "required SDK range property {name}"
                    );
                }
            }
        }
        for name in [
            "rev1/i128/-5",
            "rev1/i128/\"-5\"",
            "rev1/i128/0",
            "rev1/i128/\"0\"",
            "rev1/i128/5",
            "rev1/i128/\"5\"",
            "rev1/extra-undeclared",
        ] {
            assert!(
                cases.iter().any(|case| case["name"] == name),
                "required SDK coercion property {name}"
            );
        }
        let mut mismatches = Vec::new();
        for case in cases {
            let actual = hash_typed_data(
                case["raw"].as_str().unwrap(),
                &felt(case["address"].as_str().unwrap()),
            );
            if let Some(expected) = case["hash"].as_str() {
                if actual
                    .as_ref()
                    .map(|value| format!("{value:#x}"))
                    .as_deref()
                    != Ok(expected)
                {
                    mismatches.push(format!(
                        "{} SDK hash {expected}, Rust {actual:?}",
                        case["name"]
                    ));
                }
            } else if actual.is_ok() {
                mismatches.push(format!(
                    "{} SDK refuses, Rust signs {actual:?}",
                    case["name"]
                ));
            }
        }
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }
    #[test]
    fn sdk_negative_zero_and_composite_range_edges() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/starknet/typed-compositions.json"
        ))
        .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        for revision in [0, 1] {
            for kind in ["felt", "shortstring", "ContractAddress", "ClassHash"] {
                for zero in ["-0", "-00"] {
                    let name = format!("rev{revision}/{kind}/negative-zero-{zero}");
                    assert!(
                        cases.iter().any(|case| case["name"] == name),
                        "required negative-zero {name}"
                    );
                }
            }
            for kind in ["domain", "merkletree"] {
                for edge in ["p", "negative-zero"] {
                    let name = format!("rev{revision}/{kind}/{edge}");
                    assert!(
                        cases.iter().any(|case| case["name"] == name),
                        "required composite {name}"
                    );
                }
            }
        }
        for edge in ["p", "negative-zero"] {
            assert!(
                cases
                    .iter()
                    .any(|case| case["name"] == format!("rev1/enum/{edge}")),
                "required enum boundary {edge}"
            );
        }
        for case in cases {
            let actual = hash_typed_data(
                case["raw"].as_str().unwrap(),
                &felt(case["address"].as_str().unwrap()),
            );
            if let Some(hash) = case["hash"].as_str() {
                assert_eq!(
                    actual.map(|value| format!("{value:#x}")),
                    Ok(hash.to_owned()),
                    "{}",
                    case["name"]
                );
            } else {
                assert!(actual.is_err(), "composite range refusal {}", case["name"]);
            }
        }
    }
    #[test]
    fn malformed_schema_shapes_cannot_bypass_range_checks() {
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/starknet/typed-shapes.json"))
                .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        for name in [
            "root-array",
            "domain-array",
            "field-array",
            "type-map-array",
        ] {
            assert!(
                cases.iter().any(|case| case["name"] == name),
                "required schema shape {name}"
            );
        }
        for case in cases {
            assert!(case["error"].is_object());
            assert!(
                hash_typed_data(
                    case["raw"].as_str().unwrap(),
                    &felt(case["address"].as_str().unwrap())
                )
                .is_err(),
                "schema bypass {}",
                case["name"]
            );
        }
    }
    #[test]
    fn malformed_raw_typed_data_returns_error() {
        for raw in [
            "{",
            "{}",
            "null",
            r#"{"types":{},"primaryType":"Message","domain":{},"message":{}}"#,
        ] {
            assert!(
                hash_typed_data(raw, &Felt::ONE).is_err(),
                "malformed raw typed data must refuse: {raw}"
            );
        }
    }
    #[test]
    fn malformed_raw_scalar_returns_error() {
        let text: String = serde_json::from_str("\"0x0\"").unwrap();
        assert_eq!(
            public_key(&felt(&text)).unwrap_err(),
            "Stark private key must be a nonzero scalar below curve order"
        );
        assert_eq!(
            sign_with_nonce(&Felt::ONE, &Felt::ONE, &Felt::ZERO).unwrap_err(),
            "Stark nonce must be a nonzero scalar below curve order"
        );
    }
}

#[cfg(test)]
mod closed_kind_tests {
    use super::*;
    #[test]
    fn unrecognized_declared_members_refuse_structurally() {
        for raw in ["1", "{}", "[]", "true", r#""unknown""#] {
            let types: serde_json::Value =
                serde_json::from_str(r#"{"Message":[{"name":"x","type":"FuturePreset"}]}"#)
                    .unwrap();
            let mut message: serde_json::Value =
                serde_json::from_str(&format!(r#"{{"x":{raw}}}"#)).unwrap();
            let error = check_declared_fields(&types, "Message", &mut message, None, 0)
                .expect_err("unrecognized declared member must refuse before encoding");
            assert!(
                error.contains("Unsupported Stark type"),
                "structural unknown-kind diagnostic: {error}"
            );
        }
    }
    #[test]
    fn sdk_closed_kind_refusals_and_preset_hashes() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/starknet/typed-closed-kinds.json"
        ))
        .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        for name in [
            "unknown-number",
            "unknown-record",
            "shadow-felt",
            "shadow-preset",
            "rev0/selector/hex-p",
            "rev0/string/negative",
            "rev0/i128/p",
        ] {
            assert!(
                cases.iter().any(|case| case["name"] == name),
                "missing closed-kind refusal property {name}"
            );
        }
        for kind in ["TokenAmount", "NftId"] {
            for shape in [kind.to_owned(), format!("{kind}-array")] {
                for edge in ["zero", "one", "p", "p-plus-one", "hex-p"] {
                    let name = format!("rev1/{shape}/{edge}");
                    assert!(
                        cases.iter().any(|case| case["name"] == name),
                        "missing preset boundary {name}"
                    );
                }
            }
        }
        for case in cases {
            let hash = hash_typed_data(
                case["raw"].as_str().unwrap(),
                &Felt::from_hex("0x1234").unwrap(),
            );
            if case["name"].as_str().unwrap().starts_with("shadow-") {
                let error = hash
                    .as_ref()
                    .expect_err("reserved shadow must refuse before encoding");
                assert!(
                    error.starts_with("Reserved Stark type cannot be redefined"),
                    "reserved shadow refusal owner {}: {error}",
                    case["name"]
                );
            }
            if case.get("error").is_some() {
                assert!(
                    hash.is_err(),
                    "SDK refuses but Rust hashes {}",
                    case["name"]
                );
            } else if case["name"]
                .as_str()
                .unwrap()
                .starts_with("rev1/TokenAmount")
                || case["name"].as_str().unwrap().starts_with("rev1/NftId")
            {
                assert_eq!(
                    hash.unwrap(),
                    Felt::from_hex(case["hash"].as_str().unwrap()).unwrap(),
                    "SDK preset hash {}",
                    case["name"]
                );
            }
        }
    }
    #[test]
    fn sdk_named_encoding_exemptions_have_positive_hash_controls() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/starknet/typed-exempt-controls.json"
        ))
        .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        for name in [
            "bool-true",
            "bool-false",
            "long-string",
            "string-negative-text",
            "signed-i128",
            "minimum-i128",
            "selector-decimal-name",
            "selector-negative-name",
        ] {
            assert!(
                cases.iter().any(|case| case["name"] == name),
                "missing named exemption control {name}"
            );
        }
        for case in cases {
            assert_eq!(
                hash_typed_data(
                    case["raw"].as_str().unwrap(),
                    &Felt::from_hex("0x1234").unwrap()
                )
                .unwrap(),
                Felt::from_hex(case["hash"].as_str().unwrap()).unwrap(),
                "SDK exempt hash {}",
                case["name"]
            );
        }
    }
}

#[cfg(test)]
mod composite_shape_tests {
    use super::*;
    #[test]
    fn raw_composite_shapes_refuse_before_scalar_conversion() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/starknet/typed-composite-shapes.json"
        ))
        .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        assert!(
            cases
                .iter()
                .any(|case| case["raw"].as_str().unwrap().contains(":361850")),
            "raw oversized composite number required"
        );
        assert!(
            cases.iter().any(|case| case["name"] == "custom-record/[]"),
            "positional custom record required"
        );
        for case in cases {
            assert!(case.get("error").is_some(), "SDK shape refusal required");
            assert!(
                hash_typed_data(
                    case["raw"].as_str().unwrap(),
                    &Felt::from_hex("0x1234").unwrap()
                )
                .is_err(),
                "SDK refuses composite shape but Rust hashes {}",
                case["name"]
            );
        }
    }
}
