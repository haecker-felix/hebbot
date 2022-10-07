use serde::{de, Deserialize, Serialize, Serializer};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Project {
    #[serde(
        serialize_with = "serialize_emoji",
        deserialize_with = "deserialize_emoji"
    )]
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
            <b>Default Section</b>: {} <br>",
            self.emoji, self.title, self.name, self.description, self.website, self.default_section,
        )
    }
}

fn deserialize_emoji<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: de::Deserializer<'de>,
{
    Ok(format!("{}?", String::deserialize(deserializer)?))
}

fn serialize_emoji<S>(emoji: &String, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut emoji_without_question_mark = emoji.to_string();
    emoji_without_question_mark.pop();
    s.serialize_str(&emoji_without_question_mark)
}
