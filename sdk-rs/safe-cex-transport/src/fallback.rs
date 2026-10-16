/// Sticky silent-connect fallback; host candidates come from the generated routing table.
#[derive(Debug)]
pub struct WsDirectFallback {
    max_silent_connects: usize,
    connects: usize,
    messages: usize,
    silent_connects: usize,
    active_index: usize,
}

impl WsDirectFallback {
    pub fn new(max_silent_connects: usize) -> Self {
        Self {
            max_silent_connects,
            connects: 0,
            messages: 0,
            silent_connects: 0,
            active_index: 0,
        }
    }

    pub fn resolve_url<'a>(&mut self, primary: &'a str, fallbacks: &[Option<&'a str>]) -> &'a str {
        let candidates: Vec<_> = fallbacks
            .iter()
            .filter_map(|url| *url)
            .filter(|url| !url.is_empty())
            .collect();
        if self.connects > 0 {
            self.silent_connects = if self.messages == 0 {
                self.silent_connects + 1
            } else {
                0
            };
        }
        self.connects += 1;
        self.messages = 0;
        if self.silent_connects >= self.max_silent_connects && self.active_index < candidates.len()
        {
            self.active_index += 1;
            self.silent_connects = 0;
        }
        self.active_index
            .checked_sub(1)
            .and_then(|index| candidates.get(index).copied())
            .unwrap_or(primary)
    }

    pub fn on_message(&mut self) {
        self.messages += 1;
    }

    pub fn disregard_silence(&mut self) {
        self.messages += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_direct_relay_athena_and_sticks() {
        let mut chain = WsDirectFallback::new(2);
        let urls = [Some("relay"), Some("athena")];
        let actual: Vec<_> = (0..9).map(|_| chain.resolve_url("direct", &urls)).collect();
        assert_eq!(
            actual,
            [
                "direct", "direct", "relay", "relay", "athena", "athena", "athena", "athena",
                "athena"
            ]
        );
        chain.on_message();
        assert_eq!(chain.resolve_url("direct", &urls), "athena");
    }

    #[test]
    fn any_message_and_our_own_silence_break_consecutive_streaks() {
        let mut chain = WsDirectFallback::new(2);
        let urls = [Some("relay"), Some("athena")];
        for _ in 0..10 {
            assert_eq!(chain.resolve_url("direct", &urls), "direct");
            assert_eq!(chain.resolve_url("direct", &urls), "direct");
            chain.disregard_silence();
        }
        assert_eq!(chain.resolve_url("direct", &urls), "direct");
        chain.on_message(); // An authentication error also proves the path works.
        assert_eq!(chain.resolve_url("direct", &urls), "direct");
        assert_eq!(chain.resolve_url("direct", &urls), "direct");
        assert_eq!(chain.resolve_url("direct", &urls), "relay");
    }

    #[test]
    fn filters_disabled_candidates_and_preserves_index_when_candidates_change() {
        let mut chain = WsDirectFallback::new(2);
        let urls = [None, Some(""), Some("athena")];
        assert_eq!(chain.resolve_url("direct", &urls), "direct");
        assert_eq!(chain.resolve_url("direct", &urls), "direct");
        assert_eq!(chain.resolve_url("direct", &urls), "athena");
        assert_eq!(chain.resolve_url("direct", &[]), "direct");
        chain.on_message();
        assert_eq!(
            chain.resolve_url("direct", &[Some("relay"), Some("athena")]),
            "relay"
        );
    }
}
