use async_process::{Command, Stdio};
use matrix_sdk::deserialized_responses::TimelineEventKind;
use matrix_sdk::room::Room;
use matrix_sdk::ruma::events::room::message::{
    ImageMessageEventContent, MessageType, NoticeMessageEventContent, OriginalSyncRoomMessageEvent,
    Relation, TextMessageEventContent, VideoMessageEventContent,
};
use matrix_sdk::ruma::events::{
    AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent,
};
use matrix_sdk::ruma::{EventId, UserId};
use regex::Regex;

use std::fmt::Write;
use std::fs::File;
use std::io::Read;
use std::{env, str};

/// Helper trait for room message events.
///
/// The main feature of this trait is that it always fetches the message
/// content in the appropriate place:
///
/// - If there is a latest edit in `unsigned`, use its message type
/// - If this is an edit, use it the `new_content`
/// - Otherwise, use the `content`
pub trait MessageEventExt {
    /// The message type.
    fn msgtype(&self) -> &MessageType;

    ///If this message is an edit, the related event ID.
    fn edited_event_id(&self) -> Option<&EventId>;

    /// The text of the message, if any.
    fn text(&self, allow_notice: bool) -> Option<&str>;

    /// The image of the message, if any.
    fn image(&self) -> Option<&ImageMessageEventContent>;

    /// The video of the message, if any.
    fn video(&self) -> Option<&VideoMessageEventContent>;
}

impl MessageEventExt for OriginalSyncRoomMessageEvent {
    fn msgtype(&self) -> &MessageType {
        if let Some(Relation::Replacement(edit)) = self
            .unsigned
            .relations
            .replace
            .as_deref()
            .and_then(|edit| edit.content.relates_to.as_ref())
        {
            &edit.new_content.msgtype
        } else if let Some(Relation::Replacement(edit)) = &self.content.relates_to {
            &edit.new_content.msgtype
        } else {
            &self.content.msgtype
        }
    }

    fn edited_event_id(&self) -> Option<&EventId> {
        if let Some(Relation::Replacement(edit)) = &self.content.relates_to {
            Some(&edit.event_id)
        } else {
            None
        }
    }

    fn text(&self, allow_notice: bool) -> Option<&str> {
        match self.msgtype() {
            MessageType::Text(TextMessageEventContent { body, .. }) => Some(body),
            MessageType::Notice(NoticeMessageEventContent { body, .. }) if allow_notice => {
                Some(body)
            }
            _ => None,
        }
    }

    fn image(&self) -> Option<&ImageMessageEventContent> {
        if let MessageType::Image(content) = self.msgtype() {
            Some(content)
        } else {
            None
        }
    }

    fn video(&self) -> Option<&VideoMessageEventContent> {
        if let MessageType::Video(content) = self.msgtype() {
            Some(content)
        } else {
            None
        }
    }
}

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

