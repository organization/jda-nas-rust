#[derive(Clone)]
pub struct Queued {
    pub data: Vec<u8>,
    pub data_length: usize,
}

pub struct Unsent<'a> {
    pub packet: Queued,
    pub address: Vec<dns_lookup::AddrInfo>,
    pub explicit_socket: Option<&'a mut tokio::net::UdpSocket>,
}
