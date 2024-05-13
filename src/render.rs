use matrix_sdk::room::RoomMember;
use matrix_sdk::ruma::{EventId, OwnedMxcUri};
use serde::{Deserialize, Serialize};

use std::collections::{BTreeMap, HashSet};

use crate::{utils, Config, News, Project, Section};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct RenderProject {
    pub project: Project,
    pub news: Vec<News>,

    // For news with overwritten section information (-> doesn't match project default_section)
    pub overwritten_section: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct RenderSection {
    pub section: Section,
    pub projects: Vec<RenderProject>,

    // For news without project information
    pub news: Vec<News>,
}

pub struct RenderResult {
    pub rendered: String,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
    pub images: Vec<(String, OwnedMxcUri)>,
    pub videos: Vec<(String, OwnedMxcUri)>,
}

fn random_filter(value: minijinja::Value) -> Result<minijinja::Value, minijinja::Error> {
    if let Some(len) = value.len() {
        value.get_item_by_index(rand::random::<usize>() % len)
    } else {
        Ok(value)
    }
}

lazy_static::lazy_static! {
    static ref TEMPLATE_TEXT: String = {
        use std::io::Read;

        let path = std::env::var("TEMPLATE_PATH").unwrap_or("template.md".into());
        debug!("Reading template from file path: {:?}", path);

        let mut text = String::new();
        std::fs::File::open(path)
            .expect("Unable to open template file")
            .read_to_string(&mut text)
            .expect("Unable to read template file");

        text
    };

    static ref JINJA_ENV: minijinja::Environment<'static> = {
        let mut env = minijinja::Environment::new();
        minijinja_contrib::add_to_environment(&mut env);
        env.add_template("template", &TEMPLATE_TEXT).unwrap();
        env.add_filter("random", random_filter);
        env
    };
}

fn render_template(
    render_sections: &BTreeMap<String, RenderSection>,
    config: &Config,
    editor: &RoomMember,
) -> Option<String> {
    let template = JINJA_ENV.get_template("template").unwrap();

    let result = template
        .render(minijinja::context! {
            sections => render_sections,
            config => config,
            editor => utils::get_member_display_name(editor),
        })
        .unwrap();

    println!("{}", result);
    Some(result)
}

