pub mod login {
    use macros::Packet;

    #[derive(Packet)]
    #[packet(0x02, crate::LOGIN_STATE, false)]
    pub struct LoginSuccess {
        uuid: u128,
        username: String
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