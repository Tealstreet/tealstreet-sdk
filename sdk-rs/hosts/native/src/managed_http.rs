//! Prepared resources stay in the same actor that claims and hands off HTTP.
#[cfg(feature = "http-mutation-barriers")]
#[path = "managed_http_group.rs"]
mod group;
use super::*;
use safe_cex_engine::VenueError;
use safe_cex_engine::request_context::{Claim, Error, HttpAction, Permit, Preparation, Prepared};

/// Dropping any generation cancels only its own asynchronous work.
pub(super) struct Task(pub(super) AbortHandle);
impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

enum Resource {
    Preparing {
        token: Preparation,
        _task: Task,
    },
    Prepared {
        token: Preparation,
        id: u64,
        packet: Box<dyn PreparedHttp>,
    },
    Bound {
        token: Preparation,
        permit: Permit,
        packet: Box<dyn BoundHttp>,
        _wake: Option<Task>,
    },
    InFlight {
        token: Preparation,
        _task: Task,
    },
}
impl Resource {
    fn token(&self) -> Preparation {
        match self {
            Self::Preparing { token, .. }
            | Self::Prepared { token, .. }
            | Self::Bound { token, .. }
            | Self::InFlight { token, .. } => *token,
        }
    }
}

#[derive(Default)]
pub(super) struct Requests {
    resources: HashMap<RequestId, Resource>,
    next: u64,
}
impl Requests {
    pub(super) fn clear(&mut self) {
        self.resources.clear();
    }
}

impl<V: Venue, T: Transport> Host<V, T> {
    pub(super) fn execute_managed_http(&mut self, action: HttpAction) {
        match action {
            HttpAction::Prepare {
                preparation,
                intent,
                request,
            } => {
                if !self.engine.http_preparation_pending(preparation) {
                    return;
                }
                if self
                    .managed_http
                    .resources
                    .get(&request.id)
                    .is_some_and(|resource| resource.token() == preparation)
                {
                    return;
                }
                let request_id = request.id;
                let future = self.transport.prepare_http(request, intent);
                let events = self.events.clone();
                let task = tokio::spawn(async move {
                    let result = tokio::select! {
                        biased;
                        _ = events.closed() => return,
                        result = future => result,
                    };
                    let _ = events.send(HostEvent::HttpPrepared(preparation, result));
                });
                self.managed_http.resources.insert(
                    request_id,
                    Resource::Preparing {
                        token: preparation,
                        _task: Task(task.abort_handle()),
                    },
                );
            }
            HttpAction::Bind {
                preparation,
                permit,
                request,
            } => {
                let request_id = request.id;
                if !self
                    .managed_http
                    .resources
                    .get(&request_id)
                    .is_some_and(|resource| {
                        matches!(resource,
                        Resource::Prepared {token, id, ..}
                        if *token == preparation && *id == permit.prepared_id())
                    })
                {
                    return;
                }
                let Some(Resource::Prepared { packet, .. }) =
                    self.managed_http.resources.remove(&request_id)
                else {
                    return;
                };
                match packet.bind(request) {
                    Ok(packet) => {
                        self.managed_http.resources.insert(
                            request_id,
                            Resource::Bound {
                                token: preparation,
                                permit,
                                packet,
                                _wake: None,
                            },
                        );
                        self.claim_http_resource(request_id);
                    }
                    Err(error) => self.fail_http_preparation(preparation, error),
                }
            }
            #[cfg(feature = "http-mutation-barriers")]
            HttpAction::BindGroup { permit, members } => self.bind_http_group(permit, members),
            #[cfg(not(feature = "http-mutation-barriers"))]
            HttpAction::BindGroup { members, .. } => {
                if let Some(member) = members.first() {
                    self.fail_http_preparation(
                        member.preparation,
                        HttpPrepareError::Invalid("HTTP Mutation barriers unsupported by host"),
                    );
                }
            }
            HttpAction::Cancel {
                preparation,
                signing,
            } => {
                if let Some(attempt) = signing {
                    self.signing_tasks.remove(&attempt);
                }
                let request = preparation.attempt().request;
                if self
                    .managed_http
                    .resources
                    .get(&request)
                    .is_some_and(|resource| resource.token() == preparation)
                {
                    self.managed_http.resources.remove(&request);
                }
            }
        }
    }

