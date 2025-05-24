use matrix_sdk::ruma::{OwnedUserId, UserId};
use rand::Rng;
use serde::{Deserialize, Serialize};

use std::collections::HashSet;

use crate::{utils, Project, ReactionType, Section};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub bot_user_id: String,
    pub reporting_room_id: String,
    pub admin_room_id: String,
    pub notice_emoji: String,
    pub restrict_notice: bool,
    pub verbs: Vec<String>,
    pub min_length: usize,
    pub ack_text: String,
    pub update_config_command: String,
    pub editors: Vec<OwnedUserId>,
    pub sections: Vec<Section>,
    pub projects: Vec<Project>,
}

pub struct ConfigResult {
    pub config: Config,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
}

impl Config {
    pub fn read() -> ConfigResult {
        let data = utils::file_from_env("CONFIG_PATH", "./config.toml");
        let config: Config = toml::from_str(&data).expect("Unable to parse config file");
        Self::validate_config(config)
    }

    pub fn section_by_name(&self, name: &str) -> Option<Section> {
        for section in &self.sections {
            if section.name == name {
                return Some(section.clone());
            }
        }
        None
    }

    pub fn project_by_name(&self, name: &str) -> Option<Project> {
        for project in &self.projects {
            if project.name == name {
                return Some(project.clone());
            }
        }
        None
    }

    pub fn reaction_type_by_emoji(&self, emoji: &str) -> ReactionType {
        if utils::emoji_cmp(&self.notice_emoji, emoji) {
            return ReactionType::Notice;
        } else {
            // section
            for section in &self.sections {
                if utils::emoji_cmp(&section.emoji, emoji) {
                    return ReactionType::Section(Some(section.clone()));
                }
            }
            // project
            for project in &self.projects {
                if utils::emoji_cmp(&project.emoji, emoji) {
                    return ReactionType::Project(Some(project.clone()));
                }
            }
        }

        ReactionType::None
    }

    pub fn sections_by_usual_reporter(&self, reporter: &UserId) -> Vec<Section> {
        let mut sections_for_this_reporter = Vec::<Section>::new();
        for section in &self.sections {
            if section.usual_reporters.contains(&reporter.to_owned()) {
                sections_for_this_reporter.push(section.clone())
            }
        }

        sections_for_this_reporter
    }

    pub fn random_verb(&self) -> String {
        let mut rng = rand::rng();
        let id = rng.random_range(0..self.verbs.len());
        self.verbs[id].to_string()
    }

    fn validate_config(config: Self) -> ConfigResult {
        let mut warnings = Vec::new();
        let mut notes = Vec::new();

        // Check if something is missing / empty
        if config.notice_emoji.is_empty() {
            warnings.insert(
                0,
                "At least one emoji isn’t configured. The bot will not work properly.".to_string(),
            );
        }

        if config.editors.is_empty() {
            warnings.insert(
                0,
                "No editor is specified, the bot cannot be used without an editor".to_string(),
            );
        }

        if config.sections.is_empty() {
            notes.insert(
                0,
                "No sections are configured in the configuration file.".to_string(),
            );
        }

        if config.projects.is_empty() {
            warnings.insert(
                0,
                "No projects are configured in the configuration file.".to_string(),
            );
        }

        let mut section_names = Vec::new();
        for section in &config.sections {
            if section.name.is_empty() {
                warnings.insert(
                    0,
                    "Section without name found, this can lead to undefined behavior.".to_string(),
                );
                continue;
            }

            section_names.insert(0, section.name.clone());

            if section.emoji.is_empty() {
                warnings.insert(
                    0,
                    format!(
                        "Section “{}” doesn’t have an emoji, this can lead to undefined behavior.",
                        section.name
                    ),
                );
            }
        }

        for project in &config.projects {
            if project.name.is_empty() {
                warnings.insert(
                    0,
                    "Project without name found, this can lead to undefined behavior.".to_string(),
                );
                continue;
            }

            if project.emoji.is_empty() {
                warnings.insert(
                    0,
                    format!(
                        "Project “{}” doesn’t have an emoji, this can lead to undefined behavior.",
                        project.name
                    ),
                );
            }
            if project.default_section.is_empty() {
                warnings.insert(
                    0,
                    format!(
                        "Project “{}” doesn’t have a default section, this can lead to undefined behavior.",
                        project.name
                    ),
                );
                continue;
            }

            if !section_names.contains(&project.default_section) {
                warnings.insert(
                    0,
                    format!(
                        "Project “{}” has an unknown default section “{}”, this can lead to undefined behavior.",
                        project.name,
                        project.default_section
                    ),
                );
            }
        }

        // find duplicated emojis / names
        let mut emojis = HashSet::new();
        let mut emoji_duplicates = Vec::new();
        let mut names = HashSet::new();
        let mut name_duplicates = Vec::new();
        for project in &config.projects {
            if !emojis.insert(project.emoji.clone()) {
                emoji_duplicates.insert(0, project.emoji.clone());
            }
            if !names.insert(project.name.clone()) {
                name_duplicates.insert(0, project.name.clone());
            }
        }
        for section in &config.sections {
            if !emojis.insert(section.emoji.clone()) {
                emoji_duplicates.insert(0, section.emoji.clone());
            }
            if !names.insert(section.name.clone()) {
                name_duplicates.insert(0, section.name.clone());
            }
        }
        emoji_duplicates.sort();
        emoji_duplicates.dedup();
        name_duplicates.sort();
        name_duplicates.dedup();

        if !emoji_duplicates.is_empty() {
            warnings.insert(
                0,
                format!(
                    "At least one emoji is duplicated, this can lead to undefined behavior: {:?} ",
                    emoji_duplicates
                ),
            );
        }
        if !name_duplicates.is_empty() {
            warnings.insert(
                0,
                format!(
                    "At least one name is duplicated, this can lead to undefined behavior: {:?} ",
                    name_duplicates
                ),
            );
        }

        ConfigResult {
            config,
            warnings,
            notes,
        }
    }
}
