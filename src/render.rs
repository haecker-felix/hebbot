use matrix_sdk::RoomMember;
use regex::Regex;
use ruma::UserId;

use std::env;
use std::fs::File;
use std::io::Read;

use crate::store::News;
use crate::utils;

pub fn render(news: Vec<News>, editor: &RoomMember, bot: &UserId) -> String {
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
        if n.approvals.is_empty() {
            continue;
        }

        let section = "Section header (not implemented yet)";
        let user = format!(
            "[{}](https://matrix.to/#/{})",
            n.reporter_display_name, n.reporter_id
        );

        let message = prepare_message(n.message, bot);

        let section = format!(
            "# {}\n\
            {} reports that\n\n\
            {}\n\n",
            section, user, message
        );

        report = (report + &section).to_string();
    }

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
    template = template.replace("{{report}}", &report);

    template
}

fn prepare_message(msg: String, bot: &UserId) -> String {
    let msg = msg.trim();

    // remove bot user name
    let regex = format!("^@?{}(:{})?:?", bot.localpart(), bot.server_name());
    let re = Regex::new(&regex).unwrap();
    let msg = re.replace(&msg, "");
    let msg = msg.trim();

    // quote message
    let msg = format!("> {}", msg);
    let msg = msg.replace("\n", "\n> ");

    // lists
    msg.replace("> -", "> *")
}
