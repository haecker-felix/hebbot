use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Project {
    pub emoji: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub website: String,
    pub default_section: String,
}

impl Project {
    pub fn html_details(&self) -> String {
        format!(
            "<b>Project Details</b><br>\
            <b>Emoji</b>: {} <br>\
            <b>Name</b>: {} ({}) <br>\
            <b>Description</b>: {} <br>\
            <b>Website</b>: {} <br>\
            <b>Default Section</b>: {}",
            self.emoji, self.title, self.name, self.description, self.website, self.default_section
        )
    }
}
