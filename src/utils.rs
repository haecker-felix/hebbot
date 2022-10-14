use async_process::{Command, Stdio};
use matrix_sdk::room::Room;
use matrix_sdk::ruma::events::room::message::{
    MessageType, NoticeMessageEventContent, OriginalSyncRoomMessageEvent, Relation,
    RoomMessageEventContent, TextMessageEventContent,
};
use matrix_sdk::ruma::events::{
    AnyMessageLikeEvent, AnyRoomEvent, MessageLikeEvent, OriginalMessageLikeEvent,
};
use matrix_sdk::ruma::{EventId, OwnedEventId, UserId};
use matrix_sdk::{BaseRoomMember, RoomMember};
use regex::Regex;

use std::fmt::Write;
use std::fs::File;
use std::io::Read;
use std::{env, str};

use crate::News;

/// Try to convert a `AnyRoomEvent` into a `News`
pub fn create_news_by_event(any_room_event: &AnyRoomEvent, member: &RoomMember) -> Option<News> {
    // Fetch related event's
    // * event_id
    // * reporter_id
    // * reporter_display_name
    // * message
    if let AnyRoomEvent::MessageLike(AnyMessageLikeEvent::RoomMessage(
        MessageLikeEvent::Original(OriginalMessageLikeEvent {
            content:
                RoomMessageEventContent {
                    msgtype:
                        MessageType::Text(TextMessageEventContent { body, .. })
                        | MessageType::Notice(NoticeMessageEventContent { body, .. }),
                    ..
                },
            sender,
            ..
        }),
    )) = any_room_event
    {
        let reporter_id = sender.to_owned();
        let reporter_display_name = get_member_display_name(member);
        let message = body.clone();

        let news = News::new(
            any_room_event.event_id().to_owned(),
            reporter_id,
            reporter_display_name,
            message,
        );

        Some(news)
    } else {
        None
    }
}

/// Get room message by event id
pub async fn room_event_by_id(room: &Room, event_id: &EventId) -> Option<AnyRoomEvent> {
    room.event(event_id).await.ok()?.event.deserialize().ok()
}

pub async fn message_type(room_event: &AnyRoomEvent) -> Option<MessageType> {
    if let AnyRoomEvent::MessageLike(AnyMessageLikeEvent::RoomMessage(
        MessageLikeEvent::Original(ev),
    )) = room_event
    {
        Some(ev.content.msgtype.clone())
    } else {
        None
    }
}

/// A simplified way of getting the text from a message event
pub fn get_message_event_text(event: &OriginalSyncRoomMessageEvent) -> Option<String> {
    if let MessageType::Text(TextMessageEventContent { body, .. }) = &event.content.msgtype {
        Some(body.to_owned())
    } else {
        None
    }
}

/// A simplified way of getting an edited message
pub fn get_edited_message_event_text(
    event: &OriginalSyncRoomMessageEvent,
) -> Option<(OwnedEventId, String)> {
    if let Some(Relation::Replacement(r)) = &event.content.relates_to {
        if let MessageType::Text(TextMessageEventContent { body, .. }) = &r.new_content.msgtype {
            return Some((r.event_id.clone(), body.to_owned()));
        }
    }

    None
}

/// Gets display name for a RoomMember
/// falls back to user_id for accounts without displayname
pub fn get_member_display_name(member: &BaseRoomMember) -> String {
    member
        .display_name()
        .unwrap_or_else(|| member.user_id().as_str())
        .to_string()
}

/// Checks if a message starts with a user_id mention
/// Automatically handles @ in front of the name
pub fn msg_starts_with_mention(
    user_id: &UserId,
    display_name: Option<String>,
    msg: String,
) -> bool {
    let localpart = user_id.localpart().to_lowercase();
    // Catch "@botname ..." messages
    let msg = msg.replace(&format!("@{}", localpart), &localpart);
    let msg = msg.as_str().to_lowercase();

    let matches_localpart = msg.starts_with(&localpart);
    let matches_display_name = if let Some(display_name) = display_name {
        msg.starts_with(&display_name.to_lowercase())
    } else {
        false
    };

    matches_localpart || matches_display_name
}

/// Returns `true` if the emojis are matching
pub fn emoji_cmp(a: &str, b: &str) -> bool {
    let a = &a.replace('\u{fe0f}', "");
    let b = &b.replace('\u{fe0f}', "");
    a == b
}

/// Remove bot name from message
pub fn remove_bot_name(bot: &UserId, display_name: Option<String>, msg: &str) -> String {
    // remove user id
    let regex = format!("(?i)^@?{}(:{})?:?", bot.localpart(), bot.server_name());
    let re = Regex::new(&regex).unwrap();
    let mut msg = re.replace(msg, "").to_string();

    // remove display name
    if let Some(display_name) = display_name{
        let regex = format!("(?i)^{}:?", display_name);
        let re = Regex::new(&regex).unwrap();
        msg = re.replace(&msg, "").to_string();
    }

    msg.trim().to_string()
}

pub fn format_messages(is_warning: bool, list: &[String]) -> String {
    let emoji = if is_warning { "⚠️" } else { "ℹ️" };

    let mut messages = String::new();
    for message in list {
        write!(messages, "- {} {}<br>", emoji, message).unwrap();
    }
    messages
}

pub async fn execute_command(launch: &str) -> Option<String> {
    debug!("Executing command: {:?}", launch);

    // Merge stdout/stderr
    let launch = format!("{} 2>&1", launch);

    let out = Command::new("sh")
        .arg("-c")
        .arg(launch)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .ok()?;

    let mut lines = String::new();
    lines += str::from_utf8(&out.stdout).ok()?;
    lines += str::from_utf8(&out.stderr).ok()?;

    dbg!(&lines);
    Some(lines)
}

pub fn file_from_env(env_var_name: &str, fallback: &str) -> String {
    let path = match env::var(env_var_name) {
        Ok(val) => val,
        Err(_) => fallback.to_string(),
    };

    debug!("Trying to read file from path: {:?}", path);

    let mut file = File::open(path).expect("Unable to open file");
    let mut template = String::new();
    file.read_to_string(&mut template)
        .expect("Unable to read file");

    template
}
