use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_venue_weex::utils::WeexUtilityInput;

fn unpaired(input: &Input, units: &mut Vec<u16>) {
    match input {
        Input::Text(text) => units.extend(
            char::decode_utf16(text.iter().copied())
                .filter_map(|unit| unit.err().map(|error| error.unpaired_surrogate())),
        ),
        Input::Array(values) => {
            for value in values {
                unpaired(value, units);
            }
        }
        Input::Object(fields) => {
            for (_, value) in fields {
                unpaired(value, units);
            }
        }
        _ => {}
    }
}

pub fn admit_utf16(raw: &str) -> WeexUtilityInput {
    let inputs = [
        serde_json::from_str::<WeexUtilityInput>(raw).unwrap(),
        safe_cex_num::numeric_ingress::js_from_str::<WeexUtilityInput>(raw).unwrap(),
    ];
    for input in &inputs {
        let mut units = Vec::new();
        unpaired(input.as_value(), &mut units);
        assert_eq!(
            units,
            [0xd800],
            "retained input keeps original unpaired unit"
        );
    }
    inputs.into_iter().next().unwrap()
}
