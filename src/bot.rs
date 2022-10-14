use chrono::{DateTime, Utc};
use matrix_sdk::config::SyncSettings;
use matrix_sdk::event_handler::Ctx;
use matrix_sdk::room::{Joined, Room};
use matrix_sdk::ruma::events::reaction::{
    OriginalSyncReactionEvent, ReactionEventContent, Relation,
};
use matrix_sdk::ruma::events::room::message::{
    FileMessageEventContent, MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent,
};
use matrix_sdk::ruma::events::room::redaction::SyncRoomRedactionEvent;
use matrix_sdk::ruma::events::room::MediaSource;
use matrix_sdk::ruma::events::AnyRoomEvent;
use matrix_sdk::ruma::{EventId, OwnedMxcUri, RoomId, UserId};
use matrix_sdk::{Client, RoomMember};
use regex::Regex;

use std::env;
use std::fmt::Write;
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
        let password = env::var("BOT_PASSWORD").expect("BOT_PASSWORD env variable not specified");

        let user = UserId::parse(username).expect("Unable to parse bot user id");
        let client = Client::builder().user_id(&user).build().await.unwrap();

        Self::login(&client, user.localpart(), &password).await;

        // Get matrix rooms IDs
        let reporting_room_id = RoomId::parse(config.reporting_room_id.as_str()).unwrap();
        let admin_room_id = RoomId::parse(config.admin_room_id.as_str()).unwrap();

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

        // Setup event handlers
        bot.client
            .register_event_handler_context(bot.clone())
            .register_event_handler(Self::on_room_message)
            .await
            .register_event_handler(Self::on_room_reaction)
            .await
            .register_event_handler(Self::on_room_redaction)
            .await;

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
            BotMsgType::AdminRoomHtmlNotice => (&self.admin_room, RoomMessageEventContent::notice_html(msg, msg)),
            BotMsgType::AdminRoomHtmlText => (&self.admin_room, RoomMessageEventContent::text_html(msg, msg)),
            BotMsgType::AdminRoomPlainText => (&self.admin_room, RoomMessageEventContent::text_plain(msg)),
            BotMsgType::AdminRoomPlainNotice => (&self.admin_room, RoomMessageEventContent::notice_plain(msg)),
            BotMsgType::ReportingRoomHtmlText => (&self.reporting_room, RoomMessageEventContent::text_html(msg, msg)),
            BotMsgType::ReportingRoomPlainText => (&self.reporting_room, RoomMessageEventContent::text_plain(msg)),
            BotMsgType::ReportingRoomHtmlNotice => (&self.reporting_room, RoomMessageEventContent::notice_html(msg, msg)),
            BotMsgType::ReportingRoomPlainNotice => (&self.reporting_room, RoomMessageEventContent::notice_plain(msg)),
        };

        room.send(content, None)
            .await
            .expect("Unable to send message");
    }

    /// Simplified method for sending a reaction emoji
    async fn send_reaction(&self, reaction: &str, msg_event_id: &EventId) {
        let content =
            ReactionEventContent::new(Relation::new(msg_event_id.to_owned(), reaction.to_string()));

        if let Err(err) = self.reporting_room.send(content, None).await {
            warn!(
                "Could not send {} reaction to msg {}: {}",
                reaction,
                msg_event_id,
                err.to_string()
            );
        }
    }

    /// Simplified method for sending a file
    async fn send_file(&self, url: OwnedMxcUri, filename: String, admin_room: bool) {
        debug!("Send file (url: {:?}, admin-room: {:?})", url, admin_room);

        let file_content = FileMessageEventContent::plain(filename, url, None);
        let msgtype = MessageType::File(file_content);
        let content = RoomMessageEventContent::new(msgtype);

        let room = if admin_room {
            &self.admin_room
        } else {
            &self.reporting_room
        };

        room.send(content, None).await.expect("Unable to send file");
    }

    /// Handling room messages events
    async fn on_room_message(event: OriginalSyncRoomMessageEvent, room: Room, Ctx(bot): Ctx<Bot>) {
        if let Room::Joined(_joined) = &room {
            // Standard text message
            if let Some(text) = utils::get_message_event_text(&event) {
                let member = room.get_member(&event.sender).await.unwrap().unwrap();
                let id = &event.event_id;

                // Reporting room
                if room.room_id() == bot.reporting_room.room_id() {
                    bot.on_reporting_room_msg(text.clone(), &member, id).await;
                }

                // Admin room
                if room.room_id() == bot.admin_room.room_id() {
                    bot.on_admin_room_message(text, &member).await;
                }
            }

            // Message edit
            if let Some((edited_msg_event_id, text)) = utils::get_edited_message_event_text(&event)
            {
                // Reporting room
                if room.room_id() == bot.reporting_room.room_id() {
                    bot.on_reporting_room_msg_edit(text.clone(), &edited_msg_event_id)
                        .await;
                }
            }
        }
    }

    /// Handling room reaction events
    async fn on_room_reaction(event: OriginalSyncReactionEvent, room: Room, Ctx(bot): Ctx<Bot>) {
        if let Room::Joined(_joined) = &room {
            let reaction_sender = room.get_member(&event.sender).await.unwrap().unwrap();
            let reaction_event_id = event.event_id.clone();
            let relation = &event.content.relates_to;
            let related_event_id = relation.event_id.clone();
            let emoji = &relation.key.replace('\u{fe0f}', "");

            if let Some(related_event) = utils::room_event_by_id(&room, &related_event_id).await {
                if let Some(related_msg_type) = utils::message_type(&related_event).await {
                    // Reporting room
                    if room.room_id() == bot.reporting_room.room_id() {
                        bot.on_reporting_room_reaction(
                            &room,
                            &reaction_sender,
                            emoji,
                            &reaction_event_id,
                            &related_event,
                            &related_msg_type,
                        )
                        .await;
                    }
                } else {
                    debug!(
                        "Reaction related message isn't a room message (id {})",
                        related_event_id
                    );
                }
            } else {
                warn!(
                    "Couldn't get reaction related event (id {})",
                    related_event_id
                );
            }
        }
    }

    /// Handling room redaction events (= something got removed/reverted)
    async fn on_room_redaction(event: SyncRoomRedactionEvent, room: Room, Ctx(bot): Ctx<Bot>) {
        // FIXME: Function parameter should be OriginalSyncRoomRedactionEvent.
        // Doesn't currently compile, needs a fix in the matrix-rust-sdk.
        if let SyncRoomRedactionEvent::Original(event) = event {
            if let Room::Joined(_joined) = &room {
                let redacted_event_id = event.redacts.clone();
                let member = room.get_member(&event.sender).await.unwrap().unwrap();

                // Reporting room
                if room.room_id() == bot.reporting_room.room_id() {
                    bot.on_reporting_room_redaction(&member, &redacted_event_id)
                        .await;
                }
            }
        }
    }

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
        let bot_id = self.client.user_id().await.unwrap();
        if !utils::msg_starts_with_mention(&bot_id, message.clone()) {
            return;
        }

        let reporter_id = member.user_id();
        let reporter_display_name = utils::get_member_display_name(member);

        // Create new news entry...
        let news = News::new(
            event_id.to_owned(),
            reporter_id.to_owned(),
            reporter_display_name,
            message,
        );
        self.add_news(news, true).await;
    }

    /// New message in reporting room
    /// - When the bot gets mentioned at the beginning of the message,
    ///   the message will get stored as News in NewsStore
    async fn on_reporting_room_msg_edit(
        &self,
        updated_message: String,
        edited_msg_event_id: &EventId,
    ) {
        let bot = self.client.user_id().await.unwrap();
        let updated_message = utils::remove_bot_name(&updated_message, &bot);
        let link = self.message_link(edited_msg_event_id);

        let message = {
            let news_store = self.news_store.lock().unwrap();
            let msg = if let Some(news) = news_store.news_by_message_id(edited_msg_event_id) {
                news.set_message(updated_message);
                if news.is_assigned() {
                    Some(format!(
                        "✅ The news entry by {} got edited ({}). Check the new text, and make sure you want to keep the assigned project/section.",
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
            self.send_message(&message, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }
    }

    /// New emoji reaction in reporting room
    /// - Only reactions from editors are processed
    /// - "section emoji" -> add a news entry to a section (eg. "Interesting Projects")
    /// - "project emoji" -> add a project description to a news entry
    async fn on_reporting_room_reaction(
        &self,
        room: &Room,
        reaction_sender: &RoomMember,
        reaction_emoji: &str,
        reaction_event_id: &EventId,
        related_event: &AnyRoomEvent,
        related_message_type: &MessageType,
    ) {
        let reaction_emoji = reaction_emoji.strip_suffix('?').unwrap_or(reaction_emoji);

        // Only allow editors to use general commands
        // or the general public to use the notice emoji
        let sender_is_hebbot = reaction_sender.user_id().as_str() == self.config.bot_user_id;
        let sender_is_editor = self.is_editor(reaction_sender).await;
        if sender_is_hebbot
            || (self.config.restrict_notice
                && !sender_is_editor
                && !utils::emoji_cmp(reaction_emoji, &self.config.notice_emoji))
        {
            return;
        }

        let message: Option<String> = {
            let reaction_type = self.config.reaction_type_by_emoji(reaction_emoji);
            let related_event_id = related_event.event_id();
            let related_event_timestamp: DateTime<Utc> = related_event
                .origin_server_ts()
                .to_system_time()
                .unwrap()
                .into();
            let link = self.message_link(related_event_id);

            if reaction_type == ReactionType::None {
                debug!(
                    "Ignoring emoji reaction, doesn't match any known emoji ({:?})",
                    reaction_emoji
                );
                return;
            }

            let msg = match related_message_type {
                MessageType::Text(_) | MessageType::Notice(_) => {
                    // Check if the reaction == notice emoji,
                    // Yes -> Try to add the message as news submission
                    let msg = if utils::emoji_cmp(reaction_emoji, &self.config.notice_emoji) {
                        // we need related_event's sender
                        let related_event_sender = room
                            .get_member(related_event.sender())
                            .await
                            .unwrap()
                            .unwrap();

                        if !sender_is_editor
                            && (reaction_sender.user_id() != related_event_sender.user_id()
                                && self.config.restrict_notice)
                        {
                            return;
                        }

                        if let Some(news) =
                            utils::create_news_by_event(related_event, &related_event_sender)
                        {
                            self.add_news(news, false).await;
                            None
                        } else {
                            Some(format!(
                                "❌ Unable to add {}’s message as news, invalid event/message type [{}]",
                                related_event.sender(),
                                link,
                            ))
                        }

                    // Check if related message is a news entry
                    // (Adding the entry to a project / section by using the corresponding reaction emoji)
                    } else if let Some(news) = self
                        .news_store
                        .lock()
                        .unwrap()
                        .news_by_message_id(related_event_id)
                    {
                        match reaction_type {
                            ReactionType::Section(section) => {
                                let section = section.unwrap();
                                news.add_section_name(reaction_event_id.to_owned(), section.name);
                                Some(format!(
                                    "✅ Editor {} added {}’s news entry [{}] to the “{}” section.",
                                    reaction_sender.user_id(),
                                    news.reporter_id,
                                    link,
                                    section.title
                                ))
                            }
                            ReactionType::Project(project) => {
                                let project = project.unwrap();
                                news.add_project_name(reaction_event_id.to_owned(), project.name);
                                Some(format!(
                                    "✅ Editor {} added the project description “{}” to {}’s news entry [{}].",
                                    reaction_sender.user_id(),
                                    project.title,
                                    news.reporter_id,
                                    link
                                ))
                            }
                            _ => None,
                        }
                    } else {
                        Some(format!(
                            "❌ Unable to process {}’s {} reaction, message doesn’t exist or isn’t a news submission [{}]\n(ID {})",
                            reaction_sender.user_id(),
                            reaction_type,
                            link,
                            related_event_id
                        ))
                    };
                    msg
                }

                // Check if related message is an image
                MessageType::Image(image) => match reaction_type {
                    ReactionType::Notice => {
                        let reporter_id = reaction_sender.user_id();
                        let news_store = self.news_store.lock().unwrap();
                        if let Some(news) = news_store.find_related_news(
                            related_event.sender().as_ref(),
                            &related_event_timestamp,
                        ) {
                            if !sender_is_editor
                                && (reaction_sender.user_id() != related_event.sender()
                                    && self.config.restrict_notice)
                            {
                                return;
                            }
                            if let MediaSource::Plain(mxc_uri) = &image.source {
                                news.add_image(
                                    reaction_event_id.to_owned(),
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
                        reaction_sender.user_id(),
                        link
                    )),
                },

                // Check if related message is a video
                MessageType::Video(video) => match reaction_type {
                    ReactionType::Notice => {
                        let reporter_id = reaction_sender.user_id();
                        let news_store = self.news_store.lock().unwrap();
                        if let Some(news) = news_store.find_related_news(
                            related_event.sender().as_ref(),
                            &related_event_timestamp,
                        ) {
                            if let MediaSource::Plain(mxc_uri) = &video.source {
                                news.add_video(
                                    reaction_event_id.to_owned(),
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
                        reaction_sender.user_id(),
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
            msg
        };

        // Send confirm message to admin room
        if let Some(message) = message {
            self.send_message(&message, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }

        // Update stored news
        let news_store = self.news_store.lock().unwrap();
        news_store.write_data();
    }

    /// Something got redacted in reporting room
    /// - Undo any reaction emoji "command" (eg. removing a news entry from a section)
    /// - Or a message itself got deleted / redacted
    async fn on_reporting_room_redaction(&self, member: &RoomMember, redacted_event_id: &EventId) {
        let message = {
            let is_editor = self.is_editor(member).await;
            let mut news_store = self.news_store.lock().unwrap();
            let redacted_event_id = redacted_event_id;
            let link = self.message_link(redacted_event_id);

            // Redaction / deletion of the news entry itself
            let msg = if let Ok(news) = news_store.remove_news(redacted_event_id) {
                Some(format!(
                    "✅ {}’s news entry got deleted by {}",
                    news.reporter_id,
                    member.user_id()
                ))
            // For all other redactions, there is no point in checking them if the member is not an editor.
            } else if !is_editor {
                None
            // Redaction of reaction events (project / section)
            } else if let Some(news) = news_store.news_by_reaction_id(redacted_event_id) {
                let reaction_type = news.remove_reaction_id(redacted_event_id);
                if reaction_type != ReactionType::None {
                    Some(format!(
                        "✅ Editor {} removed {} from {}’s news entry ({}).",
                        member.user_id(),
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
            self.send_message(&message, BotMsgType::AdminRoomHtmlNotice)
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
        if !self.is_editor(member).await {
            let msg = "You don’t have the permission to use commands.";
            self.send_message(msg, BotMsgType::AdminRoomPlainNotice)
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
            "!details" => self.details_command(args).await,
            "!help" => self.help_command().await,
            "!list-config" => self.list_config_command().await,
            "!list-projects" => self.list_projects_command().await,
            "!list-sections" => self.list_sections_command().await,
            "!render" => self.render_command(member).await,
            "!restart" => self.restart_command().await,
            "!say" => self.say_command(args).await,
            "!status" => self.status_command().await,
            "!update-config" => self.update_config_command().await,
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
            !status \n\
            !update-config";

        self.send_message(help, BotMsgType::AdminRoomPlainNotice)
            .await;
    }

    async fn about_command(&self) {
        let version = env!("CARGO_PKG_VERSION");

        let msg = format!(
            "You are running Hebbot version {}<br>© 2021 Felix Häcker<br><a href=\"https://github.com/haecker-felix/hebbot/\">Open Homepage</a> | <a href=\"https://github.com/haecker-felix/hebbot/issues/new\">Report Issue</a>",
            version
        );

        self.send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
    }

    async fn clear_command(&self) {
        let msg = {
            let mut news_store = self.news_store.lock().unwrap();

            let news = news_store.news();
            news_store.clear_news();

            format!("Cleared {} news entries!", news.len())
        };

        self.send_message(&msg, BotMsgType::AdminRoomPlainNotice)
            .await;
    }

    async fn details_command(&self, term: &str) {
        let result_project = self.config.project_by_name(term);
        let result_section = self.config.section_by_name(term);
        let result_reaction = self.config.reaction_type_by_emoji(term);

        let msg = if let Some(project) = result_project {
            project.html_details()
        } else if let Some(section) = result_section {
            section.html_details()
        } else {
            match result_reaction {
                ReactionType::Section(section) => section.unwrap().html_details(),
                ReactionType::Project(project) => project.unwrap().html_details(),
                ReactionType::None => format!("❌ Unable to find details for ”{}”.", term),
                ReactionType::Notice => format!("{} is configured as notice emoji", term),
            }
        };

        self.send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
    }

    async fn list_config_command(&self) {
        let config = self.config.clone();
        let toml = toml::to_string_pretty(&config).unwrap();

        let msg = format!("<pre><code>{}</code></pre>\n", toml);
        self.send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
    }

    async fn list_projects_command(&self) {
        let config = self.config.clone();

        let mut list = String::new();
        for e in config.projects {
            writeln!(
                list,
                "{}: {} - {} ({})",
                e.emoji, e.title, e.description, e.website
            )
            .unwrap();
        }

        let msg = format!("List of projects:\n<pre><code>{}</code></pre>\n", list);
        self.send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
    }

    async fn list_sections_command(&self) {
        let config = self.config.clone();

        let mut list = String::new();
        for e in config.sections {
            writeln!(list, "{}: {}", e.emoji, e.title).unwrap();
        }

        let msg = format!("List of sections:\n<pre><code>{}</code></pre>\n", list);
        self.send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
    }

    async fn render_command(&self, editor: &RoomMember) {
        let result = {
            let news_store = self.news_store.lock().unwrap();
            let news = news_store.news();
            let config = self.config.clone();

            render::render(news, config, editor)
        };

        // Upload rendered content as markdown file
        let mut bytes = result.rendered.as_bytes();
        let response = self
            .client
            .upload(&mime::TEXT_PLAIN_UTF_8, &mut bytes)
            .await
            .expect("Can't upload rendered file.");

        // Send file
        self.send_file(response.content_uri, "rendered.md".to_string(), true)
            .await;

        // Send warnings
        let warnings = utils::format_messages(true, &result.warnings);
        if !result.warnings.is_empty() {
            self.send_message(&warnings, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }

        // Send notes
        let notes = utils::format_messages(false, &result.notes);
        if !result.notes.is_empty() {
            self.send_message(&notes, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }

        // Generate a curl command which can get used to download all files (images/videos).
        let mut files = result.images.clone();
        files.append(&mut result.videos.clone());
        if !files.is_empty() {
            self.send_message(
                "Use this command to download all files:",
                BotMsgType::AdminRoomHtmlNotice,
            )
            .await;

            let mut curl_command = "curl".to_string();
            for (filename, uri) in &files {
                if uri.is_valid() {
                    let url = format!(
                        "{}_matrix/media/r0/download/{}/{}",
                        self.client.homeserver().await,
                        uri.server_name().unwrap(),
                        uri.media_id().unwrap()
                    );

                    write!(curl_command, " {} -o {}", url, filename).unwrap();
                }
            }

            let msg = format!("<pre><code>{}</code></pre>\n", curl_command);
            self.send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }
    }

    async fn restart_command(&self) {
        self.send_message("Restarting hebbot…", BotMsgType::AdminRoomPlainNotice)
            .await;
        Command::new("/proc/self/exe").exec();
    }

    async fn say_command(&self, msg: &str) {
        self.send_message(msg, BotMsgType::ReportingRoomPlainText)
            .await;
    }

    async fn status_command(&self) {
        let msg = {
            let news_store = self.news_store.lock().unwrap();
            let news = news_store.news();

            let mut assigned_count = 0;
            let mut unassigned_count = 0;
            let sum = news.len();
            let mut assigned_list = String::new();
            let mut unassigned_list = String::new();

            for n in &news {
                let link = self.message_link(&n.event_id);
                let summary = n.message_summary();

                if n.is_assigned() {
                    assigned_count += 1;
                    write!(
                        assigned_list,
                        "- [{}] {}: {} <br>",
                        link, n.reporter_id, summary
                    )
                    .unwrap();
                } else {
                    unassigned_count += 1;
                    write!(
                        unassigned_list,
                        "- [{}] {}: {} <br>",
                        link, n.reporter_id, summary
                    )
                    .unwrap();
                }
            }

            format!(
                "{} news entries in total <br><br>\
                ✅ Assigned news entries: ({}): <br>{} <br>\
                ❌ Unassigned / ignored news entries ({}): <br>{}",
                sum, assigned_count, assigned_list, unassigned_count, unassigned_list
            )
        };

        self.send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
    }

    async fn update_config_command(&self) {
        self.send_message(
            "Updating bot configuration…",
            BotMsgType::AdminRoomHtmlNotice,
        )
        .await;

        let command = self.config.update_config_command.clone();
        let msg = match utils::execute_command(&command).await {
            Some(stdout) => format!(
                "✅ Updated bot configuration!<br><pre><code>{}</code></pre>",
                stdout
            ),
            None => "❌ Unable to run update command. Check bot logs for more details.".to_string(),
        };

        self.send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
            .await;
        self.restart_command().await;
    }

    async fn unrecognized_command(&self) {
        let msg = "Unrecognized command. Use !help to list available commands.";
        self.send_message(msg, BotMsgType::AdminRoomPlainNotice)
            .await;
    }

    async fn add_news(&self, news: News, notify_reporter: bool) {
        let link = self.message_link(&news.event_id);

        // Check if the news already exists
        if self
            .news_store
            .lock()
            .unwrap()
            .news_by_message_id(&news.event_id)
            .is_some()
        {
            let msg = format!(
                "⚠️ Cannot resubmit a news item that has already been added. [{}]",
                link
            );
            self.send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
                .await;
            return;
        }

        // Check min message length
        if news.message().len() > 30 {
            if notify_reporter && !self.config.ack_text.is_empty() {
                let msg = &self
                    .config
                    .ack_text
                    .replace("{{user}}", &news.reporter_display_name);
                self.send_message(msg, BotMsgType::ReportingRoomPlainNotice)
                    .await;
            }

            let msg = format!("✅ {} submitted a news entry. [{}]", news.reporter_id, link);
            self.send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
                .await;

            // remove bot name from message
            let bot_id = self.client.user_id().await.unwrap();
            news.set_message(utils::remove_bot_name(&news.message(), &bot_id));

            // Pre-populate with emojis to facilitate the editor's work
            for project in &self.config.projects {
                let regex = Regex::new(&format!(
                    "(?i)\\b{}\\b|\\b{}\\b",
                    project.name, project.title,
                ))
                .unwrap();
                if regex.is_match(&news.message()) {
                    self.send_reaction(&format!("{}?", &project.emoji), &news.event_id)
                        .await;
                }
            }
            for section in self.config.sections_by_usual_reporter(&news.reporter_id) {
                self.send_reaction(&section.emoji, &EventId::parse(&news.event_id).unwrap())
                    .await;
            }

            // Save it in message store
            self.news_store.lock().unwrap().add_news(news);
        } else {
            let msg = format!(
                "❌ {}: Your update is too short and was not stored. This limitation was set-up to limit spam.",
                news.reporter_display_name
            );
            self.send_message(&msg, BotMsgType::ReportingRoomPlainNotice)
                .await;
        }
    }

    async fn is_editor(&self, member: &RoomMember) -> bool {
        let user_id = member.user_id().to_owned();
        self.config.editors.contains(&user_id)
    }

    fn message_link(&self, event_id: &EventId) -> String {
        let room_id = self.config.reporting_room_id.clone();
        format!(
            "<a href=\"https://matrix.to/#/{}/{}\">open message</a>",
            room_id, event_id
        )
    }
}
