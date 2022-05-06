use chrono::{DateTime, Utc};
use matrix_sdk::ruma::{EventId, OwnedEventId};

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::{env, fs};

use crate::{Error, News};

pub struct NewsStore {
    news_map: HashMap<OwnedEventId, News>,
}

impl NewsStore {
    pub fn read() -> Self {
        // Try to open+read store.json
        let path = Self::get_path();
        debug!("Trying to read stored news file from path: {:?}", path);

        let news_map = if let Ok(mut file) = File::open(path) {
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

    pub fn remove_news(&mut self, event_id: &EventId) -> Result<News, Error> {
        if let Some(news) = self.news_map.remove(event_id) {
            debug!("Removed {:#?}", &news);
            self.write_data();
            return Ok(news);
        }

        Err(Error::NewsEventIdNotFound)
    }

    pub fn news(&self) -> Vec<News> {
        self.news_map.values().cloned().collect()
    }

    pub fn news_by_message_id(&self, message_event_id: &EventId) -> Option<&News> {
        self.news_map.get(message_event_id)
    }

    pub fn news_by_reaction_id(&self, reaction_event_id: &EventId) -> Option<&News> {
        for n in self.news_map.values() {
            if n.relates_to_reaction_id(reaction_event_id) {
                return Some(n);
            }
        }

        None
    }

    pub fn find_related_news(&self, reporter_id: &str, timestamp: &DateTime<Utc>) -> Option<&News> {
        let mut shortest_time_diff = None;
        let mut related_news = None;

        for news in self.news_map.values() {
            if news.reporter_id != reporter_id {
                continue;
            }

            let time_diff = (news.timestamp.time() - timestamp.time())
                .num_seconds()
                .abs();

            if shortest_time_diff.is_none() {
                related_news = Some(news);
                shortest_time_diff = Some(time_diff);
                continue;
            }

            if time_diff < shortest_time_diff.unwrap() {
                related_news = Some(news);
                shortest_time_diff = Some(time_diff);
            }
        }

        related_news
    }

    /// Wipes all news entries
    pub fn clear_news(&mut self) {
        self.news_map.clear();
        self.write_data();
    }

    /// Writes data as JSON to disk
    pub fn write_data(&self) {
        debug!("Writing data…");
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
