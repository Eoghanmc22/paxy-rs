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

pub mod status {
    use macros::Packet;

    #[derive(Packet)]
    #[packet(0x00, crate::STATUS_STATE, true)]
    pub struct Request {

    }

    #[derive(Packet)]
    #[packet(0x01, crate::STATUS_STATE, true)]
    pub struct Ping {
        pub payload: i64
    }
}

pub mod login {
    use macros::Packet;

    #[derive(Packet)]
    #[packet(0x00, crate::LOGIN_STATE, true)]
    pub struct LoginStart {
        name: String
    }
}