    pub(super) fn complete_http_preparation(
        &mut self,
        preparation: Preparation,
        result: Result<Box<dyn PreparedHttp>, HttpPrepareError>,
    ) {
        let request = preparation.attempt().request;
        if !self.engine.http_preparation_pending(preparation)
            || !self
                .managed_http
                .resources
                .get(&request)
                .is_some_and(|resource| {
                    matches!(resource,
                    Resource::Preparing {token, ..} if *token == preparation)
                })
        {
            return;
        }
        self.managed_http.resources.remove(&request);
        match result {
            Ok(packet) => {
                let Some(id) = self.managed_http.next.checked_add(1) else {
                    self.engine.http_preparation_failed(
                        (self.clock)(),
                        preparation,
                        Error::Capacity,
                        VenueError::new("HTTP preparation identities exhausted"),
                    );
                    return;
                };
                self.managed_http.next = id;
                self.managed_http.resources.insert(
                    request,
                    Resource::Prepared {
                        token: preparation,
                        id,
                        packet,
                    },
                );
                self.engine.http_prepared(
                    (self.clock)(),
                    preparation,
                    Prepared {
                        id,
                        expires_at: None,
                    },
                );
            }
            Err(error) => self.fail_http_preparation(preparation, error),
        }
    }

    fn fail_http_preparation(&mut self, preparation: Preparation, error: HttpPrepareError) {
        let (reason, kind) = match error {
            HttpPrepareError::Expired => (Error::TimedOut, safe_cex_engine::ErrorKind::Timeout),
            HttpPrepareError::Unsupported => {
                (Error::Invalid, safe_cex_engine::ErrorKind::Unsupported)
            }
            HttpPrepareError::Invalid(_) => {
                (Error::Invalid, safe_cex_engine::ErrorKind::InvalidInput)
            }
            HttpPrepareError::Transport(_) => (Error::State, safe_cex_engine::ErrorKind::Network),
        };
        self.engine.http_preparation_failed(
            (self.clock)(),
            preparation,
            reason,
            VenueError::new(error.message()).kind(kind),
        );
    }

    pub(super) fn wake_http(&mut self, preparation: Preparation, permit: Permit) {
        let request = preparation.attempt().request;
        if self.managed_http.resources.get(&request).is_some_and(|resource| matches!(resource,
            Resource::Bound {token, permit: current, ..} if *token == preparation && *current == permit))
        {
            self.claim_http_resource(request);
        }
    }

    fn claim_http_resource(&mut self, request: RequestId) {
        let Some(resource) = self.managed_http.resources.remove(&request) else {
            return;
        };
        let Resource::Bound {
            token,
            permit,
            packet,
            ..
        } = resource
        else {
            self.managed_http.resources.insert(request, resource);
            return;
        };
        // Every fallible resource check precedes the shared final admission.
        if !packet.ready() {
            self.fail_http_preparation(token, HttpPrepareError::Expired);
            return;
        }
        let now = (self.clock)();
        match self.engine.claim_http(now, permit) {
            Ok(Claim::Released(_)) => {
                // No await, proxy/token work or actor hop may separate this
                // handoff from the engine's successful claim above.
                let task = packet.dispatch(self.events.clone());
                self.managed_http.resources.insert(
                    request,
                    Resource::InFlight {
                        token,
                        _task: Task(task),
                    },
                );
            }
            Ok(Claim::WaitUntil(at)) => {
                let events = self.events.clone();
                let delay = Duration::from_secs_f64((at - now).max(0.) / 1_000.);
                let deadline = tokio::time::Instant::now() + delay;
                let wake = tokio::spawn(async move {
                    tokio::time::sleep_until(deadline).await;
                    let _ = events.send(HostEvent::HttpWake(token, permit));
                });
                self.managed_http.resources.insert(
                    request,
                    Resource::Bound {
                        token,
                        permit,
                        packet,
                        _wake: Some(Task(wake.abort_handle())),
                    },
                );
            }
            Err(_) => {} // The engine owns settlement/reprepare and callbacks.
        }
    }
}

#[cfg(test)]
#[path = "managed_http_tests.rs"]
mod tests;
