use chrono::{DateTime, Utc};
use matrix_sdk::room::RoomMember;
use matrix_sdk::ruma::{EventId, OwnedMxcUri, OwnedUserId};
use serde::{Deserialize, Serialize};

use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use crate::{utils, Config, News, Project, Section};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RenderNews {
    pub reporter_id: OwnedUserId,
    pub reporter_display_name: String,
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub images: Vec<(String, OwnedMxcUri)>,
    pub videos: Vec<(String, OwnedMxcUri)>,
}

impl From<News> for RenderNews {
    fn from(news: News) -> Self {
        RenderNews {
            reporter_id: news.reporter_id.clone(),
            reporter_display_name: news.reporter_display_name.clone(),
            timestamp: news.timestamp,
            message: news.message(),
            images: news.images(),
            videos: news.videos(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct RenderProject {
    pub project: Project,
    pub news: Vec<RenderNews>,

    // For news with overwritten section information (-> doesn't match project default_section)
    pub overwritten_section: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct RenderSection {
    pub section: Section,
    pub projects: Vec<RenderProject>,

    // For news without project information
    pub news: Vec<RenderNews>,
}

pub struct RenderResult {
    pub rendered: String,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
    pub images: Vec<(String, OwnedMxcUri)>,
    pub videos: Vec<(String, OwnedMxcUri)>,
}

fn template_filter_timedelta(
    timestamp: minijinja::value::ViaDeserialize<time::OffsetDateTime>,
    options: minijinja::value::Kwargs,
) -> Result<minijinja::Value, minijinja::Error> {
    let mut duration = time::Duration::seconds(0);
    if let Some(seconds) = options.get::<Option<f64>>("seconds")? {
        duration = duration.saturating_add(time::Duration::seconds_f64(seconds));
    }
    if let Some(minutes) = options.get::<Option<i64>>("minutes")? {
        duration = duration.saturating_add(time::Duration::minutes(minutes));
    }
    if let Some(hours) = options.get::<Option<i64>>("hours")? {
        duration = duration.saturating_add(time::Duration::hours(hours));
    }
    if let Some(days) = options.get::<Option<i64>>("days")? {
        duration = duration.saturating_add(time::Duration::days(days));
    }
    if let Some(weeks) = options.get::<Option<i64>>("weeks")? {
        duration = duration.saturating_add(time::Duration::days(weeks * 7));
    }
    if let Some(months) = options.get::<Option<i64>>("months")? {
        duration = duration.saturating_add(time::Duration::days(months * 30));
    }
    if let Some(years) = options.get::<Option<i64>>("years")? {
        duration = duration.saturating_add(time::Duration::days(years * 365));
    }
    options.assert_all_used()?;

    Ok(minijinja::Value::from_serialize(
        timestamp.saturating_add(duration),
    ))
}

static TEMPLATE_TEXT: LazyLock<String> = LazyLock::new(|| {
    use std::io::Read;

    let path = std::env::var("TEMPLATE_PATH").unwrap_or("template.md".into());
    debug!("Reading template from file path: {:?}", path);

    let mut text = String::new();
    std::fs::File::open(path)
        .expect("Unable to open template file")
        .read_to_string(&mut text)
        .expect("Unable to read template file");

    text
});

static JINJA_ENV: LazyLock<minijinja::Environment> = LazyLock::new(|| {
    let mut env = minijinja::Environment::new();
    minijinja_contrib::add_to_environment(&mut env);
    env.add_template("template", &TEMPLATE_TEXT).unwrap();
    env.add_filter("timedelta", template_filter_timedelta);
    env
});

pub fn render(
    news_list: Vec<News>,
    config: Config,
    editor: &RoomMember,
) -> Result<RenderResult, minijinja::Error> {
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
        images.append(&mut news.images());
        videos.append(&mut news.videos());

        // Add news entries without any project information (but with section information) directly to the specified `RenderSection`
        if news.project_names().is_empty() {
            notes.insert(0, format!("[{}] News entry by {} doesn’t have project information, it’ll appear directly in the section without any project description.", message_link, news.reporter_display_name));

            for section_name in news.section_names() {
                let section = config.section_by_name(&section_name).unwrap();
                let map_section_name = format!("{}-{}", section.order, section_name);

                match render_sections.get_mut(&map_section_name) {
                    // RenderSection already exists -> Add news entry to it
                    Some(render_section) => {
                        render_section.news.insert(0, news.clone().into());
                    }
                    // RenderSection doesn't exist yet -> Create it, and add news entry to it
                    None => {
                        let render_section = RenderSection {
                            section,
                            projects: Vec::new(),
                            news: vec![news.clone().into()],
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
                        Some(render_project) => render_project.news.insert(0, news.clone().into()),
                        // RenderProject doesn't exist yet -> Create it, and add news entry to it
                        None => {
                            let render_project = RenderProject {
                                project: project.clone(),
                                news: vec![news.clone().into()],
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
                Some(render_project) => render_project.news.insert(0, news.clone().into()),
                // RenderProject doesn't exist yet -> Create it, and add news entry to it
                None => {
                    let render_project = RenderProject {
                        project,
                        news: vec![news.clone().into()],
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

    let rendered = JINJA_ENV
        .get_template("template")?
        .render(minijinja::context! {
            timestamp => time::OffsetDateTime::now_utc(),
            sections => render_sections,
            projects => project_names,
            config => config,
            editor => utils::get_member_display_name(editor),
        })?;

    Ok(RenderResult {
        rendered,
        warnings,
        notes,
        images,
        videos,
    })
}

fn message_link(config: &Config, event_id: &EventId) -> String {
    let room_id = config.reporting_room_id.clone();
    format!(
        "<a href=\"https://matrix.to/#/{}/{}\">open message</a>",
        room_id, event_id
    )
}
