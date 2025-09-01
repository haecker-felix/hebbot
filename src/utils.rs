use async_process::{Command, Stdio};
use matrix_sdk::deserialized_responses::TimelineEventKind;
use matrix_sdk::room::Room;
use matrix_sdk::ruma::events::room::message::{
    MessageType, NoticeMessageEventContent, OriginalSyncRoomMessageEvent, Relation,
    TextMessageEventContent,
};
use matrix_sdk::ruma::events::{
    AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent,
};
use matrix_sdk::ruma::{EventId, OwnedEventId, UserId};
use regex::Regex;

use std::fmt::Write;
use std::fs::File;
use std::io::Read;
use std::{env, str};

/// Get room message by event id
pub async fn room_event_by_id(room: &Room, event_id: &EventId) -> Option<AnySyncTimelineEvent> {
    let timeline_event = room.event(event_id, None).await.ok()?;

    match timeline_event.kind {
        TimelineEventKind::PlainText { event } => event.deserialize().ok(),
        ev => {
            // This covers the other variants: DecryptedRoomEvent and UnableToDecrypt.
            // At the moment Hebbot does not support being used in encrypted rooms.
            warn!("Unsupported E2EE event: {ev:?}");
            None
        }
    }
}

/// Get the given event as a message event, if it is one.
pub fn as_message_event(
    room_event: &AnySyncTimelineEvent,
) -> Option<&OriginalSyncRoomMessageEvent> {
    if let AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
        SyncMessageLikeEvent::Original(ev),
    )) = room_event
    {
        Some(ev)
    } else {
        None
    }
}

/// A simplified way of getting the text from a message event
pub fn get_message_event_text(
    event: &OriginalSyncRoomMessageEvent,
    allow_notice: bool,
) -> Option<String> {
    match &event.content.msgtype {
        MessageType::Text(TextMessageEventContent { body, .. }) => Some(body),
        MessageType::Notice(NoticeMessageEventContent { body, .. }) if allow_notice => Some(body),
        _ => None,
    }
    .cloned()
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
    if let Some(display_name) = display_name {
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

    let mut file = File::open(path.clone())
        .unwrap_or_else(|_| panic!("Unable to open file: {path} ({env_var_name})"));
    let mut template = String::new();
    file.read_to_string(&mut template)
        .expect("Unable to read file");

    template
}
