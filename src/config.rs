use serde::{Deserialize, Serialize};

use std::env;
use std::fs::File;
use std::io::Read;

use crate::utils;
use crate::{Project, Section};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub bot_user_id: String,
    pub bot_password: String,
    pub reporting_room_id: String,
    pub admin_room_id: String,
    pub approval_emoji: String,
    pub editors: Vec<String>,
    pub sections: Vec<Section>,
    pub projects: Vec<Project>,
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

    pub fn section_by_emoji(&self, emoji: &str) -> Option<Section> {
        for section in &self.sections {
            if utils::emoji_cmp(&section.emoji, emoji) {
                return Some(section.clone());
            }
        }
        None
    }

    pub fn project_by_emoji(&self, emoji: &str) -> Option<Project> {
        for project in &self.projects {
            if utils::emoji_cmp(&project.emoji, emoji) {
                return Some(project.clone());
            }
        }
        None
    }
}
