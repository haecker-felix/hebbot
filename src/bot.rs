use chrono::{DateTime, Utc};
use matrix_sdk::room::{Joined, Room};
use matrix_sdk::uuid::Uuid;
use matrix_sdk::{Client, EventHandler, RoomMember, SyncSettings};
use ruma::events::reaction::ReactionEventContent;
use ruma::events::room::message::{FileMessageEventContent, MessageEventContent, MessageType};
use ruma::events::room::redaction::SyncRedactionEvent;
use ruma::events::{AnyMessageEventContent, AnyRoomEvent, SyncMessageEvent};
use ruma::{EventId, MxcUri, RoomId, UserId};

use std::convert::TryFrom;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::{Arc, Mutex};

use crate::{render, utils, BotMessageType as BotMsgType, Config, News, NewsStore, ReactionType};

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
        let config_result = Config::read();
        let config = config_result.config;
        let news_store = Arc::new(Mutex::new(NewsStore::read()));

        let username = config.bot_user_id.as_str();
        let user = UserId::try_from(username).expect("Unable to parse bot user id");
        let client = Client::new_from_user_id(user.clone()).await.unwrap();

        Self::login(&client, user.localpart(), &config.bot_password).await;

        // Get matrix rooms IDs
        let reporting_room_id = RoomId::try_from(config.reporting_room_id.as_str()).unwrap();
        let admin_room_id = RoomId::try_from(config.admin_room_id.as_str()).unwrap();

        // Try to accept reporting room invite, if any
        if let Some(invited_room) = client.get_invited_room(&reporting_room_id) {
            invited_room
                .accept_invitation()
                .await
                .expect("Hebbot could not join the reporting room");
        }

        // Try to accept admin room invite, if any
        if let Some(invited_room) = client.get_invited_room(&admin_room_id) {
            invited_room
                .accept_invitation()
                .await
                .expect("Hebbot could not join the admin room");
        }

        // Sync to make sure that the bot is aware of the newly joined rooms
        client
            .sync_once(SyncSettings::new())
            .await
            .expect("Unable to sync");

        let reporting_room = client
            .get_joined_room(&reporting_room_id)
            .expect("Unable to get reporting room");

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

        bot.send_message("✅ Started hebbot!", BotMsgType::AdminRoomPlainNotice)
            .await;

        // Send warnings
        let warnings = utils::format_messages(true, &config_result.warnings);
        if !config_result.warnings.is_empty() {
            bot.send_message(&warnings, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }

        // Send notes
        let notes = utils::format_messages(false, &config_result.notes);
        if !config_result.notes.is_empty() {
            bot.send_message(&notes, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }

        // Setup event handler
        let handler = Box::new(EventCallback(bot.clone()));
        bot.client.set_event_handler(handler).await;

        info!("Started syncing…");
        bot.client.sync(SyncSettings::new()).await;
    }

    /// Login
    async fn login(client: &Client, user: &str, pwd: &str) {
        info!("Logging in…");
        let response = client
            .login(user, pwd, Some("hebbot"), Some("hebbot"))
            .await
            .expect("Unable to login");

        info!("Doing the initial sync…");
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
    async fn send_message(&self, msg: &str, msg_type: BotMsgType) {
        debug!("Send message ({:?}): {}", msg_type, msg);

        #[rustfmt::skip]
        let (room, content) = match msg_type{
            BotMsgType::AdminRoomHtmlNotice => (&self.admin_room, MessageEventContent::notice_html(msg, msg)),
            BotMsgType::AdminRoomHtmlText => (&self.admin_room, MessageEventContent::text_html(msg, msg)),
            BotMsgType::AdminRoomPlainText => (&self.admin_room, MessageEventContent::text_plain(msg)),
            BotMsgType::AdminRoomPlainNotice => (&self.admin_room, MessageEventContent::notice_plain(msg)),
            BotMsgType::ReportingRoomHtmlText => (&self.reporting_room, MessageEventContent::text_html(msg, msg)),
            BotMsgType::ReportingRoomPlainText => (&self.reporting_room, MessageEventContent::text_plain(msg)),
            BotMsgType::ReportingRoomHtmlNotice => (&self.reporting_room, MessageEventContent::notice_html(msg, msg)),
            BotMsgType::ReportingRoomPlainNotice => (&self.reporting_room, MessageEventContent::notice_plain(msg)),
        };

        let content = AnyMessageEventContent::RoomMessage(content);
        let txn_id = Uuid::new_v4();

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
            let reaction_event_id = event.event_id.clone();
            let relation = &event.content.relates_to;
            let related_event_id = relation.event_id.clone();
            let emoji = &relation.emoji.replace("\u{fe0f}", "");

            if let Some(related_event) = utils::room_event_by_id(&room, &related_event_id).await {
                if let Some(related_msg_type) = utils::message_type(&related_event).await {
                    // Reporting room
                    if room.room_id() == self.0.reporting_room.room_id() {
                        self.on_reporting_room_reaction(
                            &reaction_sender,
                            &emoji,
                            &reaction_event_id,
                            &related_event,
                            &related_msg_type,
                        )
                        .await;
                    }
                } else {
                    debug!(
                        "Reaction related message isn't a room message (id {})",
                        related_event_id.to_string()
                    );
                }
            } else {
                warn!(
                    "Couldn't get reaction related event (id {})",
                    related_event_id.to_string()
                );
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
                "✅ Thanks for the report {}, I'll store your update!",
                reporter_display_name
            );
            self.0
                .send_message(&msg, BotMsgType::ReportingRoomPlainNotice)
                .await;

            let link = self.message_link(event_id.to_string());
            let msg = format!("✅ {} submitted a news entry. [{}]", member.user_id(), link);
            self.0
                .send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
                .await;

            // remove bot name from message
            let message = utils::remove_bot_name(&message, &bot);

            // Create new news entry...
            let news = News::new(event_id, reporter_id, reporter_display_name, message);

            // ...and save it for the next report!
            self.0.news_store.lock().unwrap().add_news(news);
        } else {
            let msg = format!(
                "❌ {}: Your update is too short and was not stored. This limitation was set-up to limit spam.",
                reporter_display_name
            );
            self.0
                .send_message(&msg, BotMsgType::ReportingRoomPlainNotice)
                .await;
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
        let link = self.message_link(edited_msg_event_id.to_string());

        let message = {
            let news_store = self.0.news_store.lock().unwrap();
            let msg = if let Some(news) = news_store.news_by_message_id(&event_id) {
                news.set_message(updated_message);
                if news.is_approved() {
                    Some(format!(
                        "✅ The news entry by {} got edited ({}). Check the new text, and make sure you want to keep the approval.",
                        news.reporter_id,
                        link
                    ))
                } else {
                    None
                }
            } else {
                None
            };
            news_store.write_data();
            msg
        };

        if let Some(message) = message {
            self.0
                .send_message(&message, BotMsgType::AdminRoomHtmlNotice)
                .await;
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
        reaction_event_id: &EventId,
        related_event: &AnyRoomEvent,
        related_message_type: &MessageType,
    ) {
        // Check if the sender is a editor (= has the permission to use emoji "commands")
        if !self.is_editor(&reaction_sender).await {
            return;
        }

        let message: Option<String> = {
            let news_store = self.0.news_store.lock().unwrap();

            let reaction_event_id = reaction_event_id.to_string();
            let reaction_type = self.0.config.reaction_type_by_emoji(&reaction_emoji);
            let related_event_id = related_event.event_id().to_string();
            let related_event_timestamp: DateTime<Utc> = related_event
                .origin_server_ts()
                .to_system_time()
                .unwrap()
                .into();
            let link = self.message_link(related_event_id.clone());

            if reaction_type == ReactionType::None {
                debug!(
                    "Ignoring emoji reaction, doesn't match any known emoji ({:?})",
                    reaction_emoji
                );
                return;
            }

            let msg = match related_message_type {
                MessageType::Text(_) => {
                    let msg = if let Some(news) = news_store.news_by_message_id(&related_event_id) {
                        match reaction_type {
                            ReactionType::Approval => {
                                news.add_approval(reaction_event_id);
                                Some(format!(
                                    "✅ Editor {} approved {}'s news entry. [{}]",
                                    reaction_sender.user_id().to_string(),
                                    news.reporter_id,
                                    link
                                ))
                            }
                            ReactionType::Section(section) => {
                                let section = section.unwrap();
                                news.add_section_name(reaction_event_id, section.name);
                                Some(format!(
                                    "✅ Editor {} added {}’s news entry [{}] to the “{}” section.",
                                    reaction_sender.user_id().to_string(),
                                    news.reporter_id,
                                    link,
                                    section.title
                                ))
                            }
                            ReactionType::Project(project) => {
                                let project = project.unwrap();
                                news.add_project_name(reaction_event_id, project.name);
                                Some(format!(
                                    "✅ Editor {} added the project description “{}” to {}’s news entry [{}].",
                                    reaction_sender.user_id().to_string(),
                                    project.title,
                                    news.reporter_id,
                                    link
                                ))
                            }
                            ReactionType::Image => {
                                Some(format!(
                                    "❌ It’s not possible to save {}’s news entry as image (only image messages are supported) [{}].",
                                    news.reporter_id,
                                    link
                                ))
                            }
                            ReactionType::Video => {
                                Some(format!(
                                    "❌ It’s not possible to save {}’s news entry as video (only video messages are supported) [{}].",
                                    news.reporter_id,
                                    link
                                ))
                            }
                            _ => None,
                        }
                    } else {
                        Some(format!(
                            "❌ Unable to process {}’s {} reaction, message doesn’t exist or isn’t a news submission [{}]\n(ID {})",
                            reaction_sender.user_id().to_string(),
                            reaction_type,
                            link,
                            related_event_id
                        ))
                    };
                    msg
                }
                MessageType::Image(image) => match reaction_type {
                    ReactionType::Image => {
                        let reporter_id = reaction_sender.user_id().to_string();
                        if let Some(news) = news_store.find_related_news(
                            &related_event.sender().to_string(),
                            &related_event_timestamp,
                        ) {
                            if let Some(mxc_uri) = &image.url {
                                news.add_image(
                                    reaction_event_id,
                                    image.body.clone(),
                                    mxc_uri.clone(),
                                );
                                Some(format!(
                                    "✅ Added image to {}’s news entry (“{}”) [{}].",
                                    news.reporter_id,
                                    news.message_summary(),
                                    link
                                ))
                            } else {
                                None
                            }
                        } else {
                            Some(format!(
                                "❌ Unable to save {}’s image, no matching news entry found ({}).",
                                reporter_id, link
                            ))
                        }
                    }
                    _ => Some(format!(
                        "❌ Invalid reaction emoji {} by {} for message type image [{}].",
                        reaction_emoji,
                        reaction_sender.user_id().to_string(),
                        link
                    )),
                },
                MessageType::Video(video) => match reaction_type {
                    ReactionType::Video => {
                        let reporter_id = reaction_sender.user_id().to_string();
                        if let Some(news) = news_store.find_related_news(
                            &related_event.sender().to_string(),
                            &related_event_timestamp,
                        ) {
                            if let Some(mxc_uri) = &video.url {
                                news.add_video(
                                    reaction_event_id,
                                    video.body.clone(),
                                    mxc_uri.clone(),
                                );
                                Some(format!(
                                    "✅ Added video to {}’s news entry (“{}”) [{}].",
                                    news.reporter_id,
                                    news.message_summary(),
                                    link
                                ))
                            } else {
                                None
                            }
                        } else {
                            Some(format!(
                                "❌ Unable to save {}’s video, no matching news entry found ({}).",
                                reporter_id, link
                            ))
                        }
                    }
                    _ => Some(format!(
                        "❌ Invalid reaction emoji by {} for message type video [{}].",
                        reaction_sender.user_id().to_string(),
                        link
                    )),
                },
                _ => {
                    debug!(
                        "Unsupported message type {:?} (id {}",
                        related_message_type, related_event_id
                    );
                    None
                }
            };
            news_store.write_data();
            msg
        };

        // Send confirm message to admin room
        if let Some(message) = message {
            self.0
                .send_message(&message, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }
    }

    /// Something got redacted in reporting room
    /// - Undo any reaction emoji "command" (eg. unapproving a news entry)
    /// - Or a message itself got deleted / redacted
    async fn on_reporting_room_redaction(&self, member: &RoomMember, redacted_event_id: &EventId) {
        let message = {
            let is_editor = self.is_editor(&member).await;
            let mut news_store = self.0.news_store.lock().unwrap();
            let redacted_event_id = redacted_event_id.to_string();
            let link = self.message_link(redacted_event_id.clone());

            // Redaction / deletion of the news entry itself
            let msg = if let Ok(news) = news_store.remove_news(&redacted_event_id) {
                Some(format!(
                    "✅ {}’s news entry got deleted by {}",
                    news.reporter_id,
                    member.user_id().to_string()
                ))
            // For all other redactions, there is no point in checking them if the member is not an editor.
            } else if !is_editor {
                None
            // Redaction of reaction events (approval, project, section)
            } else if let Some(news) = news_store.news_by_reaction_id(&redacted_event_id.clone()) {
                let reaction_type = news.remove_reaction_id(&redacted_event_id);
                if reaction_type != ReactionType::None {
                    Some(format!(
                        "✅ Editor {} removed {} from {}’s news entry ({}).",
                        member.user_id().to_string(),
                        reaction_type,
                        news.reporter_id,
                        link
                    ))
                } else {
                    debug!(
                        "Ignoring redaction, doesn’t match any known emoji reaction event id (ID {:?})",
                        redacted_event_id
                    );
                    None
                }
            } else {
                None
            };

            news_store.write_data();
            msg
        };

        // Send confirm message to admin room
        if let Some(message) = message {
            self.0
                .send_message(&message, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }
    }

    /// New message in admin room
    /// This is just for administrative stuff (eg. commands)
    async fn on_admin_room_message(&self, msg: String, member: &RoomMember) {
        let msg = msg.trim().to_string();

        // Check if the message is a command
        if !msg.as_str().starts_with('!') {
            return;
        }

        // Check if the sender is a editor (= has the permission to use commands)
        if !self.is_editor(&member).await {
            let msg = "You don’t have the permission to use commands.";
            self.0
                .send_message(msg, BotMsgType::AdminRoomPlainNotice)
                .await;
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
        let command = command.trim();

        info!("Received command: {} ({})", command, args);

        match command {
            "!about" => self.about_command().await,
            "!clear" => self.clear_command().await,
            "!details" => self.details_command(&args).await,
            "!help" => self.help_command().await,
            "!list-config" => self.list_config_command().await,
            "!list-projects" => self.list_projects_command().await,
            "!list-sections" => self.list_sections_command().await,
            "!render" => self.render_command(member).await,
            "!restart" => self.restart_command().await,
            "!say" => self.say_command(&args).await,
            "!status" => self.status_command().await,
            _ => self.unrecognized_command().await,
        }
    }

    async fn help_command(&self) {
        let help = "Available commands: \n\n\
            !about \n\
            !clear \n\
            !details <name> \n\
            !list-config \n\
            !list-projects \n\
            !list-sections \n\
            !render \n\
            !restart \n\
            !say <message> \n\
            !status";

        self.0
            .send_message(help, BotMsgType::AdminRoomPlainNotice)
            .await;
    }

    async fn about_command(&self) {
        let version = env!("CARGO_PKG_VERSION");

        let msg = format!(
            "You are running Hebbot version {}<br>© 2021 Felix Häcker<br><a href=\"https://github.com/haecker-felix/hebbot/\">Open Homepage</a> | <a href=\"https://github.com/haecker-felix/hebbot/issues/new\">Report Issue</a>",
            version
        );

        self.0
            .send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
    }

    async fn clear_command(&self) {
        let msg = {
            let mut news_store = self.0.news_store.lock().unwrap();

            let news = news_store.news();
            news_store.clear_news();

            format!("Cleared {} news entries!", news.len())
        };

        self.0
            .send_message(&msg, BotMsgType::AdminRoomPlainNotice)
            .await;
    }

    async fn details_command(&self, term: &str) {
        let result_project = self.0.config.project_by_name(term);
        let result_section = self.0.config.section_by_name(term);
        let result_reaction = self.0.config.reaction_type_by_emoji(term);

        let msg = if let Some(project) = result_project {
            project.html_details()
        } else if let Some(section) = result_section {
            section.html_details()
        } else {
            match result_reaction {
                ReactionType::Approval => format!("{} is configured as approval emoji.", term),
                ReactionType::Section(section) => section.unwrap().html_details(),
                ReactionType::Project(project) => project.unwrap().html_details(),
                ReactionType::Image => format!("{} is configured as image emoji.", term),
                ReactionType::Video => format!("{} is configured as video emoji.", term),
                ReactionType::None => format!("❌ Unable to find details for ”{}”.", term),
            }
        };

        self.0
            .send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
    }

    async fn list_config_command(&self) {
        let mut config = self.0.config.clone();

        // Don't print bot password
        config.bot_password = "".to_string();

        let toml = toml::to_string_pretty(&config).unwrap();

        let msg = format!("<pre><code>{}</code></pre>\n", toml);
        self.0
            .send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
    }

    async fn list_projects_command(&self) {
        let config = self.0.config.clone();

        let mut list = String::new();
        for e in config.projects {
            list += &format!(
                "{}: {} - {} ({})\n",
                e.emoji, e.title, e.description, e.website
            );
        }

        let msg = format!("List of projects:\n<pre><code>{}</code></pre>\n", list);
        self.0
            .send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
    }

    async fn list_sections_command(&self) {
        let config = self.0.config.clone();

        let mut list = String::new();
        for e in config.sections {
            list += &format!("{}: {}\n", e.emoji, e.title);
        }

        let msg = format!("List of sections:\n<pre><code>{}</code></pre>\n", list);
        self.0
            .send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
    }

    async fn render_command(&self, editor: &RoomMember) {
        let result = {
            let news_store = self.0.news_store.lock().unwrap();
            let news = news_store.news();
            let config = self.0.config.clone();

            render::render(news, config, editor)
        };

        // Upload rendered content as markdown file
        let mut bytes = result.rendered.as_bytes();
        let response = self
            .0
            .client
            .upload(&mime::TEXT_PLAIN_UTF_8, &mut bytes)
            .await
            .expect("Can't upload rendered file.");

        // Send file
        self.0
            .send_file(response.content_uri, "rendered.md".to_string(), true)
            .await;

        // Send warnings
        let warnings = utils::format_messages(true, &result.warnings);
        if !result.warnings.is_empty() {
            self.0
                .send_message(&warnings, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }

        // Send notes
        let notes = utils::format_messages(false, &result.notes);
        if !result.notes.is_empty() {
            self.0
                .send_message(&notes, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }

        // Generate a curl command which can get used to download all files (images/videos).
        let mut files = result.images.clone();
        files.append(&mut result.videos.clone());
        if !files.is_empty() {
            self.0
                .send_message(
                    "Use this command to download all files:",
                    BotMsgType::AdminRoomHtmlNotice,
                )
                .await;

            let mut curl_command = "curl".to_string();
            for (filename, uri) in &files {
                if uri.is_valid() {
                    let url = format!(
                        "{}_matrix/media/r0/download/{}/{}",
                        self.0.client.homeserver().await.to_string(),
                        uri.server_name().unwrap(),
                        uri.media_id().unwrap()
                    );

                    curl_command += &format!(" {} -o {}", url, filename);
                }
            }

            let msg = format!("<pre><code>{}</code></pre>\n", curl_command);
            self.0
                .send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }
    }

    async fn restart_command(&self) {
        self.0
            .send_message("Restarting hebbot…", BotMsgType::AdminRoomPlainNotice)
            .await;
        Command::new("/proc/self/exe").exec();
    }

    async fn say_command(&self, msg: &str) {
        self.0
            .send_message(&msg, BotMsgType::ReportingRoomPlainText)
            .await;
    }

    async fn status_command(&self) {
        let msg = {
            let news_store = self.0.news_store.lock().unwrap();
            let news = news_store.news();

            let mut approved_count = 0;
            let mut unapproved_count = 0;
            let sum = news.len();
            let mut approved_list = String::new();
            let mut unapproved_list = String::new();

            for n in &news {
                let link = self.message_link(n.event_id.clone());
                let summary = n.message_summary();

                if n.is_approved() {
                    approved_count += 1;
                    approved_list += &format!("- [{}] {}: {} <br>", link, n.reporter_id, summary);
                } else {
                    unapproved_count += 1;
                    unapproved_list += &format!("- [{}] {}: {} <br>", link, n.reporter_id, summary);
                }
            }

            format!(
                "{} news entries in total <br><br>\
                ✅ Approved news entries ({}): <br>{} <br>\
                ❌ Unapproved news entries ({}): <br>{}",
                sum, approved_count, approved_list, unapproved_count, unapproved_list
            )
        };

        self.0
            .send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
    }

    async fn unrecognized_command(&self) {
        let msg = "Unrecognized command. Use !help to list available commands.";
        self.0
            .send_message(msg, BotMsgType::AdminRoomPlainNotice)
            .await;
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
