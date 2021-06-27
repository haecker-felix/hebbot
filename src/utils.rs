use matrix_sdk::events::room::message::MessageEventContent;
use matrix_sdk::BaseRoomMember;
use ruma::events::room::message::MessageType;
use ruma::events::room::message::Relation;
use ruma::events::room::message::TextMessageEventContent;
use ruma::events::room::relationships::Replacement;
use ruma::events::SyncMessageEvent;
use ruma::EventId;
use ruma::UserId;

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
                        0: Replacement { event_id, .. },
                        ..
                    }),
                new_content,
                ..
            },
        ..
    } = event
    {
        if let Some(content) = new_content {
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
    let localpart = user_id.localpart();
    // Catch "@botname ..." messages
    let msg = msg.replace(&format!("@{}", localpart), localpart);
    msg.as_str().starts_with(localpart)
}

pub fn summary(message: &str) -> String {
    if message.len() > 60 {
        format!("{} ...", message.split_at(50).0)
    } else {
        message.to_string()
    }
}
