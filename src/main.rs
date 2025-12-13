#[macro_use]
extern crate log;

mod bot;
mod bot_message_type;
mod config;
mod error;
mod news;
mod news_store;
mod project;
mod reaction_type;
mod render;
mod section;
mod utils;

use bot_message_type::BotMessageType;
use config::Config;
use error::Error;
use news::News;
use news_store::NewsStore;
use project::Project;
use reaction_type::ReactionType;
use section::Section;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    pretty_env_logger::init_timed();

    info!("Starting hebbot");
    bot::Bot::run().await;
}
