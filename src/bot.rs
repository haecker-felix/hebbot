use matrix_sdk::room::Joined;
use matrix_sdk::room::Room;
use matrix_sdk::uuid::Uuid;
use matrix_sdk::Client;
use matrix_sdk::EventHandler;
use matrix_sdk::RoomMember;
use matrix_sdk::SyncSettings;
use ruma::events::reaction::ReactionEventContent;
use ruma::events::room::message::FileMessageEventContent;
use ruma::events::room::message::MessageType;
use ruma::events::room::redaction::SyncRedactionEvent;
use ruma::events::SyncMessageEvent;
use ruma::events::{room::message::MessageEventContent, AnyMessageEventContent};
use ruma::EventId;
use ruma::MxcUri;
use ruma::RoomId;
use ruma::UserId;

use std::convert::TryFrom;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;

use crate::render;
use crate::utils;
use crate::{Config, News, NewsStore};

#[derive(Clone)]
pub struct Bot {
    config: Config,
    news_store: Arc<Mutex<NewsStore>>,
    client: Client,
    reporting_room: Joined,
    admin_room: Joined,
}

impl Bot {
    pub async fn run() {
        let config = Config::read();
        let news_store = Arc::new(Mutex::new(NewsStore::read()));

        let username = config.bot_user_id.as_str();
        let user = UserId::try_from(username).expect("Unable to parse bot user id");
        let client = Client::new_from_user_id(user.clone()).await.unwrap();

        Self::login(&client, user.localpart(), &config.bot_password).await;

        // Get matrix rooms
        let reporting_room_id = RoomId::try_from(config.reporting_room_id.as_str()).unwrap();
        let reporting_room = client
            .get_joined_room(&reporting_room_id)
            .expect("Unable to get reporting room");

        let admin_room_id = RoomId::try_from(config.admin_room_id.as_str()).unwrap();
        let admin_room = client
            .get_joined_room(&admin_room_id)
            .expect("Unable to get admin room");

        let bot = Self {
            config,
            news_store,
            client,
            reporting_room,
            admin_room,
        };

        //bot.send_message("Started hebbot service!", true).await;

        // Setup event handler
        let handler = Box::new(EventCallback(bot.clone()));
        bot.client.set_event_handler(handler).await;

        info!("Start syncing...");
        bot.client.sync(SyncSettings::new()).await;
    }

    /// Login
    async fn login(client: &Client, user: &str, pwd: &str) {
        info!("Logging in...");
        let response = client
            .login(user, pwd, Some("hebbot"), Some("hebbot"))
            .await
            .expect("Unable to login");

        info!("Do initial sync...");
        client
            .sync_once(SyncSettings::new())
            .await
            .expect("Unable to sync");

        info!(
            "Logged in as {}, got device_id {}",
            response.user_id, response.device_id
        );
    }

    /// Simplified method for sending a matrix text/html message
    async fn send_message(&self, msg: &str, html: bool, admin_room: bool) {
        debug!(
            "Send message (html: {:?}, admin-room: {:?}): {}",
            html, admin_room, msg
        );

        let content = if html {
            AnyMessageEventContent::RoomMessage(MessageEventContent::text_html(msg, msg))
        } else {
            AnyMessageEventContent::RoomMessage(MessageEventContent::text_plain(msg))
        };
        let txn_id = Uuid::new_v4();

        let room = if admin_room {
            &self.admin_room
        } else {
            &self.reporting_room
        };

        room.send(content, Some(txn_id))
            .await
            .expect("Unable to send message");
    }

    /// Simplified method for sending a file
    async fn send_file(&self, url: MxcUri, filename: String, admin_room: bool) {
        debug!("Send file (url: {:?}, admin-room: {:?})", url, admin_room);

        let file_content = FileMessageEventContent::plain(filename, url, None);
        let msgtype = MessageType::File(file_content);
        let content = AnyMessageEventContent::RoomMessage(MessageEventContent::new(msgtype));
        let txn_id = Uuid::new_v4();

        let room = if admin_room {
            &self.admin_room
        } else {
            &self.reporting_room
        };

        room.send(content, Some(txn_id))
            .await
            .expect("Unable to send file");
    }
}

