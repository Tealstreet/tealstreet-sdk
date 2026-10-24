//! Bound group packets stay in one actor through the final synchronous claim.
use super::*;
use safe_cex_engine::request_context::{GroupClaim, GroupPacket, GroupPermit};

struct Member {
    request: RequestId,
    token: Preparation,
    permit: Permit,
    packet: Option<Box<dyn BoundHttp>>,
    task: Option<AbortHandle>,
}

impl<V: Venue, T: Transport> Host<V, T> {
    pub(super) fn bind_http_group(&mut self, permit: GroupPermit, mut packets: Vec<GroupPacket>) {
        if !self.engine.http_group_pending(&permit) {
            return;
        }
        let first = packets
            .first()
            .map(|packet| packet.preparation)
            .expect("engine group has members");
        if packets.len() != permit.members().len() {
            self.fail_http_preparation(
                first,
                HttpPrepareError::Invalid("HTTP group cardinality mismatch"),
            );
            return;
        }
        for (packet, &member) in packets.iter_mut().zip(permit.members()) {
            let token = packet.preparation;
            let request = token.attempt().request;
            let valid = match (&packet.request, self.managed_http.resources.get(&request)) {
                (
                    Some(http),
                    Some(Resource::Prepared {
                        token: current, id, ..
                    }),
                ) => {
                    http.id == request
                        && token == *current
                        && *id == member.prepared_id()
                        && token.attempt() == member.attempt()
                }
                (
                    None,
                    Some(Resource::Bound {
                        token: current,
                        permit: current_permit,
                        ..
                    }),
                ) => token == *current && member == *current_permit,
                _ => false,
            };
            if !valid {
                self.fail_http_preparation(
                    first,
                    HttpPrepareError::Invalid("HTTP group resource mismatch"),
                );
                return;
            }
        }
        for (mut packet, &member) in packets.into_iter().zip(permit.members()) {
            let token = packet.preparation;
            let request = token.attempt().request;
            let Some(http) = packet.request.take() else {
                continue;
            };
            let Some(Resource::Prepared { packet, .. }) =
                self.managed_http.resources.remove(&request)
            else {
                unreachable!("checked prepared group resource")
            };
            match packet.bind(http) {
                Ok(packet) => {
                    self.managed_http.resources.insert(
                        request,
                        Resource::Bound {
                            token,
                            permit: member,
                            packet,
                            _wake: None,
                        },
                    );
                }
                Err(error) => {
                    self.fail_http_preparation(token, error);
                    return;
                }
            }
        }
        self.claim_http_group_resources(permit);
    }

    pub(crate) fn claim_http_group_resources(&mut self, permit: GroupPermit) {
        if !self.engine.http_group_pending(&permit) {
            return;
        }
        let first = permit.members()[0].attempt().request;
        let first_token = self
            .managed_http
            .resources
            .get(&first)
            .map(Resource::token)
            .expect("validated group retains its first resource");
        let mut members = Vec::new();
        if members.try_reserve_exact(permit.members().len()).is_err()
            || self
                .managed_http
                .resources
                .try_reserve(permit.members().len())
                .is_err()
        {
            self.engine.http_preparation_failed(
                (self.clock)(),
                first_token,
                Error::Capacity,
                VenueError::new("HTTP group resources unavailable"),
            );
            return;
        }
        for token in permit.members() {
            let valid = self
                .managed_http
                .resources
                .get(&token.attempt().request)
                .is_some_and(|resource| {
                    matches!(resource,
                Resource::Bound { token: preparation, permit: current, .. }
                if *current == *token && preparation.attempt() == token.attempt())
                });
            if !valid {
                self.fail_http_preparation(
                    first_token,
                    HttpPrepareError::Invalid("HTTP group bound resource missing"),
                );
                return;
            }
        }
        for token in permit.members() {
            let request = token.attempt().request;
            let Some(Resource::Bound {
                token: preparation,
                packet,
                ..
            }) = self.managed_http.resources.remove(&request)
            else {
                unreachable!("checked bound group")
            };
            members.push(Member {
                request,
                token: preparation,
                permit: *token,
                packet: Some(packet),
                task: None,
            });
        }
        if members
            .iter()
            .any(|member| !member.packet.as_ref().unwrap().ready())
        {
            self.restore_group_resources(members, None);
            self.fail_http_preparation(first_token, HttpPrepareError::Expired);
            return;
        }
        let now = (self.clock)();
        match self.engine.claim_http_group(now, permit.clone()) {
            Ok(GroupClaim::Released(releases)) => {
                assert_eq!(members.len(), releases.len(), "group release cardinality");
                for (member, release) in members.iter_mut().zip(releases) {
                    let packet = member.packet.take().unwrap();
                    let events = self.events.clone();
                    let clock = &self.clock;
                    member.task = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let raw_now = clock();
                        if !raw_now.is_finite()
                            || !release.send_before.is_finite()
                            || raw_now >= release.send_before
                        {
                            return None;
                        }
                        Some(packet.dispatch(events))
                    }))
                    .ok()
                    .flatten();
                }
                // All sends above precede resource notices and engine events.
                for member in members {
                    if let Some(task) = member.task {
                        self.managed_http.resources.insert(
                            member.request,
                            Resource::InFlight {
                                token: member.token,
                                _task: Task(task),
                            },
                        );
                    } else {
                        let _ = self.events.send(HostEvent::HttpFailed(
                            member.request,
                            "HTTP transport request failed".into(),
                        ));
                    }
                }
            }
            Ok(GroupClaim::WaitUntil(at)) => {
                let events = self.events.clone();
                let delay = Duration::from_secs_f64((at - now).max(0.) / 1_000.);
                let task = tokio::spawn(async move {
                    tokio::time::sleep(delay).await;
                    let _ = events.send(HostEvent::HttpGroupWake(permit));
                });
                self.restore_group_resources(members, Some(Task(task.abort_handle())));
            }
            Err(_) => self.restore_group_resources(members, None),
        }
    }

    fn restore_group_resources(&mut self, members: Vec<Member>, mut wake: Option<Task>) {
        for member in members {
            self.managed_http.resources.insert(
                member.request,
                Resource::Bound {
                    token: member.token,
                    permit: member.permit,
                    packet: member.packet.unwrap(),
                    _wake: wake.take(),
                },
            );
        }
    }
}

#[cfg(all(test, feature = "http-plans"))]
#[path = "managed_http_group_tests.rs"]
mod tests;
