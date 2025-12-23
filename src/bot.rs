use chrono::{DateTime, Utc};

use matrix_sdk::config::{RequestConfig, SyncSettings};
use matrix_sdk::event_handler::Ctx;
use matrix_sdk::room::RoomMember;
use matrix_sdk::ruma::events::reaction::{OriginalSyncReactionEvent, ReactionEventContent};
use matrix_sdk::ruma::events::relation::Annotation;
use matrix_sdk::ruma::events::room::message::{
    FileMessageEventContent, MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent,
};
use matrix_sdk::ruma::events::room::redaction::SyncRoomRedactionEvent;
use matrix_sdk::ruma::events::room::MediaSource;
use matrix_sdk::ruma::events::Mentions;
use matrix_sdk::ruma::{EventId, OwnedMxcUri, RoomId, ServerName, UserId};
use matrix_sdk::{Client, Room, RoomState};

use regex::Regex;

use std::env;
use std::fmt::Write;
use std::io::Write as WriteExt;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use crate::utils::MessageEventExt;
use crate::{render, utils, BotMessageType as BotMsgType, Config, News, NewsStore, ReactionType};

#[derive(Clone)]
pub struct Bot {
    config: Config,
    news_store: Arc<Mutex<NewsStore>>,
    client: Client,
    reporting_room: Room,
    admin_room: Room,
}

impl Bot {
    pub async fn run() {
        let config_result = Config::read();
        let config = config_result.config;
        let news_store = Arc::new(Mutex::new(NewsStore::read()));

        let username = config.bot_user_id.as_str();
        let password = env::var("BOT_PASSWORD").expect("BOT_PASSWORD env variable not specified");

        let user = UserId::parse(username).expect("Unable to parse bot user id");
        let server_name = ServerName::parse(user.server_name()).unwrap();
        let request_config = RequestConfig::new().force_auth();

        let mut client_builder = Client::builder()
            .server_name(&server_name)
            .request_config(request_config);
        if let Ok(value) = env::var("HOMESERVER_URL") {
            client_builder = client_builder.homeserver_url(value);
        }
        let client = client_builder.build().await.unwrap();

        Self::login(&client, user.localpart(), &password).await;

        // Get matrix rooms IDs
        let reporting_room_id = RoomId::parse(config.reporting_room_id.as_str()).unwrap();
        let admin_room_id = RoomId::parse(config.admin_room_id.as_str()).unwrap();

        // Try to accept reporting room invite, if any
        for room in client.invited_rooms() {
            if room.room_id() == reporting_room_id {
                room.join()
                    .await
                    .expect("Hebbot could not join the reporting room");
            } else if room.room_id() == admin_room_id {
                room.join()
                    .await
                    .expect("Hebbot could not join the admin room");
            } else {
                info!("Ignored room: {}", room.room_id());
            }
        }

        // Sync to make sure that the bot is aware of the newly joined rooms
        client
            .sync_once(SyncSettings::new())
            .await
            .expect("Unable to sync");

        let reporting_room = client
            .get_room(&reporting_room_id)
            .expect("Unable to get reporting room");

        let admin_room = client
            .get_room(&admin_room_id)
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
        bot.client.add_event_handler_context(bot.clone());

        bot.client.add_event_handler(Self::on_room_message);
        bot.client.add_event_handler(Self::on_room_reaction);
        bot.client.add_event_handler(Self::on_room_redaction);

        info!("Started syncing…");
        bot.client.sync(SyncSettings::new()).await.unwrap();
    }

    /// Login
    async fn login(client: &Client, user: &str, pwd: &str) {
        info!("Logging in…");
        let response = client
            .matrix_auth()
            .login_username(user, pwd)
            .initial_device_display_name("hebbot")
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

        room.send(content).await.expect("Unable to send message");
    }

    /// Simplified method for sending a reaction emoji
    async fn send_reaction(&self, reaction: &str, msg_event_id: &EventId) {
        let content = ReactionEventContent::new(Annotation::new(
            msg_event_id.to_owned(),
            reaction.to_string(),
        ));

        if let Err(err) = self.reporting_room.send(content).await {
            warn!(
                "Could not send {} reaction to msg {}: {}",
                reaction, msg_event_id, err
            );
        }
    }