// Setup EventHandler to handle incoming matrix events
struct EventCallback(Bot);

#[async_trait::async_trait]
impl EventHandler for EventCallback {
    /// Handling room messages events
    async fn on_room_message(&self, room: Room, event: &SyncMessageEvent<MessageEventContent>) {
        if let Room::Joined(ref _joined) = room {
            // Standard text message
            if let Some(text) = utils::get_message_event_text(event) {
                let member = room.get_member(&event.sender).await.unwrap().unwrap();
                let id = &event.event_id;

                // Reporting room
                if room.room_id() == self.0.reporting_room.room_id() {
                    self.on_reporting_room_msg(text.clone(), &member, id).await;
                }

                // Admin room
                if room.room_id() == self.0.admin_room.room_id() {
                    self.on_admin_room_message(text, &member).await;
                }
            }

            // Message edit
            if let Some((edited_msg_event_id, text)) = utils::get_edited_message_event_text(event) {
                // Reporting room
                if room.room_id() == self.0.reporting_room.room_id() {
                    self.on_reporting_room_msg_edit(text.clone(), &edited_msg_event_id)
                        .await;
                }
            }
        }
    }

    /// Handling room reaction events
    async fn on_room_reaction(&self, room: Room, event: &SyncMessageEvent<ReactionEventContent>) {
        if let Room::Joined(ref _joined) = room {
            let reaction_sender = room.get_member(&event.sender).await.unwrap().unwrap();
            let relation = &event.content.relates_to;
            let reaction_event_id = event.event_id.clone();
            let message_event_id = relation.event_id.clone();

            // Remove emoji variant form
            let emoji = &relation.emoji.replace("\u{fe0f}", "");

            // Reporting room
            if room.room_id() == self.0.reporting_room.room_id() {
                self.on_reporting_room_reaction(
                    &reaction_sender,
                    &emoji,
                    &message_event_id,
                    &reaction_event_id,
                )
                .await;
            }
        }
    }

    /// Handling room redaction events (= something got removed/reverted)
    async fn on_room_redaction(&self, room: Room, event: &SyncRedactionEvent) {
        if let Room::Joined(ref _joined) = room {
            let redacted_event_id = event.redacts.clone();
            let member = room.get_member(&event.sender).await.unwrap().unwrap();

            // Reporting room
            if room.room_id() == self.0.reporting_room.room_id() {
                self.on_reporting_room_redaction(&member, &redacted_event_id)
                    .await;
            }
        }
    }
}

impl EventCallback {
    /// New message in reporting room
    /// - When the bot gets mentioned at the beginning of the message,
    ///   the message will get stored as News in NewsStore
    async fn on_reporting_room_msg(
        &self,
        message: String,
        member: &RoomMember,
        event_id: &EventId,
    ) {
        // We're going to ignore all messages, expect it mentions the bot at the beginning
        let bot_id = self.0.client.user_id().await.unwrap();
        if !utils::msg_starts_with_mention(bot_id, message.clone()) {
            return;
        }

        let event_id = event_id.to_string();
        let reporter_id = member.user_id().to_string();
        let reporter_display_name = utils::get_member_display_name(&member);
        let bot = self.0.client.user_id().await.unwrap();

        // Check min message length
        if message.len() > 30 {
            let msg = format!(
                "Thanks for the report {}, I'll store your update!",
                reporter_display_name
            );
            self.0.send_message(&msg, false, false).await;

            let link = self.message_link(event_id.to_string());
            let msg = format!("{} submitted a news entry. [{}]", member.user_id(), link);
            self.0.send_message(&msg, true, true).await;

            // remove bot name from message
            let message = utils::remove_bot_name(&message, &bot);

            // Create new news entry...
            let news = News {
                event_id,
                reporter_id,
                reporter_display_name,
                message,
                ..Default::default()
            };

            // ...and save it for the next report!
            self.0.news_store.lock().unwrap().add_news(news);
        } else {
            let msg = format!(
                "{}: Your update is too short and was not stored. This limitation was set-up to limit spam.",
                reporter_display_name
            );
            self.0.send_message(&msg, false, false).await;
        }
    }

