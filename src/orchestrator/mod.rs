pub mod types;
pub mod worker;

use types::*;
use worker::Worker;
use crate::llm::LlmClient;

pub struct Orchestrator {
    client: LlmClient,
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            client: LlmClient::new(),
        }
    }

    /// Decompose a complex task into subtasks using LLM
    pub async fn plan(&self, task: &str, available_tools: &[String]) -> Result<OrchestrationPlan, String> {
        let prompt = format!(
            r#"Bạn là chuyên gia phân tích task. Hãy chia task sau thành các subtask nhỏ hơn, có thể chạy song song.
Task: {}

Các tools có sẵn: {}

Trả về JSON array, mỗi phần tử có: id, name, description, system_prompt, allowed_tools (mảng tên tools).
Yêu cầu:
- Mỗi subtask có thể chạy độc lập
- System prompt ngắn gọn, mô tả rõ vai trò
- Allowed_tools là subset của tools có sẵn
- Tối đa 5 subtask
- Trả về hợp lệ JSON (không markdown)"#,
            task, available_tools.join(", ")
        );

        let response = self.client.chat_with_intelligent_fallback(
            vec![
                crate::llm::types::ChatMessage::system("Bạn là AI phân tích task thành subtask. Chỉ trả về JSON."),
                crate::llm::types::ChatMessage::user(&prompt),
            ],
            Vec::new(),
            None,
        ).await?;

        let content = response.choices.first()
            .and_then(|c| c.message.content.as_deref())
            .ok_or("LLM không trả về nội dung")?;

        let content = Self::strip_markdown_fences(content);

        let sub_tasks: Vec<SubTask> = serde_json::from_str(&content)
            .map_err(|e| format!("Không parse được subtasks từ LLM: {} — content: {}", e, content))?;

        let count = sub_tasks.len();
        Ok(OrchestrationPlan {
            sub_tasks,
            summary: format!("Đã phân tích task thành {} subtask", count),
        })
    }

    fn strip_markdown_fences(content: &str) -> String {
        let trimmed = content.trim();
        if trimmed.starts_with("```json") && trimmed.ends_with("```") {
            trimmed[7..trimmed.len()-3].trim().to_string()
        } else if trimmed.starts_with("```") && trimmed.ends_with("```") {
            trimmed[3..trimmed.len()-3].trim().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Execute subtasks in parallel using tokio::spawn
    pub async fn execute_parallel(&self, plan: OrchestrationPlan, config: WorkerConfig) -> Vec<SubTaskResult> {
        let mut handles = Vec::new();

        for task_def in plan.sub_tasks {
            let config = config.clone();
            let handle = tokio::spawn(async move {
                let mut worker = Worker::new(
                    &task_def.name,
                    &task_def.system_prompt,
                    &task_def.allowed_tools,
                    config,
                );
                worker.run(&task_def.description).await
            });
            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(SubTaskResult {
                    id: "unknown".into(),
                    output: String::new(),
                    error: Some(format!("Worker panicked: {}", e)),
                }),
            }
        }
        results
    }

    /// Execute subtasks sequentially
    #[allow(dead_code)]
    pub async fn execute_sequential(&self, plan: OrchestrationPlan, config: WorkerConfig) -> Vec<SubTaskResult> {
        let mut results = Vec::new();

        for task_def in plan.sub_tasks {
            let mut worker = Worker::new(
                &task_def.name,
                &task_def.system_prompt,
                &task_def.allowed_tools,
                config.clone(),
            );
            let result = worker.run(&task_def.description).await;
            results.push(result);
        }
        results
    }

    /// Smart execution: analyze dependencies and run parallel where possible
    pub async fn execute_smart(&self, plan: OrchestrationPlan, config: WorkerConfig) -> Vec<SubTaskResult> {
        // Simple heuristic: run all in parallel by default
        // Future: analyze subtask descriptions for dependencies
        self.execute_parallel(plan, config).await
    }

    /// Plan + execute in one call
    pub async fn run(&self, task: &str, tools: &[String], config: WorkerConfig) -> Result<AggregatedResult, String> {
        let plan = self.plan(task, tools).await?;
        let results = self.execute_smart(plan, config).await;

        let mut successes = Vec::new();
        let mut failures = Vec::new();
        let mut all_outputs = Vec::new();

        for r in &results {
            all_outputs.push(format!("[{}] {}", r.id, r.output));
            if r.error.is_some() {
                failures.push(r.clone());
            } else {
                successes.push(r.clone());
            }
        }

        // Tổng hợp kết quả bằng LLM
        let synthesis = self.synthesize(task, &results).await.unwrap_or_default();

        Ok(AggregatedResult {
            task: task.to_string(),
            total: results.len(),
            successes: successes.len(),
            failures: failures.len(),
            results,
            synthesis,
        })
    }

    async fn synthesize(&self, task: &str, results: &[SubTaskResult]) -> Result<String, String> {
        let parts: Vec<String> = results.iter().map(|r| {
            let status = if r.error.is_some() { "❌ LỖI" } else { "✅ OK" };
            format!("[{}] {}:\n{}", status, r.id, r.output)
        }).collect();

        let prompt = format!(
            r#"Task gốc: {}

Kết quả từ các subtask:
{}

Hãy tổng hợp kết quả cuối cùng thành câu trả lời mạch lạc, hữu ích cho người dùng."#,
            task, parts.join("\n---\n")
        );

        let response = self.client.chat_with_intelligent_fallback(
            vec![
                crate::llm::types::ChatMessage::system("Bạn là AI tổng hợp kết quả từ các agent con."),
                crate::llm::types::ChatMessage::user(&prompt),
            ],
            Vec::new(),
            None,
        ).await?;

        Ok(response.choices.first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default())
    }
}
