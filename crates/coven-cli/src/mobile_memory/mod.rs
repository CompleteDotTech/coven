pub mod contract;

pub const MOBILE_PROTOCOL_VERSION: u16 = 1;
pub const MAX_MOBILE_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_MOBILE_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
pub const MOBILE_REQUEST_WINDOW_SECONDS: i64 = 300;
