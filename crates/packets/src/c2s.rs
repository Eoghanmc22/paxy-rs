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
        // this packet has no fields
    }

    #[derive(Packet)]
    #[packet(0x01, crate::STATUS_STATE, true)]
    pub struct Ping {
        pub payload: i64
    }
}

pub mod login {
    use macros::Packet;
    use utils::indexed_vec::IndexedVec;
    use utils::sendable::{Vari32, InferLenVec};

    #[derive(Packet)]
    #[packet(0x00, crate::LOGIN_STATE, true)]
    pub struct LoginStart {
        name: String
    }

    #[derive(Packet)]
    #[packet(0x01, crate::LOGIN_STATE, true)]
    pub struct EncryptionResponse {
        pub shared_secret: IndexedVec<u8>,
        pub verify_token: IndexedVec<u8>
    }

    #[derive(Packet)]
    #[packet(0x02, crate::LOGIN_STATE, true)]
    pub struct LoginPluginResponse {
        pub message_id: Vari32,
        pub successful: bool,
        pub data: InferLenVec
    }
}

pub mod play {
    use macros::Packet;
    use utils::sendable::InferLenVec;

    #[derive(Packet)]
    #[packet(0x0B, crate::PLAY_STATE, true)]
    pub struct PluginMessage {
        pub channel: String,
        pub data: InferLenVec
    }
}