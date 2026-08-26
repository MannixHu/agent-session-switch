use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Project {
    pub id: String, // UUID
    pub name: String,
    pub description: String,
    pub path: String,
    pub color: String, // hex color
    pub is_favorited: bool,
    pub session_ids: Vec<String>, // session UUIDs
    pub created_at: String,       // ISO8601
    pub updated_at: String,
}

impl Project {
    pub fn new(name: String, path: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            description: String::new(),
            path,
            color: "#3B82F6".to_string(),
            is_favorited: false,
            session_ids: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
