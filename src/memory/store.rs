use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub content: String,
    pub tags: Vec<String>,
    pub timestamp: u64,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub keywords: Vec<String>,
    pub tags: Vec<String>,
    pub limit: usize,
}

impl MemoryQuery {
    pub fn new(query: &str) -> Self {
        Self {
            keywords: query.split_whitespace().map(|s| s.to_lowercase()).collect(),
            tags: Vec::new(),
            limit: 10,
        }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

pub struct MemoryStore {
    items: RwLock<Vec<MemoryItem>>,
    storage_path: String,
}

impl MemoryStore {
    pub fn new() -> Result<Self, String> {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let path = format!("{}/.config/agents-rust/memory.json", home);

        let store = Self {
            items: RwLock::new(Vec::new()),
            storage_path: path.clone(),
        };

        let _ = store.load();
        Ok(store)
    }

    pub fn with_path(path: &str) -> Result<Self, String> {
        let store = Self {
            items: RwLock::new(Vec::new()),
            storage_path: path.to_string(),
        };
        let _ = store.load();
        Ok(store)
    }

    fn load(&self) -> Result<(), String> {
        match std::fs::read_to_string(&self.storage_path) {
            Ok(content) => {
                let items: Vec<MemoryItem> = serde_json::from_str(&content)
                    .map_err(|e| format!("Parse memory lỗi: {}", e))?;
                *self.items.write().map_err(|e| e.to_string())? = items;
                Ok(())
            }
            Err(_) => Ok(()),
        }
    }

    fn save(&self) -> Result<(), String> {
        let dir = std::path::Path::new(&self.storage_path).parent().unwrap();
        let _ = std::fs::create_dir_all(dir);

        let items = self.items.read().map_err(|e| e.to_string())?;
        let content = serde_json::to_string_pretty(&*items)
            .map_err(|e| format!("Serialize memory lỗi: {}", e))?;
        std::fs::write(&self.storage_path, content)
            .map_err(|e| format!("Ghi memory lỗi: {}", e))
    }

    pub fn add(&self, content: &str, tags: &[String]) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let item = MemoryItem {
            id: id.clone(),
            content: content.to_string(),
            tags: tags.to_vec(),
            timestamp,
            metadata: HashMap::new(),
        };

        self.items.write().map_err(|e| e.to_string())?.push(item);
        self.save()?;
        Ok(id)
    }

    pub fn add_with_metadata(
        &self,
        content: &str,
        tags: &[String],
        metadata: HashMap<String, String>,
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let item = MemoryItem {
            id: id.clone(),
            content: content.to_string(),
            tags: tags.to_vec(),
            timestamp,
            metadata,
        };

        self.items.write().map_err(|e| e.to_string())?.push(item);
        self.save()?;
        Ok(id)
    }

    pub fn search(&self, query: &MemoryQuery) -> Vec<MemoryItem> {
        let items = self.items.read().unwrap();
        let mut scored: Vec<(i32, &MemoryItem)> = items.iter()
            .map(|item| {
                let mut score = 0i32;
                let lower_content = item.content.to_lowercase();

                for kw in &query.keywords {
                    if lower_content.contains(kw) {
                        score += 10;
                    }
                }

                for tag in &query.tags {
                    if item.tags.contains(tag) {
                        score += 20;
                    }
                }

                (score, item)
            })
            .filter(|(s, _)| *s > 0)
            .collect();

        scored.sort_by_key(|b| std::cmp::Reverse(b.0));
        scored.truncate(query.limit);

        scored.into_iter().map(|(_, item)| item.clone()).collect()
    }

    pub fn get_by_id(&self, id: &str) -> Option<MemoryItem> {
        self.items.read().ok()?.iter().find(|m| m.id == id).cloned()
    }

    pub fn get_all(&self) -> Vec<MemoryItem> {
        self.items.read().map(|items| items.clone()).unwrap_or_default()
    }

    pub fn get_recent(&self, limit: usize) -> Vec<MemoryItem> {
        let mut items = self.get_all();
        items.sort_by_key(|b| std::cmp::Reverse(b.timestamp));
        items.truncate(limit);
        items
    }

    pub fn remove(&self, id: &str) -> Result<bool, String> {
        let mut items = self.items.write().map_err(|e| e.to_string())?;
        let len_before = items.len();
        items.retain(|m| m.id != id);
        let removed = items.len() < len_before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn count(&self) -> usize {
        self.items.read().map(|i| i.len()).unwrap_or(0)
    }

    pub fn clear(&self) -> Result<(), String> {
        self.items.write().map_err(|e| e.to_string())?.clear();
        self.save()
    }
}