/// Checks if a message starts with a user_id mention
/// Automatically handles @ in front of the name
pub fn msg_starts_with_mention(user_id: &UserId, display_name: Option<String>, msg: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use assert_matches2::assert_matches;
    use matrix_sdk::ruma::events::room::message::{MessageType, OriginalSyncRoomMessageEvent};
    use matrix_sdk::ruma::events::room::MediaSource;
    use matrix_sdk::ruma::serde::JsonObject;
    use matrix_sdk::ruma::{event_id, user_id, EventId, UserId};
    use serde_json::json;

    use super::{msg_starts_with_mention, remove_bot_name, MessageEventExt};

    static ORIGINAL_EVENT_ID: LazyLock<&'static EventId> = LazyLock::new(|| event_id!("$original"));
    static EDIT_EVENT_ID: LazyLock<&'static EventId> = LazyLock::new(|| event_id!("$edit"));

    /// Construct an `m.room.message` event with the given JSON content.
    fn room_message_event(event_id: &EventId, content: serde_json::Value) -> serde_json::Value {
        json!({
            "content": content,
            "type": "m.room.message",
            "event_id": event_id,
            "sender": "@user:matrix.local",
            "origin_server_ts": 1_000_000,
        })
    }

    /// Add the given edit to the `m.relations` object of the `unsigned` object of the given original event.
    fn insert_aggregated_edit(original: &mut serde_json::Value, edit: serde_json::Value) {
        let original = original.as_object_mut().unwrap();
        let unsigned = original
            .entry("unsigned")
            .or_insert_with(|| JsonObject::new().into())
            .as_object_mut()
            .unwrap();
        let relations = unsigned
            .entry("m.relations")
            .or_insert_with(|| JsonObject::new().into())
            .as_object_mut()
            .unwrap();

        relations.insert("m.replace".to_owned(), edit);
    }

    #[test]
    fn message_event_ext_text() {
        // Original event.
        let mut original_json = room_message_event(
            &ORIGINAL_EVENT_ID,
            json!({
                "msgtype": "m.text",
                "body": "hebbot: Hello fiend!",
            }),
        );

        let event: OriginalSyncRoomMessageEvent =
            serde_json::from_value(original_json.clone()).unwrap();
        assert_matches!(event.msgtype(), MessageType::Text(_));
        assert_eq!(event.edited_event_id(), None);
        assert_eq!(event.text(true), Some("hebbot: Hello fiend!"));
        assert_eq!(event.text(false), Some("hebbot: Hello fiend!"));
        assert_matches!(event.image(), None);
        assert_matches!(event.video(), None);

        // Edit.
        let edit_json = room_message_event(
            &EDIT_EVENT_ID,
            json!({
                "msgtype": "m.text",
                "body": "*hebbot: Hello friend!",
                "m.new_content": {
                    "body": "hebbot: Hello friend!",
                    "msgtype": "m.text",
                },
                "m.relates_to": {
                    "rel_type": "m.replace",
                    "event_id": *ORIGINAL_EVENT_ID,
                },
            }),
        );

        let event: OriginalSyncRoomMessageEvent =
            serde_json::from_value(edit_json.clone()).unwrap();
        assert_matches!(event.msgtype(), MessageType::Text(_));
        assert_eq!(event.edited_event_id(), Some(*ORIGINAL_EVENT_ID));
        assert_eq!(event.text(true), Some("hebbot: Hello friend!"));
        assert_eq!(event.text(false), Some("hebbot: Hello friend!"));
        assert_matches!(event.image(), None);
        assert_matches!(event.video(), None);

        // Original event with aggregated edit.
        insert_aggregated_edit(&mut original_json, edit_json);

        let event: OriginalSyncRoomMessageEvent = serde_json::from_value(original_json).unwrap();
        assert_matches!(event.msgtype(), MessageType::Text(_));
        assert_eq!(event.edited_event_id(), None);
        assert_eq!(event.text(true), Some("hebbot: Hello friend!"));
        assert_eq!(event.text(false), Some("hebbot: Hello friend!"));
        assert_matches!(event.image(), None);
        assert_matches!(event.video(), None);
    }

    #[test]
    fn message_event_ext_notice() {
        // Original event.
        let mut original_json = room_message_event(
            &ORIGINAL_EVENT_ID,
            json!({
                "msgtype": "m.notice",
                "body": "hebbot: Hello fiend!",
            }),
        );

        let event: OriginalSyncRoomMessageEvent =
            serde_json::from_value(original_json.clone()).unwrap();
        assert_matches!(event.msgtype(), MessageType::Notice(_));
        assert_eq!(event.edited_event_id(), None);
        assert_eq!(event.text(true), Some("hebbot: Hello fiend!"));
        assert_eq!(event.text(false), None);
        assert_matches!(event.image(), None);
        assert_matches!(event.video(), None);

        // Edit.
        let edit_json = room_message_event(
            &EDIT_EVENT_ID,
            json!({
                "msgtype": "m.notice",
                "body": "*hebbot: Hello friend!",
                "m.new_content": {
                    "body": "hebbot: Hello friend!",
                    "msgtype": "m.notice",
                },
                "m.relates_to": {
                    "rel_type": "m.replace",
                    "event_id": *ORIGINAL_EVENT_ID,
                },
            }),
        );

        let event: OriginalSyncRoomMessageEvent =
            serde_json::from_value(edit_json.clone()).unwrap();
        assert_matches!(event.msgtype(), MessageType::Notice(_));
        assert_eq!(event.edited_event_id(), Some(*ORIGINAL_EVENT_ID));
        assert_eq!(event.text(true), Some("hebbot: Hello friend!"));
        assert_eq!(event.text(false), None);
        assert_matches!(event.image(), None);
        assert_matches!(event.video(), None);

        // Original event with aggregated edit.
        insert_aggregated_edit(&mut original_json, edit_json);

        let event: OriginalSyncRoomMessageEvent = serde_json::from_value(original_json).unwrap();
        assert_matches!(event.msgtype(), MessageType::Notice(_));
        assert_eq!(event.edited_event_id(), None);
        assert_eq!(event.text(true), Some("hebbot: Hello friend!"));
        assert_eq!(event.text(false), None);
        assert_matches!(event.image(), None);
        assert_matches!(event.video(), None);
    }

    #[test]
    fn message_event_ext_image() {
        // Original event.
        let mut original_json = room_message_event(
            &ORIGINAL_EVENT_ID,
            json!({
                "msgtype": "m.image",
                "body": "original_image.png",
                "url": "mxc://matrix.local/01234",
            }),
        );

        let event: OriginalSyncRoomMessageEvent =
            serde_json::from_value(original_json.clone()).unwrap();
        assert_matches!(event.msgtype(), MessageType::Image(_));
        assert_eq!(event.edited_event_id(), None);
        assert_eq!(event.text(true), None);
        assert_eq!(event.text(false), None);
        assert_matches!(event.image(), Some(image));
        assert_eq!(image.body, "original_image.png");
        assert_matches!(&image.source, MediaSource::Plain(uri));
        assert_eq!(uri, "mxc://matrix.local/01234");
        assert_matches!(event.video(), None);

        // Edit.
        let edit_json = room_message_event(
            &EDIT_EVENT_ID,
            json!({
                "msgtype": "m.image",
                "body": "*edited_image.png",
                "url": "mxc://matrix.local/56789",
                "m.new_content": {
                    "msgtype": "m.image",
                    "body": "edited_image.png",
                    "url": "mxc://matrix.local/56789",
                },
                "m.relates_to": {
                    "rel_type": "m.replace",
                    "event_id": *ORIGINAL_EVENT_ID,
                },
            }),
        );

        let event: OriginalSyncRoomMessageEvent =
            serde_json::from_value(edit_json.clone()).unwrap();
        assert_matches!(event.msgtype(), MessageType::Image(_));
        assert_eq!(event.edited_event_id(), Some(*ORIGINAL_EVENT_ID));
        assert_eq!(event.text(true), None);
        assert_eq!(event.text(false), None);
        assert_matches!(event.image(), Some(image));
        assert_eq!(image.body, "edited_image.png");
        assert_matches!(&image.source, MediaSource::Plain(uri));
        assert_eq!(uri, "mxc://matrix.local/56789");
        assert_matches!(event.video(), None);

        // Original event with aggregated edit.
        insert_aggregated_edit(&mut original_json, edit_json);

        let event: OriginalSyncRoomMessageEvent = serde_json::from_value(original_json).unwrap();
        assert_matches!(event.msgtype(), MessageType::Image(_));
        assert_eq!(event.edited_event_id(), None);
        assert_eq!(event.text(true), None);
        assert_eq!(event.text(false), None);
        assert_matches!(event.image(), Some(image));
        assert_eq!(image.body, "edited_image.png");
        assert_matches!(&image.source, MediaSource::Plain(uri));
        assert_eq!(uri, "mxc://matrix.local/56789");
        assert_matches!(event.video(), None);
    }

    #[test]
    fn message_event_ext_video() {
        // Original event.
        let mut original_json = room_message_event(
            &ORIGINAL_EVENT_ID,
            json!({
                "msgtype": "m.video",
                "body": "original_video.webm",
                "url": "mxc://matrix.local/01234",
            }),
        );

        let event: OriginalSyncRoomMessageEvent =
            serde_json::from_value(original_json.clone()).unwrap();
        assert_matches!(event.msgtype(), MessageType::Video(_));
        assert_eq!(event.edited_event_id(), None);
        assert_eq!(event.text(true), None);
        assert_eq!(event.text(false), None);
        assert_matches!(event.image(), None);
        assert_matches!(event.video(), Some(video));
        assert_eq!(video.body, "original_video.webm");
        assert_matches!(&video.source, MediaSource::Plain(uri));
        assert_eq!(uri, "mxc://matrix.local/01234");

        // Edit.
        let edit_json = room_message_event(
            &EDIT_EVENT_ID,
            json!({
                "msgtype": "m.video",
                "body": "*edited_video.webm",
                "url": "mxc://matrix.local/56789",
                "m.new_content": {
                    "msgtype": "m.video",
                    "body": "edited_video.webm",
                    "url": "mxc://matrix.local/56789",
                },
                "m.relates_to": {
                    "rel_type": "m.replace",
                    "event_id": *ORIGINAL_EVENT_ID,
                },
            }),
        );

        let event: OriginalSyncRoomMessageEvent =
            serde_json::from_value(edit_json.clone()).unwrap();
        assert_matches!(event.msgtype(), MessageType::Video(_));
        assert_eq!(event.edited_event_id(), Some(*ORIGINAL_EVENT_ID));
        assert_eq!(event.text(true), None);
        assert_eq!(event.text(false), None);
        assert_matches!(event.image(), None);
        assert_matches!(event.video(), Some(video));
        assert_eq!(video.body, "edited_video.webm");
        assert_matches!(&video.source, MediaSource::Plain(uri));
        assert_eq!(uri, "mxc://matrix.local/56789");

        // Original event with aggregated edit.
        insert_aggregated_edit(&mut original_json, edit_json);

        let event: OriginalSyncRoomMessageEvent = serde_json::from_value(original_json).unwrap();
        assert_matches!(event.msgtype(), MessageType::Video(_));
        assert_eq!(event.edited_event_id(), None);
        assert_eq!(event.text(true), None);
        assert_eq!(event.text(false), None);
        assert_matches!(event.image(), None);
        assert_matches!(event.video(), Some(video));
        assert_eq!(video.body, "edited_video.webm");
        assert_matches!(&video.source, MediaSource::Plain(uri));
        assert_eq!(uri, "mxc://matrix.local/56789");
    }

    #[test]
    fn msg_starts_with_mention_and_remove_bot_name() {
        let lowercase_user_id = user_id!("@hebbot:matrix.local");
        let uppercase_user_id = <&UserId>::try_from("@HEBBOT:matrix.local").unwrap();
        let lowercase_display_name = "the hebbot".to_owned();
        let uppercase_display_name = "THE HEBBOT".to_owned();

        let content = "This is my entry";

        let matching_localpart_prefixes = &[
            "hebbot: ",
            "@hebbot: ",
            "HEBBOT: ",
            "hebbot ",
            "@hebbot ",
            "HEBBOT ",
        ];

        for prefix in matching_localpart_prefixes {
            let message = format!("{prefix}{content}");

            // Log the message for debugging when the check fails.
            println!("Checking message: `{message}`");

            // Lowercase user ID and display name.
            assert!(msg_starts_with_mention(
                lowercase_user_id,
                Some(lowercase_display_name.clone()),
                &message
            ));
            assert_eq!(
                remove_bot_name(
                    lowercase_user_id,
                    Some(lowercase_display_name.clone()),
                    &message,
                ),
                content
            );

            // Lowercase user ID no display name.
            assert!(msg_starts_with_mention(lowercase_user_id, None, &message,));
            assert_eq!(remove_bot_name(lowercase_user_id, None, &message), content);

            // Uppercase user ID and display name.
            assert!(msg_starts_with_mention(
                uppercase_user_id,
                Some(uppercase_display_name.clone()),
                &message
            ));
            assert_eq!(
                remove_bot_name(
                    uppercase_user_id,
                    Some(uppercase_display_name.clone()),
                    &message,
                ),
                content
            );

            // Uppercase user ID no display name.
            assert!(msg_starts_with_mention(uppercase_user_id, None, &message,));
            assert_eq!(remove_bot_name(uppercase_user_id, None, &message), content);
        }

        let matching_display_name_prefixes =
            &["the hebbot: ", "THE HEBBOT: ", "the hebbot ", "THE HEBBOT "];

        for prefix in matching_display_name_prefixes {
            let message = format!("{prefix}{content}");

            // Log the message for debugging when the check fails.
            println!("Checking message: `{message}`");

            // Lowercase user ID and display name.
            assert!(msg_starts_with_mention(
                lowercase_user_id,
                Some(lowercase_display_name.clone()),
                &message
            ));
            assert_eq!(
                remove_bot_name(
                    lowercase_user_id,
                    Some(lowercase_display_name.clone()),
                    &message,
                ),
                content
            );

            // Lowercase user ID no display name.
            assert!(!msg_starts_with_mention(lowercase_user_id, None, &message,));
            assert_eq!(remove_bot_name(lowercase_user_id, None, &message), message);

            // Uppercase user ID and display name.
            assert!(msg_starts_with_mention(
                uppercase_user_id,
                Some(uppercase_display_name.clone()),
                &message
            ));
            assert_eq!(
                remove_bot_name(
                    uppercase_user_id,
                    Some(uppercase_display_name.clone()),
                    &message,
                ),
                content
            );

            // Uppercase user ID no display name.
            assert!(!msg_starts_with_mention(uppercase_user_id, None, &message,));
            assert_eq!(remove_bot_name(uppercase_user_id, None, &message), message);
        }

        let not_matching_prefixes = &[
            "[hebbot] ",
            "[@hebbot] ",
            "[HEBBOT] ",
            "[the hebbot] ",
            "[THE HEBBOT] ",
            "heb bot ",
            "@the hebbot",
        ];

        for prefix in not_matching_prefixes {
            let message = format!("{prefix}{content}");

            // Log the message for debugging when the check fails.
            println!("Checking message: `{message}`");

            // Lowercase user ID and display name.
            assert!(!msg_starts_with_mention(
                lowercase_user_id,
                Some(lowercase_display_name.clone()),
                &message
            ));
            assert_eq!(
                remove_bot_name(
                    lowercase_user_id,
                    Some(lowercase_display_name.clone()),
                    &message,
                ),
                message
            );

            // Lowercase user ID no display name.
            assert!(!msg_starts_with_mention(lowercase_user_id, None, &message,));
            assert_eq!(remove_bot_name(lowercase_user_id, None, &message), message);

            // Uppercase user ID and display name.
            assert!(!msg_starts_with_mention(
                uppercase_user_id,
                Some(uppercase_display_name.clone()),
                &message
            ));
            assert_eq!(
                remove_bot_name(
                    uppercase_user_id,
                    Some(uppercase_display_name.clone()),
                    &message,
                ),
                message
            );

            // Uppercase user ID no display name.
            assert!(!msg_starts_with_mention(uppercase_user_id, None, &message,));
            assert_eq!(remove_bot_name(uppercase_user_id, None, &message), message);
        }
    }
}
