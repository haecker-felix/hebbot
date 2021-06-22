use serde::Deserialize;

use std::env;
use std::fs::File;
use std::io::Read;

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    pub bot_user_id: String,
    pub bot_password: String,
    pub reporting_room_id: String,
    pub admin_room_id: String,
    pub approval_emoji: char,
    pub editors: Vec<String>,
}

impl Config {
    pub fn read() -> Self {
        let path = match env::var("CONFIG_PATH") {
            Ok(val) => val,
            Err(_) => "./config.json".to_string(),
        };

        let mut file = File::open(path).expect("Unable to open configuration file");
        let mut data = String::new();
        file.read_to_string(&mut data)
            .expect("Unable to read configuration file");

        serde_json::from_str(&data).expect("Unable to parse configuration file")
    }
}
