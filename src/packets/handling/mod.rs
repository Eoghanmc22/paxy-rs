use crate::packets::Packet;
use std::collections::HashMap;
use bytes::{BytesMut, Buf};
use std::any::Any;
use crate::utils::VarInts;

// buffer, end
pub fn get_valid_data(buffer: &mut BytesMut) -> Option<BytesMut> {
    let len = buffer.remaining();
    if len < 3 {
        return None;
    }

    let expected_length = if let Some((num, _bytes)) = buffer.get_var_i32_limit(3) {
        num as usize
    } else {
        return None;
    };
    buffer.advance(expected_length);
    return Some(BytesMut::from(&buffer[..expected_length]));
}

//represents a packet that is decompressed, decrypted, and has a known id
pub struct UnparsedPacket {
    id: i32,
    buf: BytesMut
}

impl UnparsedPacket {
    pub fn new(id: i32, buf: BytesMut) -> UnparsedPacket {
        UnparsedPacket { id, buf }
    }
}

pub struct HandlingContext {
    inbound_packets: HashMap<i32, Box<dyn Fn(&mut BytesMut) -> Box<dyn Packet> + Send + Sync>>,
    outbound_packets: HashMap<i32, Box<dyn Fn(&mut BytesMut) -> Box<dyn Packet> + Send + Sync>>,

    inbound_transformers: HashMap<i32, Vec<Box<dyn Fn(&mut Box<dyn Packet>) + Send + Sync>>>,
    outbound_transformers: HashMap<i32, Vec<Box<dyn Fn(&mut Box<dyn Packet>) + Send + Sync>>>
}

impl HandlingContext {
    pub fn new() -> HandlingContext {
        HandlingContext {
            inbound_packets: HashMap::new(),
            outbound_packets: HashMap::new(),
            inbound_transformers: HashMap::new(),
            outbound_transformers: HashMap::new()
        }
    }

    pub fn handle_inbound_packet(&self, mut packet: UnparsedPacket) -> BytesMut {
        let id = packet.id;
        let packet_supplier = if let Some(t) = self.inbound_packets.get(&id) {
            t
        } else { return packet.buf; };
        let transformers = if let Some(t) = self.inbound_transformers.get(&id) {
            t
        } else { return packet.buf; };

        let mut packet: Box<dyn Packet> = packet_supplier(&mut packet.buf);

        for transformer in transformers.iter() {
            transformer(&mut packet);
        }

        let mut buffer = BytesMut::new();
        packet.write(&mut buffer);

        buffer
    }

    pub fn handle_outbound_packet(&self, mut packet: UnparsedPacket) -> BytesMut {
        let id = packet.id;
        let packet_supplier = if let Some(t) = self.outbound_packets.get(&id) {
            t
        } else { return packet.buf; };
        let transformers = if let Some(t) = self.outbound_transformers.get(&id) {
            t
        } else { return packet.buf; };

        let mut packet: Box<dyn Packet> = packet_supplier(&mut packet.buf);

        for transformer in transformers.iter() {
            transformer(&mut packet);
        }

        let mut buffer = BytesMut::new();
        packet.write(&mut buffer);

        buffer
    }

    pub fn register_inbound_transformer<P: Packet, F: 'static + Fn(&mut P) + Send + Sync>(&mut self, transformer: F) {
        let packet_id = P::get_id();
        let transformer : Box<dyn Fn(&mut Box<dyn Packet>) + Send + Sync> = Box::new(move |packet| {
            let any = packet as &mut dyn Any;
            if let Some(casted_packet) = any.downcast_mut() {
                transformer(casted_packet)
            }
        });
        if let Some(vec) = self.inbound_transformers.get_mut(&packet_id) {
            vec.push(transformer);
        } else {
            self.inbound_transformers.insert(packet_id, vec![transformer]);
        }
    }

    pub fn register_outbound_transformer<P: Packet, F: 'static + Fn(&mut P) + Send + Sync>(&mut self, transformer: F) {
        let packet_id = P::get_id();
        let transformer : Box<dyn Fn(&mut Box<dyn Packet>) + Send + Sync> = Box::new(move |packet| {
            let any = packet as &mut dyn Any;
            if let Some(casted_packet) = any.downcast_mut() {
                transformer(casted_packet)
            }
        });
        if let Some(vec) = self.outbound_transformers.get_mut(&packet_id) {
            vec.push(transformer);
        } else {
            self.outbound_transformers.insert(packet_id, vec![transformer]);
        }
    }
}