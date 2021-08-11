use chrono::Datelike;
use matrix_sdk::RoomMember;
use rand::Rng;
use ruma::MxcUri;

use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs::File;
use std::io::Read;

use crate::{utils, Config, News, Project, Section};

#[derive(Debug, Default)]
struct RenderProject {
    pub project: Project,
    pub news: Vec<News>,

    // For news with overwritten section information (-> doesn't match project default_section)
    pub overwritten_section: Option<String>,
}

#[derive(Debug, Default)]
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
    pub images: Vec<(String, MxcUri)>,
    pub videos: Vec<(String, MxcUri)>,
}

pub fn render(news_list: Vec<News>, config: Config, editor: &RoomMember) -> RenderResult {
    let mut render_projects: BTreeMap<String, RenderProject> = BTreeMap::new();
    let mut render_sections: BTreeMap<String, RenderSection> = BTreeMap::new();

    let news_count = news_list.len();
    let mut not_approved = 0;
    let mut report_text = String::new();
    let mut project_names: HashSet<String> = HashSet::new();

    let mut images: Vec<(String, MxcUri)> = Vec::new();
    let mut videos: Vec<(String, MxcUri)> = Vec::new();

    let mut warnings: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    // Sort news entries into `RenderProject`s (`render_projects`)
    for news in news_list {
        let message_link = message_link(&config, &news.event_id);

        // Skip news entries which are not approved
        if !news.is_approved() {
            not_approved += 1;
            continue;
        }

        // Check if the news entry has multiple project/information set
        if news.project_names().len() > 1 || news.section_names().len() > 1 {
            warnings.insert(0, format!("[{}] News entry by {} has multiple project or section information set, it'll appear multiple times. This is probably not wanted!", message_link, news.reporter_display_name));
        }

        // Check if the news entry has at one project or section information added
        if news.project_names().is_empty() && news.section_names().is_empty() {
            warnings.insert(0, format!("[{}] News entry by {} doesn't have project/section information, it'll not appear in the rendered markdown!", message_link, news.reporter_display_name));
            continue;
        }

        // Get news images / videos
        images.append(&mut news.images().clone());
        videos.append(&mut news.videos().clone());

        // Add news entries without any project information (but with section information) directly to the specified `RenderSection`
        if news.project_names().is_empty() {
            notes.insert(0, format!("[{}] News entry by {} doesn't have project information, it'll appear directly in the section without any project description.", message_link, news.reporter_display_name));

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
                    notes.insert(0, format!("[{}] News entry by {} gets added to the \"{}\" section, which is not the default section for this project.", message_link, news.reporter_display_name, section_name));
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
        let section_name = if let Some(ref section_name) = render_project.overwritten_section {
            section_name.clone()
        } else {
            render_project.project.default_section.clone()
        };

        let section = config.section_by_name(&section_name).unwrap();
        let map_section_name = format!("{}-{}", section.order, section_name);

        match render_sections.get_mut(&map_section_name) {
            // RenderSection already exists -> default_sectionAdd render_project entry to it
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

    // Do the actual markdown rendering
    for (_, render_section) in render_sections {
        let md_section = format!("# {}\n\n", render_section.section.title);
        report_text += &md_section;

        // First add news without project information
        for news in render_section.news {
            report_text += &news_md(&news, &config);
        }

        // Then add projects
        for render_project in render_section.projects {
            let project = render_project.project;
            let project_repo = format!("[{}]({})", project.title, project.website);
            let project_text = project.description.replace("{{project}}", &project_repo);

            let project_md = format!(
                "### {} [↗]({})\n\n\
                {}\n\n",
                project.title, project.website, project_text
            );
            report_text += &project_md;

            for news in render_project.news {
                report_text += &news_md(&news, &config);
            }
        }
    }

    // Create summary notes
    if not_approved != 0 {
        let note = format!(
            "{} news are not included because of missing approval. Use !status command to list them.",
            not_approved
        );
        notes.insert(0, note);
    }

    let summary = format!(
        "Rendered markdown is including {} news, {} image(s) and {} video(s)!",
        news_count,
        images.len(),
        videos.len(),
    );
    notes.insert(0, summary);

    // Load template file
    let path = match env::var("TEMPLATE_PATH") {
        Ok(val) => val,
        Err(_) => "./template.md".to_string(),
    };

    let mut file = File::open(path).expect("Unable to open template file");
    let mut template = String::new();
    file.read_to_string(&mut template)
        .expect("Unable to read template file");

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
    projects = projects.replace("{", "");
    projects = projects.replace("}", "");

    template = template.replace("{{weeknumber}}", &weeknumber);
    template = template.replace("{{timespan}}", &timespan);
    template = template.replace("{{projects}}", &projects);
    template = template.replace("{{today}}", &today);
    template = template.replace("{{author}}", &display_name);
    template = template.replace("{{report}}", &report_text.trim());

    // Rerverse order to make it more easy to read
    warnings.reverse();
    notes.reverse();

    RenderResult {
        rendered: template,
        warnings,
        notes,
        images,
        videos,
    }
}

fn news_md(news: &News, config: &Config) -> String {
    let user = format!(
        "[{}](https://matrix.to/#/{})",
        news.reporter_display_name, news.reporter_id
    );

    let verb = random_verb();
    let message = prepare_message(news.message());

    let mut news_md = format!(
        "{} {}\n\n\
        {}\n",
        user, verb, message
    );

    // Insert images/videos into markdown > quote
    for (filename, _) in news.images() {
        let image = config.image_markdown.replace("{{file}}", &filename);
        news_md += &(image.clone() + "\n");
    }
    for (filename, _) in news.videos() {
        let video = config.video_markdown.replace("{{file}}", &filename);
        news_md += &(video.clone() + "\n");
    }

    news_md += "\n";
    news_md
}

fn prepare_message(msg: String) -> String {
    let msg = msg.trim();

    // quote message
    let msg = format!("> {}", msg);
    let msg = msg.replace("\n", "\n> ");

    // lists
    msg.replace("> -", "> *")
}

fn random_verb() -> String {
    let mut rng = rand::thread_rng();
    let verbs = vec!["reports", "says", "announces"];
    let id = rng.gen_range(0..verbs.len());
    verbs[id].to_string()
}

fn message_link(config: &Config, event_id: &str) -> String {
    let room_id = config.reporting_room_id.clone();
    format!(
        "<a href=\"https://matrix.to/#/{}/{}\">open message</a>",
        room_id, event_id
    )
}
