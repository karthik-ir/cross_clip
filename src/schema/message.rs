use serde::{Deserialize, Serialize};
use chrono::Utc;

//
// #[derive(Serialize, Deserialize, Debug)]
// struct MyMessage {
//     message: String,
//     peer_id: String
// }


#[derive(Serialize, Deserialize, Debug)]
pub struct InputMessage {
    pub message: String,
    pub timestamp: i64,
}

impl InputMessage {
    pub fn new(message: String) -> Self {
        InputMessage {
            message,
            timestamp: Utc::now().timestamp()
        }
    }
}
