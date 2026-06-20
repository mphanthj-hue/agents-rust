use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub version: String,
    pub tools: Vec<String>,
    pub instructions: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub tools: Vec<String>,
}

fn default_version() -> String {
    "1.0.0".into()
}

pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    pub fn load_all(&mut self, dir: &str) -> Result<usize, String> {
        let path = std::path::Path::new(dir);
        if !path.exists() {
            let _ = std::fs::create_dir_all(dir);
            return Ok(0);
        }

        let mut count = 0;
        let entries = std::fs::read_dir(dir)
            .map_err(|e| format!("Đọc skills dir lỗi: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Entry lỗi: {}", e))?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md") {
                match Skill::load(&path) {
                    Ok(skill) => {
                        let name = skill.name.clone();
                        self.skills.insert(name, skill);
                        count += 1;
                    }
                    Err(e) => {
                        eprintln!("[skills] Lỗi load {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(count)
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    pub fn list(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    pub fn find_by_tool(&self, tool_name: &str) -> Vec<&Skill> {
        self.skills.values()
            .filter(|s| s.tools.contains(&tool_name.to_string()))
            .collect()
    }

    pub fn count(&self) -> usize {
        self.skills.len()
    }
}

impl Skill {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Đọc skill file lỗi: {}", e))?;

        let (frontmatter, instructions) = parse_mdx(&content)?;

        Ok(Self {
            name: frontmatter.name,
            description: frontmatter.description,
            version: frontmatter.version,
            tools: frontmatter.tools,
            instructions,
            file_path: path.to_string_lossy().to_string(),
        })
    }
}

fn parse_mdx(content: &str) -> Result<(SkillFrontmatter, String), String> {
    let content = content.trim();
    if !content.starts_with("---") {
        return Err("Thiếu YAML frontmatter (---)".into());
    }

    let rest = &content[3..];
    let end = rest.find("\n---").ok_or_else(|| String::from("Thiếu tag đóng ---"))?;

    let yaml_str = &rest[..end];
    let body_start = end + 4;
    let body = rest[body_start..].trim().to_string();

    let frontmatter: SkillFrontmatter = serde_yaml::from_str(yaml_str)
        .map_err(|e| format!("Parse YAML lỗi: {}", e))?;

    Ok((frontmatter, body))
}

pub fn default_skills_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home)
        .join(".config")
        .join("agents-rust")
        .join("skills")
}
