/// Partial websocket state; topic vectors project insertion-ordered, unique JavaScript Sets.
#[derive(Debug, Clone)]
pub struct WeexSubscriptionState {
    pub topics: Vec<Vec<u16>>,
    pub ticker_topics: Vec<Vec<u16>>,
    pub is_connected: bool,
    pub is_disposed: bool,
    pub operation_timestamps: Vec<f64>,
    pub has_logged_limit: bool,
    pub subscription_id: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeexSubscriptionMethod {
    Subscribe,
    Unsubscribe,
}

/// The host supplies clock, store/parent reads, warning logging and JSON serialization/send.
/// Callbacks receive mutable state so synchronous effects occur without a retained borrow guard.
pub struct WeexSubscriptionRuntime<H, E> {
    pub now: fn(&mut H, &mut WeexSubscriptionState) -> Result<f64, E>,
    pub desired_topics: fn(&mut H, &mut WeexSubscriptionState) -> Result<Vec<Vec<u16>>, E>,
    pub log_warning: fn(&mut H, &mut WeexSubscriptionState, &str) -> Result<(), E>,
    pub send: fn(
        &mut H,
        &mut WeexSubscriptionState,
        WeexSubscriptionMethod,
        &[Vec<u16>],
        f64,
    ) -> Result<(), E>,
}

fn add(topics: &mut Vec<Vec<u16>>, topic: Vec<u16>) {
    if !topics.contains(&topic) {
        topics.push(topic);
    }
}

fn ordered_set(topics: Vec<Vec<u16>>) -> Vec<Vec<u16>> {
    let mut unique = Vec::new();
    for topic in topics {
        add(&mut unique, topic);
    }
    unique
}

pub fn send_subscription_operation<H, E>(
    host: &mut H,
    state: &mut WeexSubscriptionState,
    method: WeexSubscriptionMethod,
    topics: &[Vec<u16>],
    runtime: &WeexSubscriptionRuntime<H, E>,
) -> Result<bool, E> {
    let now = (runtime.now)(host, state)?;
    state.operation_timestamps = state
        .operation_timestamps
        .iter()
        .copied()
        .filter(|timestamp| now - timestamp < 3_600_000.0)
        .collect();
    if state.operation_timestamps.len() >= 240 {
        if !state.has_logged_limit {
            state.has_logged_limit = true;
            (runtime.log_warning)(
                host,
                state,
                "WEEX public WS subscription operation limit reached; deferring changes until another symbol update or reconnect",
            )?;
        }
        return Ok(false);
    }
    state.operation_timestamps.push(now);
    let id = state.subscription_id;
    state.subscription_id += 1.0;
    (runtime.send)(host, state, method, topics, id)?;
    Ok(true)
}

pub fn subscribe_topics<H, E>(
    host: &mut H,
    state: &mut WeexSubscriptionState,
    topics: &[Vec<u16>],
    track: bool,
    runtime: &WeexSubscriptionRuntime<H, E>,
) -> Result<(), E> {
    if track {
        for topic in topics {
            add(&mut state.topics, topic.clone());
        }
    }
    if !state.is_connected || topics.is_empty() {
        return Ok(());
    }
    for batch in topics.chunks(50) {
        send_subscription_operation(
            host,
            state,
            WeexSubscriptionMethod::Subscribe,
            batch,
            runtime,
        )?;
    }
    Ok(())
}

pub fn reconcile_ticker_topics<H, E>(
    host: &mut H,
    state: &mut WeexSubscriptionState,
    runtime: &WeexSubscriptionRuntime<H, E>,
) -> Result<(), E> {
    let next_topics = ordered_set((runtime.desired_topics)(host, state)?);
    let to_unsubscribe: Vec<Vec<u16>> = state
        .ticker_topics
        .iter()
        .filter(|topic| !next_topics.contains(topic))
        .cloned()
        .collect();
    let to_subscribe: Vec<Vec<u16>> = next_topics
        .iter()
        .filter(|topic| !state.ticker_topics.contains(topic))
        .cloned()
        .collect();
    if !state.is_connected {
        state.ticker_topics = next_topics;
        return Ok(());
    }
    let mut reconciled_topics = state.ticker_topics.clone();
    if !to_unsubscribe.is_empty()
        && send_subscription_operation(
            host,
            state,
            WeexSubscriptionMethod::Unsubscribe,
            &to_unsubscribe,
            runtime,
        )?
    {
        reconciled_topics.retain(|topic| !to_unsubscribe.contains(topic));
    }
    if !to_subscribe.is_empty()
        && send_subscription_operation(
            host,
            state,
            WeexSubscriptionMethod::Subscribe,
            &to_subscribe,
            runtime,
        )?
    {
        for topic in to_subscribe {
            add(&mut reconciled_topics, topic);
        }
    }
    state.ticker_topics = reconciled_topics;
    Ok(())
}

pub fn subscribe_ticker_topics<H, E>(
    host: &mut H,
    state: &mut WeexSubscriptionState,
    runtime: &WeexSubscriptionRuntime<H, E>,
) -> Result<(), E> {
    state.ticker_topics = ordered_set((runtime.desired_topics)(host, state)?);
    let topics = state.ticker_topics.clone();
    subscribe_topics(host, state, &topics, false, runtime)
}

pub fn resubscribe_topics<H, E>(
    host: &mut H,
    state: &mut WeexSubscriptionState,
    runtime: &WeexSubscriptionRuntime<H, E>,
) -> Result<(), E> {
    let topics = state.topics.clone();
    if !topics.is_empty() {
        subscribe_topics(host, state, &topics, false, runtime)?;
    }
    Ok(())
}

pub fn subscribe_topic<H, E>(
    host: &mut H,
    state: &mut WeexSubscriptionState,
    topic: Vec<u16>,
    runtime: &WeexSubscriptionRuntime<H, E>,
) -> Result<(), E> {
    add(&mut state.topics, topic.clone());
    reconcile_ticker_topics(host, state, runtime)?;
    subscribe_topics(host, state, &[topic], true, runtime)
}

pub fn unsubscribe_topic<H, E>(
    host: &mut H,
    state: &mut WeexSubscriptionState,
    topic: &[u16],
    runtime: &WeexSubscriptionRuntime<H, E>,
) -> Result<(), E> {
    state.topics.retain(|item| item != topic);
    if !state.is_connected {
        return Ok(());
    }
    send_subscription_operation(
        host,
        state,
        WeexSubscriptionMethod::Unsubscribe,
        &[topic.to_vec()],
        runtime,
    )?;
    reconcile_ticker_topics(host, state, runtime)
}

pub fn handle_active_ticker_symbols_changed<H, E>(
    host: &mut H,
    state: &mut WeexSubscriptionState,
    runtime: &WeexSubscriptionRuntime<H, E>,
) -> Result<(), E> {
    if state.is_disposed {
        return Ok(());
    }
    reconcile_ticker_topics(host, state, runtime)
}
