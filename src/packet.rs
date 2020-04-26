mod packet {
    pub struct Queued {
        data: u8,
        data_length: usize,
    }

    pub struct Unsent {
        packet: Queued,
        address: dns_lookup::AddrInfo,
    }
}