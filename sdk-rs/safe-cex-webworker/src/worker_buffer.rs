use serde_json::{Value, json};
use std::cell::RefCell;

// ts: ExchangeWorker._flushBufferedMessages
pub fn flush_buffered_messages<E, P>(
    buffer: &RefCell<Vec<P>>,
    mut log_info: impl FnMut(&str, &Value) -> Result<(), E>,
    mut handle_payload: impl FnMut(&mut P) -> Result<(), E>,
    mut stringify_error: impl FnMut(&E) -> Result<String, E>,
    mut log_error: impl FnMut(&str, E, &P, usize, usize) -> Result<(), E>,
) -> Result<(), E> {
    let buffer_count = buffer.borrow().len();
    if buffer_count == 0 {
        return Ok(());
    }
    let count = safe_cex_num::js_number_string(buffer_count as f64);
    if buffer_count > 10 {
        log_info(
            &format!("Processed {count} buffered messages during initialization"),
            &json!({"bufferCount":buffer_count}),
        )?;
    }
    // Logging can re-enter and change the buffer before the source snapshots it.
    // Release the borrow before invoking any handler or error callback.
    let buffered_messages = std::mem::take(&mut *buffer.borrow_mut());
    for (index, mut payload) in buffered_messages.into_iter().enumerate() {
        if let Err(error) = handle_payload(&mut payload) {
            let position = safe_cex_num::js_number_string((index + 1) as f64);
            let message = format!(
                "Failed to process buffered message {position}/{count} {}",
                stringify_error(&error)?
            );
            log_error(&message, error, &payload, index, buffer_count)?;
        }
    }
    Ok(())
}
