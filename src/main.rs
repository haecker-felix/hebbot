#[macro_use]
extern crate log;

mod bot;
mod config;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    pretty_env_logger::init();

    info!("Starting hebbot");
    bot::Bot::run().await;
}
