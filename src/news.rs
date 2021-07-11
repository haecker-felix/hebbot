use serde::{Deserialize, Serialize};

use std::collections::HashMap;

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct News {
    pub event_id: String,
    pub reporter_id: String,
    pub reporter_display_name: String,
    pub message: String,
    pub approvals: HashMap<String, String>,
    pub sections: HashMap<String, String>,
    pub projects: HashMap<String, String>,
}
