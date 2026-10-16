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
        let endpoints = (h.capture)("KUCOIN_SPOT_ENDPOINTS")?;
        let mut args = vec![(h.get)(
            &endpoints,
            if validation {
                "ACCOUNTS"
            } else {
                "ACCOUNT_SUMMARY"
            },
        )?];
        if validation {
            args.push((h.object)(vec![(
                "params",
                (h.object)(vec![("type", (h.literal)("trade"))]),
            )]));
        }
        Ok(args)
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
                    Ok((h.literal)("Failed to fetch KuCoin Spot account summary"))
                })
            })?;
            return empty_account(&h);
        }
        let user_id = fallback(
            &h,
            optional(&h, &(h.get)(&data, "data")?, "userId")?,
            || {
                fallback(&h, optional(&h, &(h.get)(&data, "data")?, "uid")?, || {
                    Ok((h.literal)(""))
                })
            },
        )?;
        let sub_id = fallback(
            &h,
            optional(&h, &(h.get)(&data, "data")?, "subUserId")?,
            || (h.intrinsic)("null", vec![]),
        )?;
        let affiliate_id = fallback(
            &h,
            optional(&h, &(h.get)(&data, "data")?, "referrerUserId")?,
            || (h.intrinsic)("null", vec![]),
        )?;
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
                Ok((h.literal)("Failed to validate KuCoin Spot account"))
            })?;
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
                Ok((h.literal)("Failed to validate KuCoin Spot account"))
            })?;
            emit(&h, || Ok(message.clone()))?;
            Ok((h.object)(vec![("error", message)]))
        }
    }
}
