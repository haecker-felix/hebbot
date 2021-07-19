use std::collections::HashMap;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Read;

use crate::{Error, News};

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

    pub fn remove_news(&mut self, event_id: &str) -> Result<News, Error> {
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

    pub fn news_by_message_id<'a>(&'a self, message_event_id: &str) -> Option<&'a News> {
        self.news_map.get(message_event_id)
    }

    pub fn news_by_reaction_id(&self, reaction_event_id: &str) -> Option<&News> {
        for n in self.news_map.values() {
            if n.relates_to_reaction_id(reaction_event_id) {
                return Some(n);
            }
        }

        None
    }

    /// Wipes all news entries
    pub fn clear_news(&mut self) {
        self.news_map.clear();
        self.write_data();
    }

    /// Writes data as JSON to disk
    pub fn write_data(&self) {
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
