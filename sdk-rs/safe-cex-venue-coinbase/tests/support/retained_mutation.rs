use safe_cex_types::retained_json::RetainedJsonInput as Input;

pub fn set(value: &mut Input, key: &str, next: Input) {
    let Input::Object(fields) = value else {
        panic!("retained fixture record")
    };
    let key: Vec<u16> = key.encode_utf16().collect();
    if let Some((_, value)) = fields.iter_mut().find(|(name, _)| name == &key) {
        *value = next;
    } else {
        fields.push((key, next));
    }
}
