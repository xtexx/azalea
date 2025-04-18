use azalea_buf::AzBuf;
use azalea_chat::FormattedText;

#[derive(Clone, Debug, AzBuf)]
pub struct ServerLinkEntry {
    pub kind: ServerLinkKind,
    pub link: String,
}

#[derive(Clone, Debug, AzBuf)]
pub enum ServerLinkKind {
    Component(FormattedText),
    Known(KnownLinkKind),
}

#[derive(Clone, Copy, Debug, AzBuf)]
pub enum KnownLinkKind {
    BugReport,
    CommunityGuidelines,
    Support,
    Status,
    Feedback,
    Community,
    Website,
    Forums,
    News,
    Announcements,
}
