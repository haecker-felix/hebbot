use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Project {
    pub emoji: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub website: String,
    pub default_section: String,
    pub usual_reporters: Vec<String>,
}

impl Project {
    pub fn html_details(&self) -> String {
        let content = format!(
            "<b>Project Details</b><br>\
            <b>Emoji</b>: {} <br>\
            <b>Name</b>: {} ({}) <br>\
            <b>Description</b>: {} <br>\
            <b>Website</b>: {} <br>\
            <b>Default Section</b>: {} <br>\
            <b>Usual reporters</b>: ",
            self.emoji, self.title, self.name, self.description, self.website, self.default_section
        );

        let mut reporters = String::new();
        for usual_reporter in &self.usual_reporters {
            reporters = reporters + &usual_reporter + ", ";
        }

        reporters.pop();
        reporters.pop();
        format!("{} {}", content, reporters)
    }
}
