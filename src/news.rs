use chrono::{DateTime, Utc};
use matrix_sdk::ruma::OwnedMxcUri;
use serde::{Deserialize, Serialize};

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;

use crate::ReactionType;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct News {
    pub event_id: String,
    pub reporter_id: String,
    pub reporter_display_name: String,
    pub timestamp: DateTime<Utc>,
    message: RefCell<String>,
    section_names: RefCell<HashMap<String, String>>,
    project_names: RefCell<HashMap<String, String>>,
    images: RefCell<HashMap<String, (String, OwnedMxcUri)>>,
    videos: RefCell<HashMap<String, (String, OwnedMxcUri)>>,
}

impl News {
    pub fn new(
        event_id: String,
        reporter_id: String,
        reporter_display_name: String,
        message: String,
    ) -> Self {
        Self {
            event_id,
            reporter_id,
            reporter_display_name,
            timestamp: chrono::Utc::now(),
            message: RefCell::new(message),
            section_names: RefCell::default(),
            project_names: RefCell::default(),
            images: RefCell::default(),
            videos: RefCell::default(),
        }
    }

    pub fn message(&self) -> String {
        self.message.borrow().clone()
    }

    pub fn message_summary(&self) -> String {
        if self.message.borrow().len() > 60 {
            format!("{} …", self.message.borrow().clone().split_at(50).0)
        } else {
            self.message.borrow().clone()
        }
    }

    pub fn set_message(&self, message: String) {
        *self.message.borrow_mut() = message;
    }

    pub fn is_assigned(&self) -> bool {
        !self.project_names.borrow().is_empty() || !self.section_names.borrow().is_empty()
    }

    pub fn section_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.section_names.borrow().values().cloned().collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn add_section_name(&self, event_id: String, emoji: String) {
        self.section_names.borrow_mut().insert(event_id, emoji);
    }

    pub fn project_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.project_names.borrow().values().cloned().collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn add_project_name(&self, event_id: String, emoji: String) {
        self.project_names.borrow_mut().insert(event_id, emoji);
    }

    pub fn images(&self) -> Vec<(String, OwnedMxcUri)> {
        Self::files(&*self.images.borrow())
    }

    pub fn add_image(&self, event_id: String, filename: String, mxc_uri: OwnedMxcUri) {
        self.images
            .borrow_mut()
            .insert(event_id, (filename, mxc_uri));
    }

    pub fn videos(&self) -> Vec<(String, OwnedMxcUri)> {
        Self::files(&*self.videos.borrow())
    }

    pub fn add_video(&self, event_id: String, filename: String, mxc_uri: OwnedMxcUri) {
        self.videos
            .borrow_mut()
            .insert(event_id, (filename, mxc_uri));
    }

    fn files(files: &HashMap<String, (String, OwnedMxcUri)>) -> Vec<(String, OwnedMxcUri)> {
        let mut images_map = HashMap::new();
        let mut images = Vec::new();

        // First we add everything to a HashMap to filter out duplicates
        // eg. having two editors who tagged the same image with the camera emoji
        for (name, uri) in files.values() {
            let path = std::path::Path::new(name);
            let suffix = path
                .extension()
                .map(|osstr| osstr.to_str().unwrap())
                .unwrap_or_else(|| "");
            let filename = format!("{}.{}", uri.media_id().unwrap_or("no-media-id"), suffix);

            images_map.insert(filename, uri.clone());
        }

        for (filename, uri) in images_map {
            images.insert(0, (filename, uri));
        }

        images
    }

    pub fn remove_reaction_id(&self, event_id: &str) -> ReactionType {
        if self.section_names.borrow_mut().remove(event_id).is_some() {
            ReactionType::Section(None)
        } else if self.project_names.borrow_mut().remove(event_id).is_some() {
            ReactionType::Project(None)
        } else {
            ReactionType::None
        }
    }

    pub fn relates_to_reaction_id(&self, reaction_id: &str) -> bool {
        for i in self.section_names.borrow().keys() {
            if i == reaction_id {
                return true;
            }
        }
        for i in self.project_names.borrow().keys() {
            if i == reaction_id {
                return true;
            }
        }
        for i in self.images.borrow().keys() {
            if i == reaction_id {
                return true;
            }
        }
        for i in self.videos.borrow().keys() {
            if i == reaction_id {
                return true;
            }
        }

        false
    }
}

impl PartialOrd for News {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.timestamp.cmp(&other.timestamp))
    }
}

impl Ord for News {
    fn cmp(&self, other: &Self) -> Ordering {
        self.timestamp.cmp(&other.timestamp)
    }
}
