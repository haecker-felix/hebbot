use matrix_sdk::events::{room::message::MessageEventContent, AnyMessageEventContent};
use matrix_sdk::room::Joined;
use matrix_sdk::room::Room;
use matrix_sdk::Client;
use matrix_sdk::EventHandler;
use matrix_sdk::RoomMember;
use matrix_sdk::SyncSettings;
use matrix_sdk_common::uuid::Uuid;
use ruma::events::SyncMessageEvent;
use ruma::EventId;
use ruma::RoomId;
use ruma::UserId;

use std::convert::TryFrom;

use crate::config::Config;
use crate::utils;

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

        //bot.send_message("Started hebbot service!", true).await;
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
        if let Room::Joined(ref _joined) = room {
            if let Some(text) = utils::get_text_msg_body(event) {
                let member = utils::get_msg_sender(&room, event).await;

                // Reporting room
                if room.room_id() == self.0.reporting_room.room_id() {
                    self.on_reporting_room_message(text, member, &event.event_id)
                        .await;
                }
                // Admin room
                if room.room_id() == self.0.admin_room.room_id() {
                    //self.on_admin_room_message(text);
                }
            }
        }
    }
}

impl EventCallback {
    async fn on_reporting_room_message(&self, msg: String, member: RoomMember, event_id: &EventId) {
        // We're going to ignore all messages, expect it mentions the bot at the beginning
        let bot_id = self.0.client.user_id().await.unwrap();
        if !utils::msg_starts_with_mention(bot_id, msg.clone()) {
            return;
        }

        let member_name = utils::get_member_display_name(&member);
        debug!("received {:?} {:?}", msg, event_id.to_string());

        let msg = format!("Hello {} I received your message!", member_name);
        self.0.send_message(&msg, false).await;
    }
}
