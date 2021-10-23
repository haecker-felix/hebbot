use std::fmt;

use crate::{Project, Section};

#[derive(Clone, Debug, PartialEq)]
pub enum ReactionType {
    Section(Option<Section>),
    Project(Option<Project>),
    Image,
    Video,
    None,
    Notice,
}

impl fmt::Display for ReactionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReactionType::Section(_) => write!(f, "section"),
            ReactionType::Project(_) => write!(f, "project"),
            ReactionType::Image => write!(f, "image"),
            ReactionType::Video => write!(f, "video"),
            ReactionType::None => write!(f, "NONE"),
            ReactionType::Notice => write!(f, "notice"),
        }
    }
}
