use std::collections::HashMap;

use bytes::Buf;

use crate::contexts::{ConnectionContext, NetworkThreadContext};
use crate::packets::Packet;
use crate::utils::IndexedVec;

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

    inbound_transformers: HashMap<i32, Vec<Box<dyn Fn(&NetworkThreadContext, &mut ConnectionContext, &mut dyn Packet) + Send + Sync>>>,
    outbound_transformers: HashMap<i32, Vec<Box<dyn Fn(&NetworkThreadContext, &mut ConnectionContext, &mut dyn Packet) + Send + Sync>>>,
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

    pub fn handle_inbound_packet(&self, thread_ctx: &NetworkThreadContext, connection_ctx: &mut ConnectionContext, mut packet: UnparsedPacket<&[u8]>) -> Option<IndexedVec<u8>> {
        let id = packet.id;
        let packet_supplier = if let Some(t) = self.inbound_packets.get(&id) {
            t
        } else { return None; };
        let transformers = if let Some(t) = self.inbound_transformers.get(&id) {
            t
        } else { return None; };

        let mut packet: Box<dyn Packet> = packet_supplier(&mut packet.buf);

        for transformer in transformers.iter() {
            transformer(thread_ctx, connection_ctx, &mut *packet);
        }

        let mut buffer: IndexedVec<u8> = IndexedVec::new();
        packet.write(&mut buffer);

        Some(buffer)
    }

    pub fn handle_outbound_packet(&self, thread_ctx: &NetworkThreadContext, connection_ctx: &mut ConnectionContext, mut packet: UnparsedPacket<&[u8]>) -> Option<IndexedVec<u8>> {
        let id = packet.id;
        let packet_supplier = if let Some(t) = self.outbound_packets.get(&id) {
            t
        } else { return None; };
        let transformers = if let Some(t) = self.outbound_transformers.get(&id) {
            t
        } else { return None; };

        let mut packet: Box<dyn Packet> = packet_supplier(&mut packet.buf);

        for transformer in transformers.iter() {
            transformer(thread_ctx, connection_ctx, &mut *packet);
        }

        let mut buffer: IndexedVec<u8> = IndexedVec::new();
        packet.write(&mut buffer);

        Some(buffer)
    }

    pub fn register_packet_supplier<P: Packet, F: 'static + Fn(&mut dyn Buf) -> P + Send + Sync>(&mut self, transformer: F) {
        let packet_id = P::get_id();
        if P::is_inbound() {
            self.inbound_packets.insert(packet_id, Box::new(move |buf| Box::new(transformer(buf))));
        } else {
            self.outbound_packets.insert(packet_id, Box::new(move |buf| Box::new(transformer(buf))));
        }
    }

    pub fn register_transformer<P: Packet, F: 'static + Fn(&NetworkThreadContext, &mut ConnectionContext, &mut P) + Send + Sync>(&mut self, transformer: F) {
        let packet_id = P::get_id();

        let transformer : Box<dyn Fn(&NetworkThreadContext, &mut ConnectionContext, &mut dyn Packet) + Send + Sync> = Box::new(move |thread_ctx, connection_ctx, packet| {
            let any_packet = packet.as_any();
            if let Some(casted_packet) = any_packet.downcast_mut() {
                transformer(thread_ctx, connection_ctx, casted_packet)
            } else {
                println!("couldnt cast, this should never be hit ever");
            }
        });

        if P::is_inbound() {
            if let Some(vec) = self.inbound_transformers.get_mut(&packet_id) {
                vec.push(transformer);
            } else {
                self.inbound_transformers.insert(packet_id, vec![transformer]);
            }
        } else {
            if let Some(vec) = self.outbound_transformers.get_mut(&packet_id) {
                vec.push(transformer);
            } else {
                self.outbound_transformers.insert(packet_id, vec![transformer]);
            }
        }
    }
}