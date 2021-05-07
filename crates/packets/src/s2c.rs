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
    use utils::sendable::Vari32;

    #[derive(Packet)]
    #[packet(0x00, crate::LOGIN_STATE, false)]
    pub struct Disconnect {
        pub reason: String
    }

    #[derive(Packet)]
    #[packet(0x01, crate::LOGIN_STATE, false)]
    pub struct EncryptionRequest {
        pub server_id: String,
        pub public_key: Vec<u8>,
        pub verify_token: Vec<u8>,
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
}

pub mod play {
    use macros::Packet;
    use utils::sendable::Vari32;

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