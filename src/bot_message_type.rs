#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BotMessageType {
    AdminRoomHtmlNotice,
    AdminRoomPlainNotice,
    ReportingRoomPlainText,
    ReportingRoomPlainNotice,
}
