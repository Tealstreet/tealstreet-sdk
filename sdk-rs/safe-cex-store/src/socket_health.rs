use serde::{Deserialize, Serialize, Serializer, ser::SerializeMap};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocketDiagnosticsSource {
    pub last_close_code: Option<f64>,
    pub last_close_at: Option<f64>,
    pub lifetime_close_count: Option<f64>,
    pub on_close_count: Option<f64>,
    pub is_reconnect_pause_active: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SocketDiagnosticsSummary {
    pub private: bool,
    pub close_code: Option<f64>,
    pub close_count_total: f64,
    pub close_streak: f64,
    pub reconnect_paused: bool,
}
impl Serialize for SocketDiagnosticsSummary {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let kind = if self.private { "private" } else { "public" };
        let mut map = serializer.serialize_map(None)?;
        // TS retains an undefined property internally; JSON omits that property.
        if let Some(code) = self.close_code {
            map.serialize_entry(&format!("{kind}CloseCode"), &code)?;
        }
        map.serialize_entry(&format!("{kind}CloseCountTotal"), &self.close_count_total)?;
        map.serialize_entry(&format!("{kind}CloseStreak"), &self.close_streak)?;
        map.serialize_entry(&format!("{kind}ReconnectPaused"), &self.reconnect_paused)?;
        map.end()
    }
}

pub fn summarize_socket_diagnostics(
    sockets: &[Option<SocketDiagnosticsSource>],
    private: bool,
) -> SocketDiagnosticsSummary {
    let mut summary = SocketDiagnosticsSummary {
        private,
        close_code: None,
        close_count_total: 0.0,
        close_streak: 0.0,
        reconnect_paused: false,
    };
    let mut close_at = -1.0;
    for socket in sockets.iter().flatten() {
        summary.close_count_total += socket.lifetime_close_count.unwrap_or(0.0);
        summary.close_streak += socket.on_close_count.unwrap_or(0.0);
        if socket.is_reconnect_pause_active == Some(true) {
            summary.reconnect_paused = true;
        }
        let at = socket.last_close_at.unwrap_or(0.0);
        if socket.last_close_code.is_some() && at > close_at {
            close_at = at;
            summary.close_code = socket.last_close_code;
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    #[test]
    fn recovery_keeps_lifetime_evidence_and_last_close_code() {
        let recovered = SocketDiagnosticsSource {
            lifetime_close_count: Some(150.0),
            on_close_count: Some(0.0),
            last_close_code: Some(1006.0),
            last_close_at: Some(20.0),
            ..SocketDiagnosticsSource::default()
        };
        let unhealthy = SocketDiagnosticsSource {
            lifetime_close_count: Some(5.0),
            on_close_count: Some(3.0),
            last_close_code: Some(4001.0),
            last_close_at: Some(10.0),
            is_reconnect_pause_active: Some(true),
        };
        let summary =
            summarize_socket_diagnostics(&[None, Some(recovered), Some(unhealthy)], false);
        assert_eq!(
            serde_json::to_value(&summary).unwrap(),
            json!({"publicCloseCode":1006.0,"publicCloseCountTotal":155.0,"publicCloseStreak":3.0,"publicReconnectPaused":true})
        );
    }
    #[test]
    fn ties_missing_codes_missing_times_and_empty_sockets() {
        let source = |code, at| {
            Some(SocketDiagnosticsSource {
                last_close_code: code,
                last_close_at: at,
                ..SocketDiagnosticsSource::default()
            })
        };
        let summary = summarize_socket_diagnostics(
            &[
                source(Some(1000.0), None),
                source(Some(1006.0), Some(0.0)),
                source(None, Some(500.0)),
                source(Some(4001.0), Some(-1.0)),
            ],
            true,
        );
        assert_eq!(summary.close_code, Some(1000.0));
        assert_eq!(
            serde_json::to_value(summarize_socket_diagnostics(&[], false)).unwrap(),
            json!({"publicCloseCountTotal":0.0,"publicCloseStreak":0.0,"publicReconnectPaused":false})
        );
    }
    #[test]
    fn frozen_typescript_socket_snapshots() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../tests/socket_oracle.json")).unwrap();
        for case in cases {
            let sockets: Vec<Option<SocketDiagnosticsSource>> =
                serde_json::from_value(case["sockets"].clone()).unwrap();
            let actual = summarize_socket_diagnostics(&sockets, case["kind"] == "private");
            crate::test_support::assert_json_eq(
                &serde_json::to_value(actual).unwrap(),
                &case["expected"],
            );
        }
    }
}
