#[tokio::main]
async fn main() {
    if let Err(error) = ai_switch_lib::server::run_from_env().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
