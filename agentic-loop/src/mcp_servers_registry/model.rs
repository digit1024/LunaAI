#[derive(Debug, Clone, PartialEq)]
pub enum ServerStatus {
    Connected,
    Failed(String), // error message
}
pub struct  ServerWithStatus {
    pub server_name: String,
    pub server_status: ServerStatus,
}