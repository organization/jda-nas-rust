pub struct Queued {
    pub data: Vec<u8>,
    pub data_length: usize,
}

impl Clone for Queued {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            data_length: self.data_length,
        }
    }
}

pub struct Unsent<'a> {
    pub packet: Queued,
    pub address: Vec<dns_lookup::AddrInfo>,
    pub explicit_socket: Option<&'a mut tokio::net::UdpSocket>,
}