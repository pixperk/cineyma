#[derive(Debug, PartialEq, Eq)]
pub enum MailboxError {
    ///Actor's mailbox is closed (actor has stopped)
    MailboxClosed,
    ///Requested operation timed out
    Timeout,
}

impl std::fmt::Display for MailboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MailboxError::MailboxClosed => write!(f, "Actor's mailbox is closed"),
            MailboxError::Timeout => write!(f, "Requested operation timed out"),
        }
    }
}

impl std::error::Error for MailboxError {}
