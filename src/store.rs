use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Read;

use crate::error::Error;

#[derive(Clone, Debug)]
enum EmojiType {
    Approval,
    Section,
    Project,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct News {
    pub event_id: String,
    pub reporter_id: String,
    pub reporter_display_name: String,
    pub message: String,
    pub approvals: HashMap<String, String>,
    pub sections: HashMap<String, String>,
    pub projects: HashMap<String, String>,
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

    pub fn remove_news(&mut self, redacted_event_id: &str) -> Result<News, Error> {
        if let Some(news) = self.news_map.remove(redacted_event_id) {
            debug!("Removed {:#?}", &news);
            self.write_data();
            return Ok(news);
        }

        Err(Error::NewsEventIdNotFound)
    }

    pub fn update_news(
        &mut self,
        news_event_id: String,
        updated_text: String,
    ) -> Result<News, Error> {
        if let Some(news) = self.news_map.get(&news_event_id) {
            let mut updated_news = news.clone();
            updated_news.message = updated_text;

            info!("Updated news entry with event id {}", news_event_id);
            self.news_map.insert(news_event_id, updated_news.clone());

            self.write_data();
            return Ok(updated_news);
        }

        Err(Error::NewsEventIdNotFound)
    }

    /// Add news approval, returns updated news entry
    pub fn add_news_approval(
        &mut self,
        news_event_id: &str,
        reaction_event_id: &str,
        reaction_emoji: String,
    ) -> Result<News, Error> {
        self.add_news_emoji(
            EmojiType::Approval,
            news_event_id,
            reaction_event_id,
            reaction_emoji,
        )
    }

    /// Add news section, returns updated news entry
    pub fn add_news_section(
        &mut self,
        news_event_id: &str,
        reaction_event_id: &str,
        reaction_emoji: String,
    ) -> Result<News, Error> {
        self.add_news_emoji(
            EmojiType::Section,
            news_event_id,
            reaction_event_id,
            reaction_emoji,
        )
    }

    /// Add news project, returns updated news entry
    pub fn add_news_project(
        &mut self,
        news_event_id: &str,
        reaction_event_id: &str,
        reaction_emoji: String,
    ) -> Result<News, Error> {
        self.add_news_emoji(
            EmojiType::Project,
            news_event_id,
            reaction_event_id,
            reaction_emoji,
        )
    }

    /// Remove news approval, returns updated news entry
    pub fn remove_news_approval(&mut self, redacted_event_id: &str) -> Result<News, Error> {
        self.remove_news_emoji(EmojiType::Approval, redacted_event_id)
    }

    /// Remove news section, returns updated news entry
    pub fn remove_news_section(&mut self, redacted_event_id: &str) -> Result<News, Error> {
        self.remove_news_emoji(EmojiType::Section, redacted_event_id)
    }

    /// Remove news project, returns updated news entry
    pub fn remove_news_project(&mut self, redacted_event_id: &str) -> Result<News, Error> {
        self.remove_news_emoji(EmojiType::Project, redacted_event_id)
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

    fn add_news_emoji(
        &mut self,
        emoji_type: EmojiType,
        news_event_id: &str,
        reaction_event_id: &str,
        reaction_emoji: String,
    ) -> Result<News, Error> {
        if let Some(news) = self.news_map.get(news_event_id) {
            let mut updated_news = news.clone();

            match emoji_type {
                EmojiType::Approval => updated_news
                    .approvals
                    .insert(reaction_event_id.to_string(), reaction_emoji),
                EmojiType::Section => updated_news
                    .sections
                    .insert(reaction_event_id.to_string(), reaction_emoji),
                EmojiType::Project => updated_news
                    .projects
                    .insert(reaction_event_id.to_string(), reaction_emoji),
            };
            self.news_map
                .insert(news_event_id.to_string(), updated_news.clone());

            self.write_data();
            Ok(updated_news)
        } else {
            warn!(
                "Cannot add {:?} emoji, news event id {} not found",
                emoji_type, news_event_id
            );
            Err(Error::NewsEventIdNotFound)
        }
    }

    /// Tries to remove a news approval
    fn remove_news_emoji(
        &mut self,
        emoji_type: EmojiType,
        redacted_event_id: &str,
    ) -> Result<News, Error> {
        for news in self.news_map.values() {
            let map = match emoji_type {
                EmojiType::Approval => &news.approvals,
                EmojiType::Section => &news.sections,
                EmojiType::Project => &news.projects,
            };

            if map.contains_key(redacted_event_id) {
                let mut updated_news = news.clone();

                match emoji_type {
                    EmojiType::Approval => {
                        updated_news.approvals.remove(redacted_event_id).unwrap()
                    }
                    EmojiType::Section => updated_news.sections.remove(redacted_event_id).unwrap(),
                    EmojiType::Project => updated_news.projects.remove(redacted_event_id).unwrap(),
                };

                let news_id = updated_news.event_id.clone();
                self.news_map.insert(news_id, updated_news.clone());
                self.write_data();

                return Ok(updated_news);
            }
        }

        warn!(
            "Unable to remove {:?} emoji, no matching event id found",
            emoji_type
        );
        Err(Error::RedactionEventIdNotFound)
    }
}
