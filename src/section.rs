use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Section {
    pub emoji: String,
    pub name: String,
    pub title: String,
    pub order: u32,
}

impl Section {
    pub fn html_details(&self) -> String {
        format!(
            "<b>Section Details</b><br>\
            <b>Emoji</b>: {} <br>\
            <b>Name</b>: {} ({}) <br>\
            <b>Order</b>: {}",
            self.emoji, self.title, self.name, self.order
        )
    }
}

impl PartialOrd for Section {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.order.cmp(&other.order))
    }
}
