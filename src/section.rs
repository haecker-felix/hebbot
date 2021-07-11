use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Section {
    pub title: String,
    pub emoji: String,
}
