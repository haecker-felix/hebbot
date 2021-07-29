use matrix_sdk::room::Room;
use matrix_sdk::BaseRoomMember;
use regex::Regex;
use ruma::api::client::r0::room::get_room_event::Request;
use ruma::events::room::message::MessageEventContent;
use ruma::events::room::message::MessageType;
use ruma::events::room::message::Relation;
use ruma::events::room::message::Replacement;
use ruma::events::room::message::TextMessageEventContent;
use ruma::events::AnyMessageEvent;
use ruma::events::AnyRoomEvent;
use ruma::events::MessageEvent;
use ruma::events::SyncMessageEvent;
use ruma::EventId;
use ruma::UserId;

/// Get room message by event id
pub async fn room_event_by_id(room: &Room, event_id: &EventId) -> Option<AnyRoomEvent> {
    let request = Request::new(room.room_id(), &event_id);
    let event = room.event(request).await.ok()?.event.deserialize().ok()?;

    Some(event)
}

pub async fn message_type(room_event: &AnyRoomEvent) -> Option<MessageType> {
    if let AnyRoomEvent::Message(AnyMessageEvent::RoomMessage(MessageEvent {
        content: MessageEventContent {
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
pub fn get_message_event_text(event: &SyncMessageEvent<MessageEventContent>) -> Option<String> {
    if let SyncMessageEvent {
        content:
            MessageEventContent {
                msgtype: MessageType::Text(TextMessageEventContent { body: msg_body, .. }),
                relates_to: None,
                ..
            },
        ..
    } = event
    {
        return Some(msg_body.to_owned());
    }
    None
}

/// A simplified way of getting an edited message
pub fn get_edited_message_event_text(
    event: &SyncMessageEvent<MessageEventContent>,
) -> Option<(EventId, String)> {
    if let SyncMessageEvent {
        content:
            MessageEventContent {
                relates_to:
                    Some(Relation::Replacement {
                        0:
                            Replacement {
                                event_id,
                                new_content,
                                ..
                            },
                        ..
                    }),
                ..
            },
        ..
    } = event
    {
        if let content = new_content {
            if let MessageEventContent {
                msgtype: MessageType::Text(TextMessageEventContent { body: msg_body, .. }),
                relates_to: None,
                ..
            } = &**content
            {
                return Some((event_id.clone(), msg_body.to_string()));
            }
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
    let message = re.replace(&message, "");
    message.trim().to_string()
}
