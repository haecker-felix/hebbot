use matrix_sdk::events::{room::message::MessageEventContent, AnyMessageEventContent};
use matrix_sdk::room::Joined;
use matrix_sdk::room::Room;
use matrix_sdk::BaseRoomMember;
use matrix_sdk::Client;
use matrix_sdk::EventHandler;
use matrix_sdk::RoomMember;
use matrix_sdk::SyncSettings;
use matrix_sdk_common::uuid::Uuid;
use ruma::events::room::message::MessageType;
use ruma::events::room::message::TextMessageEventContent;
use ruma::events::SyncMessageEvent;
use ruma::RoomId;
use ruma::UserId;

use std::convert::TryFrom;

use crate::config::Config;

#[derive(Clone)]
pub struct Bot {
    config: Config,
    client: Client,
    reporting_room: Joined,
    admin_room: Joined,
}

impl Bot {
    pub async fn run() {
        let config = Config::read();

        let username = config.bot_user_id.as_str();
        let user = UserId::try_from(username).expect("Unable to parse bot user id");
        let client = Client::new_from_user_id(user.clone()).await.unwrap();

        Self::login(&client, user.localpart(), &config.bot_password).await;

        // Get matrix rooms
        let reporting_room_id = RoomId::try_from(config.reporting_room_id.as_str()).unwrap();
        let reporting_room = client
            .get_joined_room(&reporting_room_id)
            .expect("Unable to get reporting room");

        let admin_room_id = RoomId::try_from(config.admin_room_id.as_str()).unwrap();
        let admin_room = client
            .get_joined_room(&admin_room_id)
            .expect("Unable to get admin room");

        let bot = Self {
            config,
            client,
            reporting_room,
            admin_room,
        };

        bot.send_message("Started hebbot service!", true).await;
        let handler = Box::new(EventCallback(bot.clone()));
        bot.client.set_event_handler(handler).await;

        info!("Start syncing...");
        bot.client.sync(SyncSettings::new()).await;
    }

    async fn login(client: &Client, user: &str, pwd: &str) {
        info!("Logging in...");
        let response = client
            .login(user, pwd, None, Some("hebbot"))
            .await
            .expect("Unable to login");

        info!("Do initial sync...");
        client
            .sync_once(SyncSettings::new())
            .await
            .expect("Unable to sync");

        info!(
            "Logged in as {}, got device_id {} and access_token {}",
            response.user_id, response.device_id, response.access_token
        );
    }

    async fn send_message(&self, msg: &str, admin_room: bool) {
        let content = AnyMessageEventContent::RoomMessage(MessageEventContent::text_plain(msg));
        let txn_id = Uuid::new_v4();

        let room = if admin_room {
            &self.admin_room
        } else {
            &self.reporting_room
        };

        room.send(content, Some(txn_id))
            .await
            .expect("Unable to send message");
    }
}

struct EventCallback(Bot);

#[async_trait::async_trait]
impl EventHandler for EventCallback {
    async fn on_room_message(&self, room: Room, event: &SyncMessageEvent<MessageEventContent>) {
        if let Room::Joined(ref joined) = room {
            if let Some(text) = Self::get_text_msg_body(event) {
                if !self.mentions_bot(text.clone()).await {
                    return;
                }

                let member = Self::get_msg_sender(&room, event).await;
                let member_name = Self::get_member_display_name(&member);
                debug!("received {:?}", text);

                let msg = format!("Hello {} I received your message!", member_name);
                self.0.send_message(&msg, false).await;
            }
        }
    }
}

impl EventCallback {
    fn get_text_msg_body(event: &SyncMessageEvent<MessageEventContent>) -> Option<String> {
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

    async fn get_msg_sender(
        room: &Room,
        event: &SyncMessageEvent<MessageEventContent>,
    ) -> RoomMember {
        room.get_member(&event.sender).await.unwrap().unwrap()
    }

    fn get_member_display_name(member: &BaseRoomMember) -> String {
        member
            .display_name()
            .unwrap_or_else(|| member.user_id().as_str())
            .to_string()
    }

    async fn mentions_bot(&self, msg: String) -> bool {
        let bot_id = self.0.client.user_id().await.unwrap();
        let localpart = bot_id.localpart();
        // Catch "@botname ..." messages
        let msg = msg.replace(&format!("@{}", localpart), localpart);
        msg.as_str().starts_with(localpart)
    }
}
