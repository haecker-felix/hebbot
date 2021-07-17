use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Project {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub website: String,
    pub emoji: String,
}
