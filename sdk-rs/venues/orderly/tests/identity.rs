use safe_cex_engine::{Action, Command, Event, Notice};
use safe_cex_venue_orderly::{Options, Orderly};

fn fetch_account(options: Options) -> Result<String, String> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Orderly::new(options));
    engine.handle(0.0, Event::Command(Command::FetchAccount { command: 4 }));
    engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone { command: 4, result }) => Some(Ok(result)),
            Action::Notice(Notice::CommandFailed { command: 4, error }) => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .expect("FetchAccount completes")
}

#[test]
fn account_identity_is_the_broker_application() {
    assert_eq!(
        fetch_account(Options {
            application_id: Some("0xaccount".into()),
            ..Options::default()
        }),
        Ok(r#"{"userId":"0xaccount","affiliateId":"0xaccount"}"#.into())
    );
}

#[test]
fn public_or_unidentified_accounts_have_no_identity() {
    for options in [
        Options {
            application_id: Some("0xaccount".into()),
            public_data_only: true,
            ..Options::default()
        },
        Options {
            application_id: Some(String::new()),
            ..Options::default()
        },
        Options::default(),
    ] {
        assert_eq!(
            fetch_account(options),
            Err("Orderly private account ID is required".into())
        );
    }
}
