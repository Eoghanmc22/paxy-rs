use crate::packets::Packet;
use std::collections::HashMap;
use bytes::Buf;

//represents a packet that is decompressed, decrypted, and has a known id
pub struct UnparsedPacket<T: Buf> {
    id: i32,
    buf: T
}

impl<T: Buf> UnparsedPacket<T> {
    pub fn new(id: i32, buf: T) -> UnparsedPacket<T> {
        UnparsedPacket { id, buf }
    }
}

pub struct HandlingContext {
    inbound_packets: HashMap<i32, Box<dyn Fn(&mut dyn Buf) -> Box<dyn Packet> + Send + Sync>>,
    outbound_packets: HashMap<i32, Box<dyn Fn(&mut dyn Buf) -> Box<dyn Packet> + Send + Sync>>,

    inbound_transformers: HashMap<i32, Vec<Box<dyn Fn(&mut dyn Packet) + Send + Sync>>>,
    outbound_transformers: HashMap<i32, Vec<Box<dyn Fn(&mut dyn Packet) + Send + Sync>>>,
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

    pub fn handle_inbound_packet(&self, mut packet: UnparsedPacket<&[u8]>) -> Option<Vec<u8>> {
        let id = packet.id;
        let packet_supplier = if let Some(t) = self.inbound_packets.get(&id) {
            t
        } else { return None; };
        let transformers = if let Some(t) = self.inbound_transformers.get(&id) {
            t
        } else { return None; };

        let mut packet: Box<dyn Packet> = packet_supplier(&mut packet.buf);

        for transformer in transformers.iter() {
            transformer(&mut *packet);
        }

        let mut buffer: Vec<u8> = Vec::new();
        packet.write(&mut buffer);

        Some(buffer)
    }

    pub fn handle_outbound_packet(&self, mut packet: UnparsedPacket<&[u8]>) -> Option<Vec<u8>> {
        let id = packet.id;
        let packet_supplier = if let Some(t) = self.outbound_packets.get(&id) {
            t
        } else { return None; };
        let transformers = if let Some(t) = self.outbound_transformers.get(&id) {
            t
        } else { return None; };

        let mut packet: Box<dyn Packet> = packet_supplier(&mut packet.buf);

        for transformer in transformers.iter() {
            transformer(&mut *packet);
        }

        let mut buffer: Vec<u8> = Vec::new();
        packet.write(&mut buffer);

        Some(buffer)
    }

    pub fn register_inbound_packet_supplier<P: Packet, F: 'static + Fn(&mut dyn Buf) -> P + Send + Sync>(&mut self, transformer: F) {
        if !P::is_inbound() {
            panic!("bad inbound_packet_supplier");
        }
        let packet_id = P::get_id();
        self.inbound_packets.insert(packet_id, Box::new(move |buf| Box::new(transformer(buf))));
    }

    pub fn register_outbound_packet_supplier<P: Packet, F: 'static + Fn(&mut dyn Buf) -> Box<P> + Send + Sync>(&mut self, transformer: F) {
        if P::is_inbound() {
            panic!("bad outbound_packet_supplier");
        }
        let packet_id = P::get_id();
        self.outbound_packets.insert(packet_id, Box::new(move |buf| transformer(buf)));
    }

    pub fn register_inbound_transformer<P: Packet, F: 'static + Fn(&mut Box<P>) + Send + Sync>(&mut self, transformer: F) {
        if !P::is_inbound() {
            panic!("bad inbound_transformer");
        }
        let packet_id = P::get_id();
        let transformer : Box<dyn Fn(&mut dyn Packet) + Send + Sync> = Box::new(move |packet| {
            let any_packet = packet.as_any();
            if let Some(casted_packet) = any_packet.downcast_mut() {
                transformer(casted_packet)
            } else {
                println!("couldnt cast, this should never be hit ever");
            }
        });
        if let Some(vec) = self.inbound_transformers.get_mut(&packet_id) {
            vec.push(transformer);
        } else {
            self.inbound_transformers.insert(packet_id, vec![transformer]);
        }
    }

    pub fn register_outbound_transformer<P: Packet, F: 'static + Fn(&mut P) + Send + Sync>(&mut self, transformer: F) {
        if P::is_inbound() {
            panic!("bad outbound_transformer");
        }
        let packet_id = P::get_id();
        let transformer : Box<dyn Fn(&mut dyn Packet) + Send + Sync> = Box::new(move |packet| {
            let any_packet = packet.as_any();
            if let Some(casted_packet) = any_packet.downcast_mut() {
                transformer(casted_packet)
            } else {
                println!("couldnt cast, this should never be hit ever");
            }
        });
        if let Some(vec) = self.outbound_transformers.get_mut(&packet_id) {
            vec.push(transformer);
        } else {
            self.outbound_transformers.insert(packet_id, vec![transformer]);
        }
    }
}