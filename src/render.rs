use std::env;
use std::fs::File;
use std::io::Read;

use crate::store::News;

pub fn render(news: Vec<News>, editor: String) -> String {
    let path = match env::var("TEMPLATE_PATH") {
        Ok(val) => val,
        Err(_) => "./template.md".to_string(),
    };

    let mut file = File::open(path).expect("Unable to open template file");
    let mut template = String::new();
    file.read_to_string(&mut template)
        .expect("Unable to read template file");

    let mut report = String::new();
    for n in news {
        // skip not approved news
        if !n.approved {
            continue;
        }

        let section = "Section header (not implemented yet)";
        let user = format!(
            "[{}](https://matrix.to/#/{})",
            n.reporter_display_name, n.reporter_id
        );

        let section = format!(
            "# {}\n\
            {} reports that\n\
            > {}\n\n",
            section, user, n.message
        );

        report = (report + &section).to_string();
    }

    let now: chrono::DateTime<chrono::Utc> = chrono::Utc::now();
    let today = now.format("%Y-%m-%d");

    template = template.replace("{{today}}", &today.to_string());
    template = template.replace("{{author}}", &editor);
    template = template.replace("{{report}}", &report);

    template
}
