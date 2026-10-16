fn main() -> std::process::ExitCode {
    match safe_cex_parity::run_progress() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
