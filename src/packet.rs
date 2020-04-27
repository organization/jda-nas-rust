pub struct Queued {
    pub data: Vec<u8>,
    pub data_length: usize,
}

pub struct Unsent {
    pub packet: Queued,
    pub address: Vec<dns_lookup::AddrInfo>,
}