    /// Simplified method for sending a file
    async fn send_file(&self, url: OwnedMxcUri, filename: String, admin_room: bool) {
        debug!("Send file (url: {:?}, admin-room: {:?})", url, admin_room);

        let file_content = FileMessageEventContent::plain(filename, url);
        let msgtype = MessageType::File(file_content);
        let content = RoomMessageEventContent::new(msgtype);

        let room = if admin_room {
            &self.admin_room
        } else {
            &self.reporting_room
        };

        room.send(content).await.expect("Unable to send file");
    }

    /// Handling room messages events
    async fn on_room_message(event: OriginalSyncRoomMessageEvent, room: Room, Ctx(bot): Ctx<Bot>) {
        if room.state() != RoomState::Joined {
            return;
        }

        // Message edit
        if let Some(edited_msg_event_id) = event.edited_event_id() {
            if let Some(text) = event.text(false) {
                // Reporting room
                if room.room_id() == bot.reporting_room.room_id() {
                    bot.on_reporting_room_msg_edit(text, edited_msg_event_id)
                        .await;
                }
            }
        }
        // Standard text message
        else if let Some(text) = event.text(false) {
            let member = room.get_member(&event.sender).await.unwrap().unwrap();
            let id = &event.event_id;

            // Reporting room
            if room.room_id() == bot.reporting_room.room_id() {
                bot.on_reporting_room_msg(text, event.content.mentions.as_ref(), &member, id)
                    .await;
            }

            // Admin room
            if room.room_id() == bot.admin_room.room_id() {
                bot.on_admin_room_message(text, &member).await;
            }
        }
    }

