use chrono::Datelike;
use matrix_sdk::room::RoomMember;
use matrix_sdk::ruma::{EventId, OwnedMxcUri};
use regex::Regex;

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;

use crate::{utils, Config, News, Project, Section};

#[derive(Clone, Debug, Default)]
struct RenderProject {
    pub project: Project,
    pub news: Vec<News>,

    // For news with overwritten section information (-> doesn't match project default_section)
    pub overwritten_section: Option<String>,
}

#[derive(Clone, Debug, Default)]
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

pub fn render(news_list: Vec<News>, config: Config, editor: &RoomMember) -> RenderResult {
    let mut render_projects: BTreeMap<String, RenderProject> = BTreeMap::new();
    let mut render_sections: BTreeMap<String, RenderSection> = BTreeMap::new();

    let mut news_count = 0;
    let mut not_assigned = 0;
    let mut rendered_report = String::new();
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

    // Do the actual markdown rendering
    for (_, render_section) in sorted_render_sections {
        let rendered_section = render_section_md(&render_section, &config);
        write!(rendered_report, "{}\n\n", rendered_section).unwrap();
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

    // Replace template variables with content
    let display_name = utils::get_member_display_name(editor);

    // Date for the blog
    let now: chrono::DateTime<chrono::Utc> = chrono::Utc::now();
    let today = now.format("%Y-%m-%d").to_string();
    let weeknumber = now.iso_week().week().to_string();

    // Generate timespan text
    let week_later = chrono::Utc::now() - chrono::Duration::days(7);
    let end = now.format("%B %d").to_string();
    let start = week_later.format("%B %d").to_string();
    let timespan = format!("{} to {}", start, end);

    // Projects list (can be get used for hugo tags for example)
    let mut projects = format!("{:?}", &project_names);
    projects = projects.replace('{', "");
    projects = projects.replace('}', "");

    // Load the section template
    let env_name = "REPORT_TEMPLATE_PATH";
    let fallback = "./report_template.md";
    let mut rendered = utils::file_from_env(env_name, fallback);

    // Replace the template variables with values
    rendered = rendered.replace("{{sections}}", rendered_report.trim());
    rendered = rendered.replace("{{weeknumber}}", &weeknumber);
    rendered = rendered.replace("{{timespan}}", &timespan);
    rendered = rendered.replace("{{projects}}", &projects);
    rendered = rendered.replace("{{today}}", &today);
    rendered = rendered.replace("{{author}}", &display_name);

    RenderResult {
        rendered,
        warnings,
        notes,
        images,
        videos,
    }
}

fn render_section_md(render_section: &RenderSection, config: &Config) -> String {
    // Load the section template
    let env_name = "SECTION_TEMPLATE_PATH";
    let fallback = "./section_template.md";
    let mut rendered_section = utils::file_from_env(env_name, fallback);

    // First iterate over news without project information
    let mut rendered_news = String::new();
    for news in &render_section.news {
        rendered_news += &render_news_md(news, config);
    }
    rendered_news = rendered_news.trim().to_string();

    // Then iterate over projects
    let mut rendered_projects = String::new();
    for render_project in &render_section.projects {
        rendered_projects += &("\n\n".to_owned() + &render_project_md(render_project, config));
    }
    let rendered_projects = rendered_projects.trim().to_string();

    // Replace the template variables with values
    let section = &render_section.section;
    rendered_section = rendered_section.replace("{{section.title}}", &section.title);
    rendered_section = rendered_section.replace("{{section.emoji}}", &section.emoji);
    rendered_section = rendered_section.replace("{{section.news}}", &rendered_news);
    rendered_section = rendered_section.replace("{{section.projects}}", &rendered_projects);

    rendered_section.trim().to_string()
}

fn render_project_md(render_project: &RenderProject, config: &Config) -> String {
    // Load the project template
    let env_name = "PROJECT_TEMPLATE_PATH";
    let fallback = "./project_template.md";
    let mut rendered_project = utils::file_from_env(env_name, fallback);

    // Iterate over project news items
    let mut rendered_news = String::new();
    for news in &render_project.news {
        rendered_news += &render_news_md(news, config);
    }
    rendered_news = rendered_news.trim().to_string();

    // Replace the template variables with values
    let project = &render_project.project;
    rendered_project = rendered_project.replace("{{project.title}}", &project.title);
    rendered_project = rendered_project.replace("{{project.emoji}}", &project.emoji);
    rendered_project = rendered_project.replace("{{project.website}}", &project.website);
    rendered_project = rendered_project.replace("{{project.description}}", &project.description);
    rendered_project = rendered_project.replace("{{project.news}}", &rendered_news);

    rendered_project.trim().to_string()
}

fn render_news_md(news: &News, config: &Config) -> String {
    let user = format!(
        "[{}](https://matrix.to/#/{})",
        news.reporter_display_name, news.reporter_id
    );

    let verb = &config.random_verb();
    let message = prepare_message(news.message());

    let mut news_md = format!(
        "{} {}\n\n\
        {}\n",
        user, verb, message
    );

    // Insert images/videos into markdown > quote, separating it from any elements before it
    for (filename, _) in news.images() {
        let image = config.image_markdown.replace("{{file}}", &filename);
        news_md += &("\n>".to_owned() + &image.clone() + "\n");
    }
    for (filename, _) in news.videos() {
        let video = config.video_markdown.replace("{{file}}", &filename);
        news_md += &("\n>".to_owned() + &video.clone() + "\n");
    }

    news_md += "\n";
    news_md
}

fn prepare_message(msg: String) -> String {
    let msg = msg.trim();

    // Turn matrix room aliases into matrix.to links
    let matrix_rooms_re =
        Regex::new("(^#([a-zA-Z0-9]|-|_)+:([a-zA-Z0-9]|-|_)+\\.([a-zA-Z0-9])+)").unwrap();
    let msg = matrix_rooms_re.replace_all(msg, "[$1](https://matrix.to/#/$1)");
    let matrix_rooms_re =
        Regex::new(" (#([a-zA-Z0-9]|-|_)+:([a-zA-Z0-9]|-|_)+\\.([a-zA-Z0-9])+)").unwrap();
    let msg = matrix_rooms_re.replace_all(&msg, " [$1](https://matrix.to/#/$1)");

    // Turn <del> tags into markdown strikethrough
    // NOTE: this does not work for nested tag, which shouldn't really happen in Matrix IM anyway
    let strikethrough_re = Regex::new("<del>(.+?)</del>").unwrap();
    let msg = strikethrough_re.replace_all(&msg, "~~$1~~");

    // quote message
    let msg = format!("> {}", msg);
    let msg = msg.replace('\n', "\n> ");

    // lists
    msg.replace("> -", "> *")
}

fn message_link(config: &Config, event_id: &EventId) -> String {
    let room_id = config.reporting_room_id.clone();
    format!(
        "<a href=\"https://matrix.to/#/{}/{}\">open message</a>",
        room_id, event_id
    )
}
