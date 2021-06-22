use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Read;

use crate::error::Error;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct News {
    pub event_id: String,
    pub reporter_id: String,
    pub reporter_display_name: String,
    pub message: String,
    pub approvals: Vec<String>,
}

#[derive(Clone)]
pub struct NewsStore {
    news_map: HashMap<String, News>,
}

impl NewsStore {
    pub fn read() -> Self {
        // Try to open+read store.json
        let path = Self::get_path();
        let news_map: HashMap<String, News> = if let Ok(mut file) = File::open(path) {
            let mut data = String::new();
            file.read_to_string(&mut data)
                .expect("Unable to read news store file");
            serde_json::from_str(&data).expect("Unable to parse news store file")
        } else {
            warn!("Unable to open news store file");
            HashMap::new()
        };

        Self { news_map }
    }

    pub fn add_news(&mut self, news: News) {
        debug!("Store {:#?}", &news);

        self.news_map.insert(news.event_id.clone(), news);
        self.write_data();
    }

    /// Tries to add a news approval
    pub fn approve_news(
        &mut self,
        news_event_id: &str,
        reaction_event_id: &str,
    ) -> Result<News, Error> {
        if let Some(news) = self.news_map.get(news_event_id) {
            let mut updated_news = news.clone();
            let reaction_id = reaction_event_id.to_string();
            let news_id = news_event_id.to_string();

            updated_news.approvals.insert(0, reaction_id);
            self.news_map.insert(news_id, updated_news.clone());

            self.write_data();
            Ok(updated_news)
        } else {
            warn!("Cannot approve news, event_id not found");
            Err(Error::NewsEventIdNotFound)
        }
    }

    /// Tries to remove a news approval
    pub fn unapprove_news(&mut self, redacted_event_id: &str) -> Result<News, Error> {
        // Check if we have a news approval with a matching reaction event_id (=redacted_event_id)
        for n in self.news_map.values() {
            for (i, approval) in n.approvals.iter().enumerate() {
                if approval == redacted_event_id {
                    let mut updated_news = n.clone();
                    updated_news.approvals.remove(i);

                    let id = updated_news.event_id.clone();
                    self.news_map.insert(id, updated_news.clone());

                    self.write_data();
                    return Ok(updated_news);
                }
            }
        }

        warn!(
            "Cannot unapprove news, no reaction id {} found",
            redacted_event_id
        );
        Err(Error::ApprovalReactionIdNotFound)
    }

    pub fn get_news(&self) -> Vec<News> {
        self.news_map.values().cloned().collect()
    }

    /// Wipes all news entries
    pub fn clear_news(&mut self) {
        self.news_map.clear();
        self.write_data();
    }

    /// Writes data as JSON to disk
    fn write_data(&self) {
        debug!("Write data...");
        let json = serde_json::to_string_pretty(&self.news_map).unwrap();
        let path = Self::get_path();
        fs::write(path, json).expect("Unable to write news store");
    }

    fn get_path() -> String {
        match env::var("STORE_PATH") {
            Ok(val) => val,
            Err(_) => "./store.json".to_string(),
        }
    }
}
