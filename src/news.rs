use serde::{Deserialize, Serialize};

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::ReactionType;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct News {
    pub event_id: String,
    pub reporter_id: String,
    pub reporter_display_name: String,
    message: RefCell<String>,
    approvals: RefCell<HashSet<String>>,
    section_names: RefCell<HashMap<String, String>>,
    project_names: RefCell<HashMap<String, String>>,
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
            message: RefCell::new(message),
            approvals: RefCell::default(),
            section_names: RefCell::default(),
            project_names: RefCell::default(),
        }
    }

    pub fn message(&self) -> String {
        self.message.borrow().clone()
    }

    pub fn set_message(&self, message: String) {
        *self.message.borrow_mut() = message;
    }

    pub fn is_approved(&self) -> bool {
        !self.approvals.borrow().is_empty()
    }

    pub fn add_approval(&self, event_id: String) {
        self.approvals.borrow_mut().insert(event_id);
    }

    pub fn section_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .section_names
            .borrow()
            .values()
            .map(|s| s.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn add_section_name(&self, event_id: String, emoji: String) {
        self.section_names.borrow_mut().insert(event_id, emoji);
    }

    pub fn project_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .project_names
            .borrow()
            .values()
            .map(|s| s.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn add_project_name(&self, event_id: String, emoji: String) {
        self.project_names.borrow_mut().insert(event_id, emoji);
    }

    pub fn remove_reaction_id(&self, event_id: &String) -> ReactionType {
        if self.approvals.borrow_mut().remove(event_id) {
            ReactionType::Approval
        } else if self.section_names.borrow_mut().remove(event_id).is_some() {
            ReactionType::Section(None)
        } else if self.project_names.borrow_mut().remove(event_id).is_some() {
            ReactionType::Project(None)
        } else {
            ReactionType::None
        }
    }

    pub fn relates_to_reaction_id(&self, reaction_id: &String) -> bool {
        for i in &*self.approvals.borrow() {
            if i == reaction_id {
                return true;
            }
        }
        for (i, _) in &*self.section_names.borrow() {
            if i == reaction_id {
                return true;
            }
        }
        for (i, _) in &*self.project_names.borrow() {
            if i == reaction_id {
                return true;
            }
        }

        false
    }
}
