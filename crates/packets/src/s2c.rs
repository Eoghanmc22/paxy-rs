pub mod status {
    use macros::Packet;

    #[derive(Packet)]
    #[packet(0x00, crate::STATUS_STATE, false)]
    pub struct Response {
        pub json: String
    }

    #[derive(Packet)]
    #[packet(0x01, crate::STATUS_STATE, false)]
    pub struct Pong {
        pub payload: i64
    }
}

pub mod login {
    use macros::Packet;
    use utils::sendable::{Vari32, InferLenVec};
    use utils::indexed_vec::IndexedVec;

    #[derive(Packet)]
    #[packet(0x00, crate::LOGIN_STATE, false)]
    pub struct Disconnect {
        pub reason: String
    }

    #[derive(Packet)]
    #[packet(0x01, crate::LOGIN_STATE, false)]
    pub struct EncryptionRequest {
        pub server_id: String,
        pub public_key: IndexedVec<u8>,
        pub verify_token: IndexedVec<u8>,
    }

    #[derive(Packet)]
    #[packet(0x02, crate::LOGIN_STATE, false)]
    pub struct LoginSuccess {
        pub uuid: u128,
        pub username: String
    }

    #[derive(Packet)]
    #[packet(0x03, crate::LOGIN_STATE, false)]
    pub struct SetCompression {
        pub threshold: Vari32,
    }

    #[derive(Packet)]
    #[packet(0x04, crate::LOGIN_STATE, false)]
    pub struct LoginPluginRequest {
        pub message_id: Vari32,
        pub channel: String,
        pub data: InferLenVec
    }
}

pub mod play {
    use macros::Packet;
    use utils::sendable::{Vari32, InferLenVec};

    #[derive(Packet)]
    #[packet(0x17, crate::PLAY_STATE, false)]
    pub struct PluginMessage {
        pub channel: String,
        pub data: InferLenVec
    }

    #[derive(Packet)]
    #[packet(0x27, crate::PLAY_STATE, false)]
    pub struct EntityPositionPacket {
        pub entity_id: Vari32,
        pub delta_x: i16,
        pub delta_y: i16,
        pub delta_z: i16,
        pub on_ground: bool
    }
}