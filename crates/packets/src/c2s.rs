pub mod handshake {
    use macros::Packet;
    use utils::sendable::Vari32;

    #[derive(Packet)]
    #[packet(0x00, crate::HANDSHAKING_STATE, true)]
    pub struct HandshakePacket {
        pub protocol_version: Vari32,
        pub ip: String,
        pub port: u16,
        pub next_state: Vari32
    }
}