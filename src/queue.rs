mod queue {
    use std::sync::Mutex;

    struct Buffer {
        index: usize,
        size: usize,
        capacity: usize
    }

    struct Item {
        next_due_time: i64,
        packet_buffer: packet::Queued,
        buffer: Buffer,
        address: dns_lookup::AddrInfo,
        explicit_socket: tokio::net::UdpSocket
    }

    pub struct Manager {
        queues: std::collections::HashMap<u64, Item>,
        queue_buffer_capacity: usize,
        packet_interval: i64,
        lock: Mutex<Manager>,
        shutting_down: bool,
        queue_linked: std::collections::LinkedList<Item>
    }
}