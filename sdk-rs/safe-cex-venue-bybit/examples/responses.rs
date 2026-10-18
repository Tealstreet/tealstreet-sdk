use std::io::{self, Read};
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let cases: Vec<safe_cex_types::retained_json::RetainedJsonInput> =
        serde_json::from_str(&input).unwrap();
    let actual: Vec<_> = cases
        .iter()
        .map(
            |input| match safe_cex_venue_bybit::response::get_bybit_response_meta(input) {
                Ok(meta) => serde_json::to_value(meta).unwrap(),
                Err(error) => {
                    serde_json::json!({"refusal":{"name":error.name,"message":error.message}})
                }
            },
        )
        .collect();
    println!("{}", serde_json::to_string(&actual).unwrap());
}
