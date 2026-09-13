use std::fmt;

/// The observed cause that led the server to dispose of a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisconnectReason {
    /// An outbound client transport could not accept or write a frame.
    TransportWriteFailed,
    /// The client-side transport ended or reported that its peer was gone.
    PeerClosed,
    /// An existing mobile or relay connection was retired for a replacement.
    Replaced,
    /// The host is disposing its remaining clients as part of shutdown.
    HostShutdown,
    /// An unauthenticated client reached a path that disposes it.
    Unauthenticated,
    /// An authenticated client sent a malformed request that has no response target.
    ProtocolViolation,
    /// A paired mobile device was revoked while its client was connected.
    DeviceRevoked,
}

impl DisconnectReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::TransportWriteFailed => "transport_write_failed",
            Self::PeerClosed => "peer_closed",
            Self::Replaced => "replaced",
            Self::HostShutdown => "host_shutdown",
            Self::Unauthenticated => "unauthenticated",
            Self::ProtocolViolation => "protocol_violation",
            Self::DeviceRevoked => "device_revoked",
        }
    }
}

impl fmt::Display for DisconnectReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_names_are_stable_for_structured_logs() {
        assert_eq!(
            DisconnectReason::TransportWriteFailed.as_str(),
            "transport_write_failed"
        );
        assert_eq!(DisconnectReason::PeerClosed.as_str(), "peer_closed");
        assert_eq!(DisconnectReason::Replaced.as_str(), "replaced");
        assert_eq!(DisconnectReason::HostShutdown.as_str(), "host_shutdown");
        assert_eq!(
            DisconnectReason::Unauthenticated.as_str(),
            "unauthenticated"
        );
        assert_eq!(
            DisconnectReason::ProtocolViolation.as_str(),
            "protocol_violation"
        );
        assert_eq!(DisconnectReason::DeviceRevoked.as_str(), "device_revoked");
    }
}
