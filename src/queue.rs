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

pub struct Manager<'a> {
    pub queues: std::collections::HashMap<u64, Item<'a>>,
    pub queue_buffer_capacity: usize,
    pub packet_interval: u128,
    pub shutting_down: bool,
    pub queue_linked: std::collections::LinkedList<Item<'a>>,
}

impl Manager<'_> {
    pub fn new<'a>(queue_buffer_capacity: usize, packet_interval: u128) -> Manager<'a> {
        Manager {
            queues: Default::default(),
            queue_buffer_capacity,
            packet_interval,
            shutting_down: false,
            queue_linked: Default::default(),
        }
    }
}