    /// Handling room reaction events
    async fn on_room_reaction(event: OriginalSyncReactionEvent, room: Room, Ctx(bot): Ctx<Bot>) {
        if room.state() != RoomState::Joined {
            return;
        }

        let reaction_sender = room.get_member(&event.sender).await.unwrap().unwrap();
        let reaction_event_id = event.event_id.clone();
        let relation = &event.content.relates_to;
        let related_event_id = relation.event_id.clone();
        let emoji = &relation.key.replace('\u{fe0f}', "");

        if let Some(related_event) = utils::room_event_by_id(&room, &related_event_id).await {
            if let Some(related_event) = utils::as_message_event(&related_event) {
                // Reporting room
                if room.room_id() == bot.reporting_room.room_id() {
                    bot.on_reporting_room_reaction(
                        &room,
                        &reaction_sender,
                        emoji,
                        &reaction_event_id,
                        related_event,
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

    /// Handling room redaction events (= something got removed/reverted)
    async fn on_room_redaction(event: SyncRoomRedactionEvent, room: Room, Ctx(bot): Ctx<Bot>) {
        // In some room versions, the redacts field of the redaction event can be redacted,
        // so we return early if we don't know which event was redacted.
        let room_version = room.clone_info().room_version_rules_or_default();
        let Some(redacted_event_id) = event.redacts(&room_version.redaction) else {
            return;
        };

        if room.state() != RoomState::Joined {
            return;
        }
        let member = room.get_member(event.sender()).await.unwrap().unwrap();

        // Reporting room
        if room.room_id() == bot.reporting_room.room_id() {
            bot.on_reporting_room_redaction(&member, redacted_event_id)
                .await;
        }
    }

    /// New message in reporting room
    /// - When the bot gets mentioned with `m.mentions` or by name
    ///   at the beginning of the message, the message will get
    ///   stored as News in NewsStore
    async fn on_reporting_room_msg(
        &self,
        message: &str,
        mentions: Option<&Mentions>,
        member: &RoomMember,
        event_id: &EventId,
    ) {
        // We're going to ignore all messages, except if the mentions contain the ID of the bot,
        // or if the message mentions the bot by name at the beginning
        let bot_id = self.client.user_id().unwrap();
        let bot_display_name = self.client.account().get_display_name().await.unwrap();
        if mentions.is_none_or(|mentions| !mentions.user_ids.contains(bot_id))
            && !utils::msg_starts_with_mention(bot_id, bot_display_name, message)
        {
            return;
        }

        // Create new news entry...
        let news = News::new(event_id.to_owned(), member, message.to_owned());
        self.add_news(news, true).await;
    }

    /// New message in reporting room
    /// - When the bot gets mentioned at the beginning of the message,
    ///   the message will get stored as News in NewsStore
    async fn on_reporting_room_msg_edit(
        &self,
        updated_message: &str,
        edited_msg_event_id: &EventId,
    ) {
        let bot_id = self.client.user_id().unwrap();
        let bot_display_name = self.client.account().get_display_name().await.ok().unwrap();
        let updated_message = utils::remove_bot_name(bot_id, bot_display_name, updated_message);
        let link = self.message_link(edited_msg_event_id);

        let message = {
            let news_store = self.news_store.lock().unwrap();
            let msg = if let Some(news) = news_store.news_by_message_id(edited_msg_event_id) {
                news.set_message(updated_message);
                if news.is_assigned() {
                    Some(format!(
                        "✅ The news entry by {} got edited. Check the new text, and make sure you want to keep the assigned project/section. [{}]",
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
        related_event: &OriginalSyncRoomMessageEvent,
    ) {
        let reaction_emoji = reaction_emoji.strip_suffix(" ?").unwrap_or(reaction_emoji);

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
            let related_event_id = &related_event.event_id;
            let related_event_timestamp: DateTime<Utc> = related_event
                .origin_server_ts
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

            if let Some(text) = related_event.text(true) {
                // Check if the reaction == notice emoji,
                // Yes -> Try to add the message as news submission
                if utils::emoji_cmp(reaction_emoji, &self.config.notice_emoji) {
                    // we need related_event's sender
                    let related_event_sender = room
                        .get_member(&related_event.sender)
                        .await
                        .unwrap()
                        .unwrap();

                    if !sender_is_editor
                        && (reaction_sender.user_id() != related_event_sender.user_id()
                            && self.config.restrict_notice)
                    {
                        return;
                    }

                    let news = News::new(
                        related_event_id.clone(),
                        &related_event_sender,
                        text.to_owned(),
                    );
                    self.add_news(news, false).await;
                    None
                }
                // Check if related message is a news entry
                // (Adding the entry to a project / section by using the corresponding reaction emoji)
                else if let Some(news) = self
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
                                "✅ {} added {}’s news entry [{}] to the “{}” section.",
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
                                "✅ {} added the project description “{}” to {}’s news entry [{}].",
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
                            "⚠️ Unable to process {}’s {} reaction, message doesn’t exist or isn’t a news submission [{}]\n(ID {})",
                            reaction_sender.user_id(),
                            reaction_type,
                            link,
                            related_event_id
                        ))
                }
            }
            // Check if related message is an image
            else if let Some(image) = related_event.image() {
                match reaction_type {
                    ReactionType::Notice => {
                        let reporter_id = reaction_sender.user_id();
                        let news_store = self.news_store.lock().unwrap();
                        if let Some(news) = news_store.find_related_news(
                            related_event.sender.as_ref(),
                            &related_event_timestamp,
                        ) {
                            if !sender_is_editor
                                && (reaction_sender.user_id() != related_event.sender
                                    && self.config.restrict_notice)
                            {
                                return;
                            }
                            if let MediaSource::Plain(mxc_uri) = &image.source {
                                news.add_image(
                                    reaction_event_id.to_owned(),
                                    related_event_id.clone(),
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
                }
            }
            // Check if related message is a video
            else if let Some(video) = related_event.video() {
                match reaction_type {
                    ReactionType::Notice => {
                        let reporter_id = reaction_sender.user_id();
                        let news_store = self.news_store.lock().unwrap();
                        if let Some(news) = news_store.find_related_news(
                            related_event.sender.as_ref(),
                            &related_event_timestamp,
                        ) {
                            if let MediaSource::Plain(mxc_uri) = &video.source {
                                news.add_video(
                                    reaction_event_id.to_owned(),
                                    related_event_id.clone(),
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
                }
            } else {
                debug!(
                    "Unsupported message type {:?} (id {}",
                    related_event.msgtype(),
                    related_event_id
                );
                None
            }
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
            let mut news_store = self.news_store.lock().unwrap();
            let link = self.message_link(redacted_event_id);

            // Redaction / deletion of the news entry itself
            let msg = if let Ok(news) = news_store.remove_news(redacted_event_id) {
                Some(format!(
                    "✅ {}’s news entry got deleted by {}.",
                    news.reporter_id,
                    member.user_id()
                ))
            // An image / video got redacted / deleted
            } else if let Some(news) = news_store.news_by_file_id(redacted_event_id) {
                news.remove_file(&redacted_event_id.to_owned());
                Some(format!(
                    "✅ {} deleted an image/video of {}’s news entry.",
                    member.user_id(),
                    news.reporter_id,
                ))
            // Redaction of reaction events (project / section)
            } else if let Some(news) = news_store.news_by_reaction_id(redacted_event_id) {
                let reaction_type = news.remove_reaction_id(redacted_event_id);
                if reaction_type == ReactionType::Notice {
                    Some(format!(
                        "✅ {} removed their image/video notice reaction from {}’s news entry. [{}]",
                        member.user_id(),
                        news.reporter_id,
                        link
                    ))
                } else if reaction_type != ReactionType::None {
                    Some(format!(
                        "✅ {} removed their {} reaction from {}’s news entry. [{}]",
                        member.user_id(),
                        reaction_type,
                        news.reporter_id,
                        link
                    ))
                } else {
                    debug!(
                        "❌️ Ignoring redaction, doesn’t match any known emoji reaction event id (ID {:?})",
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
    async fn on_admin_room_message(&self, msg: &str, member: &RoomMember) {
        let msg = msg.trim();

        // Check if the message is a command
        if !msg.starts_with('!') {
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
            "!publish" => self.publish_command(member).await,
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
            !publish \n\
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

            format!("✅ Cleared {} news entries!", news.len())
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
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                let msg = format!("❌ Could not render template: <pre>{}</pre>", error);
                self.send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
                    .await;
                return;
            }
        };

        // Upload rendered content as markdown file
        let bytes = result.rendered.into_bytes();
        let response = self
            .client
            .media()
            .upload(&mime::TEXT_PLAIN_UTF_8, bytes, None)
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
                        self.client.homeserver(),
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

    async fn publish_command(&self, editor: &RoomMember) {
        let result = {
            let news_store = self.news_store.lock().unwrap();
            let news = news_store.news();
            let config = self.config.clone();

            render::render(news, config, editor)
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                let msg = format!("❌ Could not render template: <pre>{}</pre>", error);
                self.send_message(&msg, BotMsgType::AdminRoomHtmlNotice)
                    .await;
                return;
            }
        };
        // Upload rendered content as markdown file
        let bytes = result.rendered.into_bytes();

        let publish_command = self.config.publish_command.clone();
        if let Some(command) = publish_command {
            let mut child = Command::new(command)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .expect("Can't spawn child process.");

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&bytes).expect("Couldn't write bytes.");
                let _ = stdin.flush();
            }

            let output = child
                .wait_with_output()
                .expect("Can't wait on the child command.");

            if output.status.success() {
                let stdout =
                    String::from_utf8(output.stdout.clone()).expect("Program output was not valid utf-8");
                self.send_message(&"publish_command was successful", BotMsgType::AdminRoomHtmlNotice)
                    .await;
                self.send_message(&stdout, BotMsgType::AdminRoomHtmlNotice)
                    .await;
            } else {
                if let Some(code) = output.status.code() {
                    self.send_message(
                        &format!("ErrorCode: {}", code),
                        BotMsgType::AdminRoomHtmlNotice,
                    )
                    .await;
                }
                let stderr =
                    String::from_utf8(output.stderr.clone()).expect("Program output was not valid utf-8");
                self.send_message(&stderr, BotMsgType::AdminRoomHtmlNotice)
                    .await;
            }
        } else {
            let message = utils::format_messages(
                true,
                &vec![
                    "No publish_command configured.".into(),
                    "Will not perform any action.".into(),
                ],
            );
            self.send_message(&message, BotMsgType::AdminRoomHtmlNotice)
                .await;
        }

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
                        self.client.homeserver(),
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
        let _ = Command::new("/proc/self/exe").exec();
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

        // remove bot name from message before we check length
        let bot_id = self.client.user_id().unwrap();
        let bot_display_name = self.client.account().get_display_name().await.ok().unwrap();
        news.set_message(utils::remove_bot_name(
            bot_id,
            bot_display_name,
            &news.message(),
        ));

        // Check min message length
        if news.message().len() > self.config.min_length {
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

            // Pre-populate with emojis to facilitate the editor's work
            for project in &self.config.projects {
                let regex = Regex::new(&format!(
                    "(?i)\\b{}\\b|\\b{}\\b",
                    project.name, project.title,
                ))
                .unwrap();
                if regex.is_match(&news.message()) {
                    self.send_reaction(&format!("{} ?", &project.emoji), &news.event_id)
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
