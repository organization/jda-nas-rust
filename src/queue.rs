use std::sync::Mutex;

use crate::packet;

pub struct Buffer {
    pub index: usize,
    pub size: usize,
    pub capacity: usize,
}

pub struct Item {
    pub next_due_time: i64,
    pub packet_buffer: Vec<packet::Queued>,
    pub buffer: Buffer,
    pub address: Vec<dns_lookup::AddrInfo>,
    pub explicit_socket: tokio::net::UdpSocket,
}

pub struct Manager {
    queues: std::collections::HashMap<u64, Item>,
    queue_buffer_capacity: usize,
    packet_interval: i64,
    shutting_down: bool,
    queue_linked: std::collections::LinkedList<Item>,
}

impl Manager {
    pub fn new(
        queue_buffer_capacity: usize,
        packet_interval: i64,
    ) -> Manager {
        Manager {
            queues: Default::default(),
            queue_buffer_capacity,
            packet_interval,
            shutting_down: false,
            queue_linked: Default::default(),
        }
    }
}