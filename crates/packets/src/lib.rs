pub mod c2s;
pub mod s2c;

pub const HANDSHAKING_STATE: u8 = 0;
pub const STATUS_STATE: u8 = 1;
pub const LOGIN_STATE: u8 = 2;
pub const PLAY_STATE: u8 = 3;