use async_process::{Command, Stdio};
use matrix_sdk::room::Room;
use matrix_sdk::ruma::api::client::r0::room::get_room_event::Request;
use matrix_sdk::ruma::events::room::message::{
    MessageType, Relation, Replacement, RoomMessageEventContent, SyncRoomMessageEvent,
    TextMessageEventContent,
};
use matrix_sdk::ruma::events::{AnyMessageEvent, AnyRoomEvent, MessageEvent};
use matrix_sdk::ruma::{EventId, UserId};
use matrix_sdk::{BaseRoomMember, RoomMember};
use regex::Regex;

use crate::News;

/// Try to convert a `AnyRoomEvent` into a `News`
pub fn news_by_event(any_room_event: &AnyRoomEvent, member: &RoomMember) -> Option<News> {
    // Fetch related event's
    // * event_id
    // * reporter_id
    // * reporter_display_name
    // * message
    if let AnyRoomEvent::Message(AnyMessageEvent::RoomMessage(MessageEvent {
        content:
            RoomMessageEventContent {
                msgtype: MessageType::Text(c),
                ..
            },
        sender,
        ..
    })) = any_room_event
    {
        let reporter_id = sender.to_string();
        let reporter_display_name = get_member_display_name(member);
        let message = c.body.clone();

        let news = News::new(
            any_room_event.event_id().clone().to_string(),
            reporter_id,
            reporter_display_name,
            message,
        );

        return Some(news);
    }

    None
}

/// Get room message by event id
pub async fn room_event_by_id(room: &Room, event_id: &EventId) -> Option<AnyRoomEvent> {
    let request = Request::new(room.room_id(), event_id);
    let event = room.event(request).await.ok()?.event.deserialize().ok()?;

    Some(event)
}

pub async fn message_type(room_event: &AnyRoomEvent) -> Option<MessageType> {
    if let AnyRoomEvent::Message(AnyMessageEvent::RoomMessage(MessageEvent {
        content: RoomMessageEventContent {
            msgtype: msg_type, ..
        },
        ..
    })) = room_event
    {
        Some(msg_type.clone())
    } else {
        None
    }
}

/// A simplified way of getting the text from a message event
pub fn get_message_event_text(event: &SyncRoomMessageEvent) -> Option<String> {
    if let RoomMessageEventContent {
        msgtype: MessageType::Text(TextMessageEventContent { body: msg_body, .. }),
        relates_to: None,
        ..
    } = &event.content
    {
        return Some(msg_body.to_owned());
    }
    None
}

/// A simplified way of getting an edited message
pub fn get_edited_message_event_text(event: &SyncRoomMessageEvent) -> Option<(EventId, String)> {
    if let Some(Relation::Replacement(Replacement {
        event_id,
        new_content,
        ..
    })) = &event.content.relates_to
    {
        if let RoomMessageEventContent {
            msgtype: MessageType::Text(TextMessageEventContent { body: msg_body, .. }),
            relates_to: None,
            ..
        } = &**new_content
        {
            return Some((event_id.clone(), msg_body.to_string()));
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
pub fn msg_starts_with_mention(user_id: UserId, msg: String) -> bool {
    let localpart = user_id.localpart().to_lowercase();
    // Catch "@botname ..." messages
    let msg = msg.replace(&format!("@{}", localpart), &localpart);
    msg.as_str().to_lowercase().starts_with(&localpart)
}

/// Returns `true` if the emojis are matching
pub fn emoji_cmp(a: &str, b: &str) -> bool {
    let a = &a.replace("\u{fe0f}", "");
    let b = &b.replace("\u{fe0f}", "");
    a == b
}

/// Remove bot name from message
pub fn remove_bot_name(message: &str, bot: &UserId) -> String {
    let regex = format!("(?i)^@?{}(:{})?:?", bot.localpart(), bot.server_name());
    let re = Regex::new(&regex).unwrap();
    let message = re.replace(message, "");
    message.trim().to_string()
}

pub fn format_messages(is_warning: bool, list: &[String]) -> String {
    let emoji = if is_warning { "⚠️" } else { "ℹ️" };

    let mut messages = String::new();
    for message in list {
        messages += &format!("- {} {}<br>", emoji, message);
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
    lines += &String::from_utf8(out.stdout).ok()?;
    lines += &String::from_utf8(out.stderr).ok()?;

    dbg!(&lines);
    Some(lines)
}
