use matrix_sdk::RoomMember;
use rand::Rng;

use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::Read;

use crate::config::Config;
use crate::config::{Project, Section};
use crate::store::News;
use crate::utils;

pub fn render(news_list: Vec<News>, config: Config, editor: &RoomMember) -> String {
    let mut section_map: HashMap<Section, Vec<News>> = HashMap::new();

    // Sort news entries into sections
    for news in news_list {
        // skip not approved news
        if news.approvals.is_empty() {
            continue;
        }

        // Filter out duplicated sections
        // (eg. two editors are adding the same section to a news entry)
        let mut sections = HashSet::new();
        for section in news.sections.values().collect::<Vec<&String>>() {
            let section_emoji = section.replace("\u{fe0f}", "");
            sections.insert(section_emoji);
        }

        if sections.is_empty() {
            // For news entries without a section
            let todo_section = Section {
                title: "TODO".into(),
                emoji: "❔".into(),
            };
            insert_into_map(&mut section_map, &todo_section, news);
        } else {
            for section_emoji in sections {
                let section = config.section_by_emoji(&section_emoji).unwrap();
                insert_into_map(&mut section_map, &section, news.clone());
            }
        }
    }

    // Load template file
    let path = match env::var("TEMPLATE_PATH") {
        Ok(val) => val,
        Err(_) => "./template.md".to_string(),
    };

    let mut file = File::open(path).expect("Unable to open template file");
    let mut template = String::new();
    file.read_to_string(&mut template)
        .expect("Unable to read template file");

    // Generate actual report
    let mut report_text = String::new();
    for (section, news) in section_map {
        let mut section_text = format!("# {}\n", section.title);

        for n in news {
            // Filter out duplicated project
            // (eg. two editors are adding the same project description to a news entry)
            let mut projects = HashSet::new();
            for project in n.projects.values().collect::<Vec<&String>>() {
                let project_emoji = project.replace("\u{fe0f}", "");
                projects.insert(project_emoji);
            }

            if projects.is_empty() {
                // For news entries without a project
                let project = Project {
                    title: "TODO: Unknown project!".into(),
                    description: "This message was not annotated with a project description."
                        .into(),
                    ..Default::default()
                };
                let news_text = generate_news_text(&n, &project);
                section_text += &news_text;
            } else {
                for p in projects {
                    let project = config.project_by_emoji(&p).unwrap();
                    let news_text = generate_news_text(&n, &project);
                    section_text += &news_text;
                }
            }
        }

        report_text += &section_text;
    }
    let report_text = report_text.trim();

    // Editor user name / link
    let display_name = utils::get_member_display_name(editor);
    let author = format!(
        "[{}](https://matrix.to/#/{})",
        display_name,
        editor.user_id()
    );

    // Date for the blog
    let now: chrono::DateTime<chrono::Utc> = chrono::Utc::now();
    let today = now.format("%Y-%m-%d");

    template = template.replace("{{today}}", &today.to_string());
    template = template.replace("{{author}}", &author);
    template = template.replace("{{report}}", &report_text);

    template
}

fn insert_into_map(section_map: &mut HashMap<Section, Vec<News>>, section: &Section, news: News) {
    if let Some(entries) = section_map.get_mut(&section) {
        entries.insert(0, news);
    } else {
        let mut entries = Vec::new();
        entries.insert(0, news);
        section_map.insert(section.clone(), entries);
    }
}

fn generate_news_text(news: &News, project: &Project) -> String {
    let user = format!(
        "[{}](https://matrix.to/#/{})",
        news.reporter_display_name, news.reporter_id
    );

    let project_repo = format!("[{}]({})", project.title, project.repository);
    let project_text = project.description.replace("{{project}}", &project_repo);
    let verb = random_verb();
    let message = prepare_message(news.message.clone());

    let news_text = format!(
        "### {}\n\n\
        {}\n\n\
        {} {}\n\n\
        {}\n\n",
        project.title, project_text, user, verb, message
    );

    news_text
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
    let verbs = vec!["reports", "offers", "said", "announces", "reveals", "tells"];
    let id = rng.gen_range(0..verbs.len());
    verbs[id].to_string()
}
