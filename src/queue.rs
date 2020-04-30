use crate::packet;

pub struct Buffer {
    pub index: usize,
    pub size: usize,
    pub capacity: usize,
}

impl Clone for Buffer {
    fn clone(&self) -> Self {
        Self {
            index: self.index,
            size: self.size,
            capacity: self.capacity,
        }
    }
}

pub struct Item<'a> {
    pub next_due_time: u128,
    pub packet_buffer: Vec<packet::Queued>,
    pub buffer: Buffer,
    pub address: Vec<dns_lookup::AddrInfo>,
    pub explicit_socket: Option<&'a mut tokio::net::UdpSocket>,
}

impl Clone for Item<'_> {
    fn clone(&self) -> Self {
        Self {
            next_due_time: self.next_due_time,
            packet_buffer: self.packet_buffer.clone(),
            buffer: self.buffer.clone(),
            address: self.address.clone(),
            explicit_socket: None,
        }
    }
}

pub struct Manager<'a> {
    pub queues: linked_hash_map::LinkedHashMap<u64, Item<'a>>,
    pub queue_buffer_capacity: usize,
    pub packet_interval: u128,
    pub shutting_down: bool,
}

impl Manager<'_> {
    pub fn new<'a>(queue_buffer_capacity: usize, packet_interval: u128) -> Manager<'a> {
        Manager {
            queues: linked_hash_map::LinkedHashMap::new(),
            queue_buffer_capacity,
            packet_interval,
            shutting_down: false,
        }
    }
}
