use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub allowed_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTaskResult {
    pub id: String,
    pub output: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationPlan {
    pub sub_tasks: Vec<SubTask>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub max_iterations: usize,
    pub timeout_secs: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            max_iterations: 8,
            timeout_secs: 120,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AggregatedResult {
    pub task: String,
    pub total: usize,
    pub successes: usize,
    pub failures: usize,
    pub results: Vec<SubTaskResult>,
    pub synthesis: String,
}

impl AggregatedResult {
    pub fn is_all_ok(&self) -> bool {
        self.failures == 0
    }
}