    /// New message in reporting room
    /// - When the bot gets mentioned at the beginning of the message,
    ///   the message will get stored as News in NewsStore
    async fn on_reporting_room_msg_edit(
        &self,
        updated_message: String,
        edited_msg_event_id: &EventId,
    ) {
        let event_id = edited_msg_event_id.to_string();
        let bot = self.0.client.user_id().await.unwrap();
        let updated_message = utils::remove_bot_name(&updated_message, &bot);

        let message = if let Ok(news) = self
            .0
            .news_store
            .lock()
            .unwrap()
            .update_news(event_id, updated_message)
        {
            if !news.approvals.is_empty() {
                let link = self.message_link(edited_msg_event_id.to_string());
                let msg = format!(
                    "The news entry by {} got edited ({}). Check the new text, and make sure if you want to keep the approval.",
                    news.reporter_id,
                    link
                );
                Some(msg)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(message) = message {
            self.0.send_message(&message, true, true).await;
        }
    }

    /// New emoji reaction in reporting room
    /// - Only reactions from editors are processed
    /// - "approval emoji" -> approve a news entry
    /// - "section emoji" -> add a news entry to a section (eg. "Interesting Projects")
    /// - "project emoji" -> add a project description to a news entry
    async fn on_reporting_room_reaction(
        &self,
        reaction_sender: &RoomMember,
        reaction_emoji: &str,
        message_event_id: &EventId,
        reaction_event_id: &EventId,
    ) {
        // Check if the sender is a editor (= has the permission to use emoji "commands")
        if !self.is_editor(&reaction_sender).await {
            return;
        }

        let message_event_id = message_event_id.to_string();
        let reaction_event_id = reaction_event_id.to_string();
        let approval_emoji = &self.0.config.approval_emoji;
        let link = self.message_link(message_event_id.clone());

        // Approval emoji
        let message = if utils::emoji_cmp(reaction_emoji, approval_emoji) {
            let mut news_store = self.0.news_store.lock().unwrap();
            let msg = match news_store.add_news_approval(
                &message_event_id,
                &reaction_event_id,
                reaction_emoji.into(),
            ) {
                Ok(news) => format!(
                    "Editor {} approved {}'s news entry. ({})",
                    reaction_sender.user_id().to_string(),
                    news.reporter_id,
                    link
                ),
                Err(err) => format!(
                    "Unable to add {}'s news approval ({}): {:?}\n(ID {})",
                    reaction_sender.user_id().to_string(),
                    link,
                    err,
                    message_event_id
                ),
            };
            Some(msg)

        // Section emoji
        } else if let Some(section) = &self.0.config.section_by_emoji(&reaction_emoji) {
            let mut news_store = self.0.news_store.lock().unwrap();
            let msg = match news_store.add_news_section(
                &message_event_id,
                &reaction_event_id,
                reaction_emoji.into(),
            ) {
                Ok(news) => format!(
                    "Editor {} added {}'s news entry ({}) to the \"{}\" section.",
                    reaction_sender.user_id().to_string(),
                    news.reporter_id,
                    link,
                    section.title
                ),
                Err(err) => format!(
                    "Unable to add {}'s news entry ({}) to the {} section: {:?}\n(ID {})",
                    reaction_sender.user_id().to_string(),
                    link,
                    section.title,
                    err,
                    message_event_id
                ),
            };
            Some(msg)

        // Project emoji
        } else if let Some(project) = &self.0.config.project_by_emoji(&reaction_emoji) {
            let mut news_store = self.0.news_store.lock().unwrap();
            let msg = match news_store.add_news_project(
                &message_event_id,
                &reaction_event_id,
                reaction_emoji.into(),
            ) {
                Ok(news) => format!(
                    "Editor {} added the project description \"{}\" to {}'s news entry ({}).",
                    reaction_sender.user_id().to_string(),
                    project.display_name,
                    news.reporter_id,
                    link
                ),
                Err(err) => format!(
                    "Unable to add project description \"{}\"  to {}'s news entry ({}): {:?}\n(ID {})",
                    project.display_name,
                    reaction_sender.user_id().to_string(),
                    link,
                    err,
                    message_event_id
                ),
            };
            Some(msg)
        } else {
            debug!(
                "Ignore emoji reaction, doesn't match any known emoji ({:?})",
                reaction_emoji
            );
            None
        };

        // Send confirm message to admin room
        if let Some(message) = message {
            self.0.send_message(&message, true, true).await;
        }
    }

    /// Something got redacted in reporting room
    /// - Only redaction from editors are processed
    /// - Undo any reaction emoji "command" (eg. unapproving a news entry)
    async fn on_reporting_room_redaction(&self, member: &RoomMember, redacted_event_id: &EventId) {
        dbg!(&redacted_event_id);

        // Check if the sender is a editor (= has the permission to use emoji commands)
        if !self.is_editor(&member).await {
            return;
        }

        let message = {
            let mut news_store = self.0.news_store.lock().unwrap();
            let redacted_event_id = redacted_event_id.to_string();
            let link = self.message_link(redacted_event_id.clone());

            // News entry itself
            if let Ok(news) = news_store.remove_news(&redacted_event_id) {
                Some(format!(
                    "{}'s news entry got removed by {}",
                    news.reporter_id,
                    member.user_id().to_string()
                ))
            // News approval
            } else if let Ok(news) = news_store.remove_news_approval(&redacted_event_id) {
                let mut msg = format!(
                    "Editor {} removed their approval from {}'s news entry ({}).",
                    member.user_id().to_string(),
                    news.reporter_id,
                    link
                );

                if news.approvals.is_empty() {
                    msg += " This news entry doesn't have an approval anymore."
                }

                Some(msg)

            // News section
            } else if let Ok(news) = news_store.remove_news_section(&redacted_event_id) {
                Some(format!(
                    "Editor {} removed a section from {}'s news entry ({}).",
                    member.user_id().to_string(),
                    news.reporter_id,
                    link
                ))

            // News project
            } else if let Ok(news) = news_store.remove_news_project(&redacted_event_id) {
                Some(format!(
                    "Editor {} removed a project from {}'s news entry ({}).",
                    member.user_id().to_string(),
                    news.reporter_id,
                    link
                ))
            } else {
                debug!(
                    "Ignore redaction, doesn't match any known emoji reaction event id (ID {:?})",
                    redacted_event_id
                );
                None
            }
        };

        // Send confirm message to admin room
        if let Some(message) = message {
            self.0.send_message(&message, true, true).await;
        }
    }

    /// New message in admin room
    /// This is just for administrative stuff (eg. commands)
    async fn on_admin_room_message(&self, msg: String, member: &RoomMember) {
        // Check if the message is a command
        if !msg.as_str().starts_with('!') {
            return;
        }

        // Check if the sender is a editor (= has the permission to use commands)
        if !self.is_editor(&member).await {
            let msg = "You don't have the permission to use commands.";
            self.0.send_message(msg, false, true).await;
            return;
        }

        // Parse command and optional args
        let mut split: Vec<&str> = msg.splitn(2, ' ').collect();
        let args = if split.len() == 2 {
            split.pop().unwrap()
        } else {
            ""
        };
        let command = split.pop().unwrap_or("");

        info!("Received command: {} ({})", command, args);

        match command {
            "!clear" => self.clear_command().await,
            "!help" => self.help_command().await,
            "!list-config" => self.list_config_command().await,
            "!list-projects" => self.list_projects_command().await,
            "!list-sections" => self.list_sections_command().await,
            "!render-file" => self.render_file_command(member).await,
            "!render-message" => self.render_message_command(member).await,
            "!restart" => self.restart_command().await,
            "!say" => self.say_command(&args).await,
            "!status" => self.status_command().await,
            _ => self.unrecognized_command().await,
        }
    }

    async fn help_command(&self) {
        let help = "Available commands: \n\n\
            !clear \n\
            !list-config \n\
            !list-projects \n\
            !list-sections \n\
            !render-file \n\
            !render-message \n\
            !restart \n\
            !say <message>  \n\
            !status";

        self.0.send_message(help, false, true).await;
    }

    async fn clear_command(&self) {
        let msg = {
            let mut news_store = self.0.news_store.lock().unwrap();

            let news = news_store.get_news();
            news_store.clear_news();

            format!("Cleared {} news!", news.len())
        };

        self.0.send_message(&msg, false, true).await;
    }

    async fn list_config_command(&self) {
        let mut config = self.0.config.clone();

        // Don't print bot password
        config.bot_password = "".to_string();

        let json = serde_json::to_string_pretty(&config).unwrap();

        let msg = format!("<pre><code>{}</code></pre>\n", json);
        self.0.send_message(&msg, true, true).await;
    }

    async fn list_projects_command(&self) {
        let config = self.0.config.clone();

        let mut list = String::new();
        for e in config.projects {
            list += &format!(
                "{}: {} - {} ({})\n",
                e.emoji, e.display_name, e.description, e.repository
            );
        }

        let msg = format!("List of projects:\n<pre><code>{}</code></pre>\n", list);
        self.0.send_message(&msg, true, true).await;
    }

    async fn list_sections_command(&self) {
        let config = self.0.config.clone();

        let mut list = String::new();
        for e in config.sections {
            list += &format!("{}: {}\n", e.emoji, e.title);
        }

        let msg = format!("List of sections:\n<pre><code>{}</code></pre>\n", list);
        self.0.send_message(&msg, true, true).await;
    }

    async fn render_file_command(&self, editor: &RoomMember) {
        let rendered = {
            let news_store = self.0.news_store.lock().unwrap();
            let news = news_store.get_news();
            let config = self.0.config.clone();

            render::render(news, config, editor)
        };
        let mut bytes = rendered.as_bytes();

        let response = self
            .0
            .client
            .upload(&mime::TEXT_PLAIN_UTF_8, &mut bytes)
            .await
            .expect("Can't upload rendered file.");

        self.0
            .send_file(response.content_uri, "rendered.md".to_string(), true)
            .await;
    }

    async fn render_message_command(&self, editor: &RoomMember) {
        let rendered = {
            let news_store = self.0.news_store.lock().unwrap();
            let news = news_store.get_news();
            let config = self.0.config.clone();

            let r = render::render(news, config, editor);

            format!("<pre><code>{}</code></pre>\n", r)
        };

        self.0.send_message(&rendered, true, true).await;
    }

    async fn restart_command(&self) {
        self.0
            .send_message("Restarting hebbot...", false, true)
            .await;
        Command::new("/proc/self/exe").exec();
    }

    async fn say_command(&self, msg: &str) {
        self.0.send_message(&msg, true, false).await;
    }

    async fn status_command(&self) {
        let msg = {
            let news_store = self.0.news_store.lock().unwrap();
            let news = news_store.get_news();

            let mut approved_count = 0;
            let mut unapproved_count = 0;
            let sum = news.len();
            let mut approved_list = String::new();
            let mut unapproved_list = String::new();

            for n in news {
                let link = self.message_link(n.event_id);
                let summary = utils::summary(&n.message);

                if !n.approvals.is_empty() {
                    approved_count += 1;
                    approved_list += &format!("- [{}] {}: {} <br>", link, n.reporter_id, summary);
                } else {
                    unapproved_count += 1;
                    unapproved_list += &format!("- [{}] {}: {} <br>", link, n.reporter_id, summary);
                }
            }

            format!(
                "{} news in total <br><br>\
                ✅ Approved news ({}): <br>{} <br>\
                ❌ Unapproved news ({}): <br>{}",
                sum, approved_count, approved_list, unapproved_count, unapproved_list
            )
        };

        self.0.send_message(&msg, true, true).await;
    }

    async fn unrecognized_command(&self) {
        let msg = "Unrecognized command. Use !help to list available commands.";
        self.0.send_message(msg, false, true).await;
    }

    async fn is_editor(&self, member: &RoomMember) -> bool {
        let user_id = member.user_id().to_string();
        self.0.config.editors.contains(&user_id)
    }

    fn message_link(&self, event_id: String) -> String {
        let room_id = self.0.config.reporting_room_id.clone();
        format!(
            "<a href=\"https://matrix.to/#/{}/{}\">open message</a>",
            room_id, event_id
        )
    }
}