pub fn render(news_list: Vec<News>, config: Config, editor: &RoomMember) -> RenderResult {
    let mut render_projects: BTreeMap<String, RenderProject> = BTreeMap::new();
    let mut render_sections: BTreeMap<String, RenderSection> = BTreeMap::new();

    let mut news_count = 0;
    let mut not_assigned = 0;
    let mut project_names: HashSet<String> = HashSet::new();

    let mut images: Vec<(String, OwnedMxcUri)> = Vec::new();
    let mut videos: Vec<(String, OwnedMxcUri)> = Vec::new();

    let mut warnings: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    // Sort news entries into `RenderProject`s (`render_projects`)
    for news in news_list {
        let message_link = message_link(&config, &news.event_id);

        // Skip news entries which are not assigned
        if !news.is_assigned() {
            not_assigned += 1;
            continue;
        }

        // Check if the news entry has multiple project/information set
        if news.project_names().len() > 1 || news.section_names().len() > 1 {
            warnings.insert(0, format!("[{}] News entry by {} has multiple project or section information set, it’ll appear multiple times. This is probably not wanted!", message_link, news.reporter_display_name));
        }

        // Check if the news entry has at one project or section information added
        if news.project_names().is_empty() && news.section_names().is_empty() {
            warnings.insert(0, format!("[{}] News entry by {} doesn’t have project/section information, it’ll not appear in the rendered markdown!", message_link, news.reporter_display_name));
            continue;
        }

        // The news entry is assigned to a project / section, and will be rendered -> increase counter.
        news_count += 1;

        // Get news images / videos
        images.append(&mut news.images().clone());
        videos.append(&mut news.videos().clone());

        // Add news entries without any project information (but with section information) directly to the specified `RenderSection`
        if news.project_names().is_empty() {
            notes.insert(0, format!("[{}] News entry by {} doesn’t have project information, it’ll appear directly in the section without any project description.", message_link, news.reporter_display_name));

            for section_name in news.section_names() {
                let section = config.section_by_name(&section_name).unwrap();
                let map_section_name = format!("{}-{}", section.order, section_name);

                match render_sections.get_mut(&map_section_name) {
                    // RenderSection already exists -> Add news entry to it
                    Some(render_section) => {
                        render_section.news.insert(0, news.clone());
                    }
                    // RenderSection doesn't exist yet -> Create it, and add news entry to it
                    None => {
                        let render_section = RenderSection {
                            section,
                            projects: Vec::new(),
                            news: vec![news.clone()],
                        };
                        render_sections.insert(map_section_name, render_section);
                    }
                }
            }
        }

        // News entry *does* have valid project information
        for news_project_name in news.project_names() {
            project_names.insert(news_project_name.clone());
            let project = config.project_by_name(&news_project_name).unwrap();
            let mut overwritten_section = false;

            // Handle news entries with sections which don't match the project default_section
            for section_name in news.section_names() {
                if section_name != project.default_section {
                    notes.insert(0, format!("[{}] News entry by {} gets added to the “{}” section, which is not the default section for this project.", message_link, news.reporter_display_name, section_name));
                    overwritten_section = true;

                    let custom_project_section_name =
                        format!("{}-{}", news_project_name, section_name);

                    match render_projects.get_mut(&custom_project_section_name) {
                        // RenderProject already exists -> Add news entry to it
                        Some(render_project) => render_project.news.insert(0, news.clone()),
                        // RenderProject doesn't exist yet -> Create it, and add news entry to it
                        None => {
                            let render_project = RenderProject {
                                project: project.clone(),
                                news: vec![news.clone()],
                                overwritten_section: Some(section_name),
                            };
                            render_projects
                                .insert(custom_project_section_name.clone(), render_project);
                        }
                    }
                }
            }

            if overwritten_section {
                continue;
            }

            // Standard (news entry doesn't use a custom section)
            match render_projects.get_mut(&news_project_name) {
                // RenderProject already exists -> Add news entry to it
                Some(render_project) => render_project.news.insert(0, news.clone()),
                // RenderProject doesn't exist yet -> Create it, and add news entry to it
                None => {
                    let render_project = RenderProject {
                        project,
                        news: vec![news.clone()],
                        overwritten_section: None,
                    };
                    render_projects.insert(news_project_name, render_project);
                }
            }
        }
    }

    // Sort `RenderProject`s into `RenderSection`s
    for (_, render_project) in render_projects {
        let section_name = if let Some(section_name) = &render_project.overwritten_section {
            section_name.clone()
        } else {
            render_project.project.default_section.clone()
        };

        let section = config.section_by_name(&section_name).unwrap();
        let map_section_name = format!("{}-{}", section.order, section_name);

        match render_sections.get_mut(&map_section_name) {
            // RenderSection already exists -> default_section Add render_project entry to it
            Some(render_section) => {
                render_section.projects.insert(0, render_project);
            }
            // RenderSection doesn't exist yet -> Create it, and add render_project entry to it
            None => {
                let render_section = RenderSection {
                    section,
                    projects: vec![render_project],
                    news: Vec::new(),
                };
                render_sections.insert(map_section_name, render_section);
            }
        }
    }

    // Sort sections
    let mut sorted_render_sections: BTreeMap<Section, RenderSection> = BTreeMap::new();
    for render_section in render_sections.values() {
        sorted_render_sections.insert(render_section.section.clone(), render_section.clone());
    }

    // Create summary notes for the admin room
    if not_assigned != 0 {
        let note = format!(
            "{} news are not included because of project/section assignment is missing. Use !status command to list them.",
            not_assigned
        );
        warnings.insert(0, note);
    }

    let summary = format!(
        "Rendered markdown is including {} news, {} image(s) and {} video(s)!",
        news_count,
        images.len(),
        videos.len(),
    );
    notes.insert(0, summary);

    // Rerverse order to make it more easy to read
    warnings.reverse();
    notes.reverse();

    let rendered = render_template(&render_sections, &config, editor).unwrap();

    RenderResult {
        rendered,
        warnings,
        notes,
        images,
        videos,
    }
}

fn message_link(config: &Config, event_id: &EventId) -> String {
    let room_id = config.reporting_room_id.clone();
    format!(
        "<a href=\"https://matrix.to/#/{}/{}\">open message</a>",
        room_id, event_id
    )
}
