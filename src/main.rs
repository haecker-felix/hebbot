#[macro_use]
extern crate log;

mod bot;
mod config;
mod error;
mod news;
mod news_store;
mod project;
mod reaction_type;
mod render;
mod section;
mod utils;

pub use config::Config;
pub use error::Error;
pub use news::News;
pub use news_store::NewsStore;
pub use project::Project;
pub use reaction_type::ReactionType;
pub use section::Section;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    pretty_env_logger::init();

    info!("Starting hebbot");
    bot::Bot::run().await;
}
