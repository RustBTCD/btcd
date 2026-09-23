//! When each message is valid on a connection.

use super::Message;

/// The part of a connection's life in which a message may be sent or received.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// The `version` and `verack` exchange itself.
    Handshake,
    /// Only between the peer's `version` and its `verack`.
    Negotiation,
    /// From the peer's `version` onwards.
    NegotiationOrEstablished,
    /// Only after the handshake completes.
    Established,
}

impl Window {
    /// Whether a message with this window may arrive before the handshake completes.
    pub fn allowed_during_negotiation(self) -> bool {
        matches!(self, Window::Negotiation | Window::NegotiationOrEstablished)
    }
}

/// The window a message belongs to.
pub fn window(message: &Message) -> Window {
    match message {
        Message::Version(_) | Message::Verack => Window::Handshake,
        Message::WtxidRelay
        | Message::SendAddrV2
        | Message::Feature(_)
        | Message::SendTxRcnCl(_) => Window::Negotiation,
        Message::SendHeaders | Message::SendCmpct(_) => Window::NegotiationOrEstablished,
        _ => Window::Established,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::validate::tests::{feature, version};

    #[test]
    fn windows_follow_the_message_catalogue() {
        assert_eq!(window(&Message::Version(version())), Window::Handshake);
        assert_eq!(window(&Message::Verack), Window::Handshake);
        assert_eq!(window(&Message::WtxidRelay), Window::Negotiation);
        assert_eq!(window(&Message::SendAddrV2), Window::Negotiation);
        assert_eq!(window(&Message::Feature(feature())), Window::Negotiation);
        assert_eq!(
            window(&Message::SendHeaders),
            Window::NegotiationOrEstablished
        );
        assert_eq!(window(&Message::GetAddr), Window::Established);
        assert_eq!(
            window(&Message::Ping(p2p::message::Ping::new(1))),
            Window::Established
        );
    }

    #[test]
    fn negotiation_messages_are_allowed_before_verack() {
        assert!(Window::Negotiation.allowed_during_negotiation());
        assert!(Window::NegotiationOrEstablished.allowed_during_negotiation());
        assert!(!Window::Established.allowed_during_negotiation());
        assert!(!Window::Handshake.allowed_during_negotiation());
    }
}
