use chrono::{DateTime, Utc};
use matrix_sdk::ruma::{EventId, OwnedEventId, OwnedMxcUri, OwnedUserId};
use serde::{Deserialize, Serialize};

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;

use crate::ReactionType;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct News {
    pub event_id: OwnedEventId,
    pub reporter_id: OwnedUserId,
    pub reporter_display_name: String,
    pub timestamp: DateTime<Utc>,
    message: RefCell<String>,
    section_names: RefCell<HashMap<OwnedEventId, String>>,
    project_names: RefCell<HashMap<OwnedEventId, String>>,
    // <Reaction event id, (file event id, filename, mxc uri)>
    images: RefCell<HashMap<OwnedEventId, (OwnedEventId, String, OwnedMxcUri)>>,
    videos: RefCell<HashMap<OwnedEventId, (OwnedEventId, String, OwnedMxcUri)>>,
}

impl News {
    pub fn new(
        event_id: OwnedEventId,
        reporter_id: OwnedUserId,
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
            format!(
                "{} …",
                self.message
                    .borrow()
                    .clone()
                    .chars()
                    .take(50)
                    .collect::<String>()
            )
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

    pub fn add_section_name(&self, event_id: OwnedEventId, emoji: String) {
        self.section_names.borrow_mut().insert(event_id, emoji);
    }

    pub fn project_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.project_names.borrow().values().cloned().collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn add_project_name(&self, event_id: OwnedEventId, emoji: String) {
        self.project_names.borrow_mut().insert(event_id, emoji);
    }

    pub fn images(&self) -> Vec<(String, OwnedMxcUri)> {
        Self::deduplicate_files(&self.images.borrow())
    }

    pub fn add_image(
        &self,
        reaction_event_id: OwnedEventId,
        image_event_id: OwnedEventId,
        filename: String,
        mxc_uri: OwnedMxcUri,
    ) {
        self.images
            .borrow_mut()
            .insert(reaction_event_id, (image_event_id, filename, mxc_uri));
    }

    pub fn videos(&self) -> Vec<(String, OwnedMxcUri)> {
        Self::deduplicate_files(&self.videos.borrow())
    }

    pub fn add_video(
        &self,
        reaction_event_id: OwnedEventId,
        video_event_id: OwnedEventId,
        filename: String,
        mxc_uri: OwnedMxcUri,
    ) {
        self.videos
            .borrow_mut()
            .insert(reaction_event_id, (video_event_id, filename, mxc_uri));
    }

    /// Remove a image or video file from this news
    pub fn remove_file(
        &self,
        event_id: &OwnedEventId,
    ) -> Option<(OwnedEventId, String, OwnedMxcUri)> {
        let img = self.images.borrow_mut().remove(event_id);
        let vid = self.videos.borrow_mut().remove(event_id);

        img.or(vid)
    }

    /// Deduplicates files based on the mxc uri.
    /// Can happen when multiple editors reacted to the same file
    /// -> File is listed for every single reaction
    fn deduplicate_files(
        files: &HashMap<OwnedEventId, (OwnedEventId, String, OwnedMxcUri)>,
    ) -> Vec<(String, OwnedMxcUri)> {
        let mut deduplicated = HashMap::new();

        for (_event_id, filename, mxc_uri) in files.values() {
            // The filenames aren't guaranteed to be unique ("image.png"), so prefix them with the media id
            let unique_name = format!("{}_{}", mxc_uri.media_id().unwrap_or_default(), filename);

            deduplicated.insert(mxc_uri.clone(), (unique_name, mxc_uri.clone()));
        }

        deduplicated.values().cloned().collect()
    }

    pub fn remove_reaction_id(&self, event_id: &EventId) -> ReactionType {
        if self.section_names.borrow_mut().remove(event_id).is_some() {
            ReactionType::Section(None)
        } else if self.project_names.borrow_mut().remove(event_id).is_some() {
            ReactionType::Project(None)
        } else {
            ReactionType::None
        }
    }

    pub fn relates_to_reaction_id(&self, reaction_id: &EventId) -> bool {
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
