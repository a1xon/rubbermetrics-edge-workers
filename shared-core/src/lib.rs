use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommonResponse {
    pub success: bool,
    pub message: String,
}
