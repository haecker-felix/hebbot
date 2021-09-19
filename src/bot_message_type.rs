#[derive(Clone, Debug, PartialEq)]
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
