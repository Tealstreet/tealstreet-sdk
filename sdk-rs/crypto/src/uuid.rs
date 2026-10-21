//! nanoid default21 followed by removal of '-' and '_'.
const URL_ALPHABET: &[u8; 64] = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";
pub fn uuid_with_entropy(
    fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
) -> Result<String, String> {
    let mut bytes = [0; 21];
    fill(&mut bytes)?;
    Ok(bytes
        .iter()
        .map(|byte| URL_ALPHABET[usize::from(byte & 63)])
        .filter(|byte| *byte != b'-' && *byte != b'_')
        .map(char::from)
        .collect())
}
