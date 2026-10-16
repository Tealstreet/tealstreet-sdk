use crate::{
    public_streams::StreamOps,
    spot_private_class::{call, fallback, optional},
};
use std::rc::Rc;

fn empty_account<V, E>(h: &StreamOps<V, E>) -> Result<V, E> {
    Ok((h.object)(vec![
        ("userId", (h.literal)("")),
        ("subId", (h.intrinsic)("null", vec![])?),
        ("affiliateId", (h.intrinsic)("null", vec![])?),
    ]))
}
fn caught_message<V, E>(h: &StreamOps<V, E>, error: &E) -> Result<V, E> {
    let error = (h.error_value)(error);
    let response = optional(h, &error, "response")?;
    let data = optional(h, &response, "data")?;
    fallback(h, optional(h, &data, "msg")?, || {
        optional(h, &error, "message")
    })
}
fn emit<V, E>(h: &StreamOps<V, E>, message: impl FnOnce() -> Result<V, E>) -> Result<(), E> {
    call(h, "this.emitAndLogError", || Ok(vec![message()?]))?;
    Ok(())
}
async fn request<V, E>(h: &StreamOps<V, E>, validation: bool) -> Result<V, E> {
    let response = call(h, "this.xhr.get", || {
        Ok(vec![(h.literal)(if validation {
            "/api/v1/account-overview"
        } else {
            "/api/v1/user-info"
        })])
    })?;
    let response = (h.await_value)(response).await?;
    (h.intrinsic)("destructure-data", vec![response])
}
fn bad_code<V, E>(h: &StreamOps<V, E>, data: &V) -> Result<bool, E> {
    Ok(!(h.equal)(&(h.get)(data, "code")?, &(h.literal)("200000")))
}
pub async fn get_account<V: Clone + 'static, E: 'static>(h: Rc<StreamOps<V, E>>) -> Result<V, E> {
    let result = async {
        let data = request(&h, false).await?;
        if bad_code(&h, &data)? {
            emit(&h, || {
                fallback(&h, (h.get)(&data, "msg")?, || {
                    Ok((h.literal)("Failed to get account info"))
                })
            })?;
            return empty_account(&h);
        }
        let details = (h.get)(&data, "data")?;
        let user_id = fallback(&h, (h.get)(&details, "userId")?, || Ok((h.literal)("")))?;
        let details = (h.get)(&data, "data")?;
        let sub_id = fallback(&h, (h.get)(&details, "subUserId")?, || {
            (h.intrinsic)("null", vec![])
        })?;
        let details = (h.get)(&data, "data")?;
        let affiliate_id = fallback(&h, (h.get)(&details, "referrerUserId")?, || {
            (h.intrinsic)("null", vec![])
        })?;
        Ok((h.object)(vec![
            ("userId", user_id),
            ("subId", sub_id),
            ("affiliateId", affiliate_id),
        ]))
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            emit(&h, || caught_message(&h, &error))?;
            empty_account(&h)
        }
    }
}
pub async fn validate_account<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
) -> Result<V, E> {
    let result = async {
        let data = request(&h, true).await?;
        if bad_code(&h, &data)? {
            let error = fallback(&h, (h.get)(&data, "msg")?, || {
                Ok((h.literal)("Failed to validate account"))
            })?;
            emit(&h, || Ok(error.clone()))?;
            return Ok((h.object)(vec![("error", error)]));
        }
        let balance = call(&h, "this.fetchBalance", || Ok(vec![]))?;
        let catch = (h.get)(&balance, "catch")?;
        let handler = (h.callback)("", 0, Rc::new(|_| Ok(())));
        let caught = (h.intrinsic)(
            "call-with-this",
            vec![catch, balance, (h.array)(vec![handler])],
        )?;
        let balance = (h.await_value)(caught).await?;
        Ok((h.object)(vec![
            ("error", (h.literal)("")),
            ("balance", balance),
        ]))
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let message = fallback(&h, caught_message(&h, &error)?, || {
                Ok((h.literal)("Unknown error validating account"))
            })?;
            emit(&h, || Ok(message.clone()))?;
            Ok((h.object)(vec![("error", message)]))
        }
    }
}
