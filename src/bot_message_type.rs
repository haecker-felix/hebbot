#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BotMessageType {
    AdminRoomHtmlText,
    AdminRoomPlainText,
    AdminRoomHtmlNotice,
    AdminRoomPlainNotice,
    ReportingRoomHtmlText,
    ReportingRoomPlainText,
    ReportingRoomHtmlNotice,
    ReportingRoomPlainNotice,
}
