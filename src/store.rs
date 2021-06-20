use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Read;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct News {
    pub event_id: String,
    pub reporter_id: String,
    pub reporter_display_name: String,
    pub message: String,
    pub approval_count: u32,
}

#[derive(Clone)]
pub struct NewsStore {
    news_map: HashMap<String, News>,
}

impl NewsStore {
    pub fn read() -> Self {
        let path = Self::get_path();
        let mut file = File::open(path).expect("Unable to open news store file");
        let mut data = String::new();
        file.read_to_string(&mut data)
            .expect("Unable to read news store file");

        let news_map: HashMap<String, News> =
            serde_json::from_str(&data).expect("Unable to parse news store file");

        Self { news_map }
    }

    pub fn add_news(&mut self, news: News) {
        debug!("Store {:#?}", &news);

        self.news_map.insert(news.event_id.clone(), news);
        self.write_data();
    }

    pub fn get_news(&self) -> Vec<News> {
        self.news_map.values().map(|n| n.clone()).collect()
    }

    pub fn clear_news(&mut self) {
        self.news_map.clear();
        self.write_data();
    }

    fn write_data(&self) {
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
