use serde::Deserialize;

use crate::{EndpointOptions, invalid};
use safe_cex_engine::VenueError;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkUrls {
    #[serde(default)]
    livenet: String,
    #[serde(default)]
    testnet: String,
}

impl NetworkUrls {
    fn select(self, testnet: bool) -> String {
        if testnet { self.testnet } else { self.livenet }
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Endpoints {
    #[serde(default)]
    rest: NetworkUrls,
    #[serde(default)]
    public_ws: NetworkUrls,
    #[serde(default)]
    private_ws: NetworkUrls,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HyroConfig {
    mode: Option<String>,
    #[serde(default)]
    production: Endpoints,
    #[serde(default)]
    simulator: Endpoints,
}

impl HyroConfig {
    pub(crate) fn is_demo(&self) -> bool {
        self.mode.as_deref() == Some("demo")
    }

    pub fn resolve(self, testnet: bool) -> Result<EndpointOptions, VenueError> {
        // The missing-production fallback checks livenet even for testnet accounts.
        let selected =
            if self.mode.as_deref() == Some("demo") || self.production.rest.livenet.is_empty() {
                self.simulator
            } else {
                self.production
            };
        let endpoints = EndpointOptions {
            rest: selected.rest.select(testnet),
            public_ws: selected.public_ws.select(testnet),
            private_ws: selected.private_ws.select(testnet),
        };
        if endpoints.rest.is_empty()
            || endpoints.public_ws.is_empty()
            || endpoints.private_ws.is_empty()
        {
            return Err(invalid(
                "HyroTrader REST/public/private URLs are not configured",
            ));
        }
        Ok(endpoints)
    }
}
