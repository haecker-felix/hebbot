use matrix_sdk::events::room::message::MessageEventContent;
use matrix_sdk::room::Room;
use matrix_sdk::BaseRoomMember;
use matrix_sdk::RoomMember;
use ruma::events::room::message::MessageType;
use ruma::events::room::message::TextMessageEventContent;
use ruma::events::SyncMessageEvent;
use ruma::UserId;

/// A simplified way of getting a the acutal message from text message event
pub fn get_text_msg_body(event: &SyncMessageEvent<MessageEventContent>) -> Option<String> {
    if let SyncMessageEvent {
        content:
            MessageEventContent {
                msgtype: MessageType::Text(TextMessageEventContent { body: msg_body, .. }),
                ..
            },
        ..
    } = event
    {
        return Some(msg_body.to_owned());
    }
    None
}

/// Gets the sender RoomMember from a message event
pub async fn get_msg_sender(
    room: &Room,
    event: &SyncMessageEvent<MessageEventContent>,
) -> RoomMember {
    room.get_member(&event.sender).await.unwrap().unwrap()
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
