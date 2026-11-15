//! Passive lot outcomes; dispatch and stop policy remain in trading.
use safe_cex_engine::write_plan::{AppliedEffect, Element, ElementResult, WritePlan};
use safe_cex_engine::{CommandId, Level, LogEvent, Outcome, RequestId, VenueError};

pub(crate) struct SplitReport {
    plan: WritePlan,
    next: usize,
    pub request_id: Option<RequestId>,
    pub status: Option<u16>,
}

impl SplitReport {
    pub fn new(elements: Vec<Element>) -> Option<Self> {
        (elements.len() > 1).then(|| Self {
            plan: WritePlan::new(elements),
            next: 0,
            request_id: None,
            status: None,
        })
    }

    pub fn placed(&mut self, ids: &[String]) {
        self.record(ElementResult::Placed {
            order_ids: ids.to_vec(),
        });
    }

    pub fn canceled(&mut self, target: &str) {
        self.record(ElementResult::Applied {
            effect: AppliedEffect::Cancel,
            target_order_id: Some(target.into()),
            position_key: None,
        });
    }

    fn record(&mut self, result: ElementResult) {
        self.plan
            .record(self.next, result)
            .expect("accepted split write has a reporting index");
        self.next += 1;
    }

    pub fn failed(&mut self, error: &VenueError) {
        let result = if error.outcome == Some(Outcome::Unknown) {
            ElementResult::Unknown {
                error: Box::new(error.clone()),
            }
        } else {
            ElementResult::NotSent {
                error: Some(Box::new(error.clone())),
            }
        };
        self.plan
            .record(self.next, result)
            .expect("failed split lot has a reporting index");
    }

    pub fn attach(&self, error: VenueError) -> VenueError {
        self.plan.attach(error)
    }

    pub fn unknown_event(&self, command: CommandId, ids: &[String]) -> LogEvent {
        let mut placed = String::new();
        for id in ids
            .iter()
            .filter(|id| (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()))
            .take(16)
        {
            let separator = usize::from(!placed.is_empty());
            if placed.len() + separator + id.len() > safe_cex_engine::log::MAX_LOG_BYTES {
                break;
            }
            if separator != 0 {
                placed.push(',');
            }
            placed.push_str(id);
        }
        let mut fields = vec![
            ("command", format!("woo-{command}").into()),
            ("outcome", "unknown".into()),
            ("placedOrderIds", placed.into()),
            (
                "unknownIndices",
                u16::try_from(self.next)
                    .map(|i| i.to_string())
                    .unwrap_or_default()
                    .into(),
            ),
        ];
        if let Some(request) = self.request_id {
            fields.push(("request", request.0.to_string().into()));
        }
        if let Some(status) = self.status.filter(|status| (100..=599).contains(status)) {
            fields.push(("status", u32::from(status).into()));
        }
        LogEvent {
            level: Level::Warn,
            code: "order.outcome_unknown",
            message: safe_cex_engine::UNKNOWN_OUTCOME_TEXT.into(),
            fields,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_uses_only_bounded_closed_nx_fields() {
        let mut report = SplitReport::new(
            (0..2)
                .map(|index| Element {
                    index,
                    client_order_id: None,
                })
                .collect(),
        )
        .unwrap();
        let ids: Vec<_> = [
            "hostile https://venue.test?signature=payload body={}".into(),
            "9".repeat(33),
        ]
        .into_iter()
        .chain((1..=20).map(|id| id.to_string()))
        .collect();
        for correlation_id in [0, u64::MAX] {
            for status in [
                None,
                Some(0),
                Some(99),
                Some(100),
                Some(599),
                Some(600),
                Some(u16::MAX),
            ] {
                report.request_id = Some(RequestId(correlation_id));
                report.status = status;
                let event = report.unknown_event(u32::MAX, &ids);
                assert!(
                    event.fields.contains(&(
                        "placedOrderIds",
                        (1..=16)
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                            .into()
                    ))
                );
                assert!(
                    event
                        .fields
                        .contains(&("request", correlation_id.to_string().into()))
                );
                assert!(event.fields.contains(&("command", "woo-4294967295".into())));
                let observed = event
                    .fields
                    .iter()
                    .find(|(key, _)| *key == "status")
                    .map(|(_, value)| value.clone());
                assert_eq!(
                    observed,
                    status
                        .filter(|s| (100..=599).contains(s))
                        .map(|s| u32::from(s).into())
                );
                assert_eq!(event.fields.len(), 5 + usize::from(observed.is_some()));
            }
        }
        report.next = 65535;
        let event = report.unknown_event(0, &vec!["9".repeat(32); 20]);
        assert!(event.fields.contains(&("unknownIndices", "65535".into())));
        let placed = event
            .fields
            .iter()
            .find(|(key, _)| *key == "placedOrderIds")
            .unwrap()
            .1
            .as_str()
            .unwrap();
        assert_eq!(placed.split(',').count(), 15);
        assert!(placed.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
        report.next = 65536;
        assert!(
            report
                .unknown_event(0, &[])
                .fields
                .contains(&("unknownIndices", "".into()))
        );
    }
}
