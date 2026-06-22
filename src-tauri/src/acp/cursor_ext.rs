//! Cursor CLI ACP extension methods (`cursor/*`).
//!
//! Blocking: `cursor/ask_question`, `cursor/create_plan` — the agent waits for a
//! JSON-RPC response. Notifications: `cursor/update_todos`, `cursor/task`,
//! `cursor/generate_image` — fire-and-forget; we handle todos as plan updates
//! and log the rest.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::acp::question::{
    QuestionAnswer, QuestionOption, QuestionOutcome, QuestionSpec, MAX_HEADER_CHARS,
    MAX_OPTIONS, MAX_QUESTIONS, MIN_OPTIONS,
};
use crate::acp::types::PlanEntryInfo;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorAskQuestionRequest {
    pub tool_call_id: String,
    pub title: Option<String>,
    pub questions: Vec<CursorQuestion>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<CursorQuestionOption>,
    #[serde(default)]
    pub allow_multiple: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorQuestionOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorCreatePlanRequest {
    pub tool_call_id: String,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub plan: String,
    #[serde(default)]
    pub todos: Vec<CursorTodo>,
    #[serde(default)]
    pub is_project: bool,
    #[serde(default)]
    pub phases: Vec<CursorPlanPhase>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorPlanPhase {
    pub name: String,
    #[serde(default)]
    pub todos: Vec<CursorTodo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorTodo {
    pub id: String,
    pub content: String,
    pub status: CursorTodoStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CursorTodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorUpdateTodosRequest {
    pub tool_call_id: String,
    pub todos: Vec<CursorTodo>,
    #[serde(default)]
    pub merge: bool,
}

/// Cursor `cursor/task` notification — subagent task lifecycle update.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorTaskRequest {
    pub tool_call_id: String,
    pub description: String,
    pub prompt: String,
    #[serde(default = "default_subagent_type")]
    pub subagent_type: Value,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

fn default_subagent_type() -> Value {
    json!("unspecified")
}

/// Cursor `cursor/generate_image` notification.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorGenerateImageRequest {
    pub tool_call_id: String,
    pub description: String,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub reference_image_paths: Vec<String>,
}

/// Human-readable label for Cursor's `subagentType` wire value (string or
/// `{ custom: "..." }`).
pub fn cursor_subagent_type_label(raw: &Value) -> String {
    if let Some(s) = raw.as_str() {
        return s.to_string();
    }
    if let Some(custom) = raw.get("custom").and_then(|v| v.as_str()) {
        return custom.to_string();
    }
    "unspecified".to_string()
}

/// Awaiting-approval plan stored on the session snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingPlanState {
    pub plan_id: String,
    pub tool_call_id: String,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub plan: String,
    pub todos: Vec<CursorTodo>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanAnswer {
    #[serde(default)]
    pub accepted: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub cancelled: bool,
}

pub struct RegisteredPlan {
    pub plan_id: String,
    pub answer_rx: oneshot::Receiver<Value>,
}

#[async_trait]
pub trait SessionCursorPlanAccess: Send + Sync {
    async fn register_plan(
        &self,
        parent_connection_id: &str,
        request: CursorCreatePlanRequest,
    ) -> Option<RegisteredPlan>;

    async fn cancel_plans_by_parent(&self, parent_connection_id: &str);
}

fn truncate_header(text: &str) -> String {
    text.trim()
        .chars()
        .take(MAX_HEADER_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Map a Cursor `cursor/ask_question` payload into Codeg [`QuestionSpec`]s for
/// the existing `AskQuestionCard`. Preserves Cursor question ids; option labels
/// are shown verbatim and matched back to option ids on answer.
pub fn map_cursor_questions_to_specs(req: &CursorAskQuestionRequest) -> Result<Vec<QuestionSpec>, String> {
    if req.questions.is_empty() {
        return Err("cursor/ask_question requires at least one question".into());
    }
    if req.questions.len() > MAX_QUESTIONS {
        return Err(format!(
            "cursor/ask_question supports at most {MAX_QUESTIONS} questions"
        ));
    }
    let mut out = Vec::with_capacity(req.questions.len());
    for (qi, q) in req.questions.iter().enumerate() {
        if q.id.trim().is_empty() {
            return Err(format!("questions[{qi}] is missing a non-empty `id`"));
        }
        if q.prompt.trim().is_empty() {
            return Err(format!("questions[{qi}] is missing a non-empty `prompt`"));
        }
        if q.options.len() < MIN_OPTIONS {
            return Err(format!(
                "questions[{qi}] must have at least {MIN_OPTIONS} options"
            ));
        }
        let options: Vec<QuestionOption> = q
            .options
            .iter()
            .take(MAX_OPTIONS)
            .map(|o| QuestionOption {
                label: o.label.clone(),
                description: String::new(),
            })
            .collect();
        if options.len() < MIN_OPTIONS {
            return Err(format!(
                "questions[{qi}] must have at least {MIN_OPTIONS} usable options"
            ));
        }
        let header = req
            .title
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(truncate_header)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| truncate_header(&q.prompt));
        let header = if header.is_empty() {
            format!("Q{}", qi + 1)
        } else {
            header
        };
        out.push(QuestionSpec {
            id: q.id.clone(),
            question: q.prompt.clone(),
            header,
            multi_select: q.allow_multiple,
            options,
        });
    }
    Ok(out)
}

pub fn cursor_ask_cancelled_response() -> Value {
    json!({ "outcome": { "outcome": "cancelled" } })
}

pub fn cursor_ask_skipped_response(reason: &str) -> Value {
    json!({
        "outcome": {
            "outcome": "skipped",
            "reason": reason
        }
    })
}

/// Build the Cursor `cursor/ask_question` JSON-RPC result from the original
/// request and the resolved [`QuestionOutcome`].
pub fn build_cursor_ask_response(
    req: &CursorAskQuestionRequest,
    outcome: QuestionOutcome,
) -> Value {
    if outcome.declined {
        return cursor_ask_skipped_response("User declined to answer");
    }
    let mut answers = Vec::new();
    for cq in &req.questions {
        let selected = outcome
            .answers
            .iter()
            .find(|a| a.question == cq.prompt)
            .map(|a| &a.selected)
            .cloned()
            .unwrap_or_default();
        let selected_option_ids: Vec<String> = selected
            .iter()
            .filter_map(|label| {
                cq.options
                    .iter()
                    .find(|o| o.label == *label)
                    .map(|o| o.id.clone())
            })
            .collect();
        answers.push(json!({
            "questionId": cq.id,
            "selectedOptionIds": selected_option_ids
        }));
    }
    json!({
        "outcome": {
            "outcome": "answered",
            "answers": answers
        }
    })
}

/// Richer mapping that joins [`QuestionAnswer`] (frontend) with the original
/// Cursor request so option ids resolve by label.
pub fn build_cursor_ask_response_from_answer(
    req: &CursorAskQuestionRequest,
    answer: &QuestionAnswer,
) -> Value {
    if answer.declined {
        return cursor_ask_skipped_response("User declined to answer");
    }
    let mut answers = Vec::new();
    for cq in &req.questions {
        let item = answer.answers.iter().find(|a| a.question_id == cq.id);
        let selected_option_ids: Vec<String> = item
            .map(|a| {
                a.labels
                    .iter()
                    .filter_map(|label| {
                        cq.options
                            .iter()
                            .find(|o| o.label == *label)
                            .map(|o| o.id.clone())
                    })
                    .collect()
            })
            .unwrap_or_default();
        answers.push(json!({
            "questionId": cq.id,
            "selectedOptionIds": selected_option_ids
        }));
    }
    json!({
        "outcome": {
            "outcome": "answered",
            "answers": answers
        }
    })
}

pub fn cursor_plan_cancelled_response() -> Value {
    json!({ "outcome": { "outcome": "cancelled" } })
}

pub fn build_cursor_plan_response(answer: &PlanAnswer) -> Value {
    if answer.cancelled {
        return cursor_plan_cancelled_response();
    }
    if answer.accepted {
        json!({ "outcome": { "outcome": "accepted" } })
    } else {
        json!({
            "outcome": {
                "outcome": "rejected",
                "reason": answer.reason.as_deref().unwrap_or("User rejected the plan")
            }
        })
    }
}

pub fn map_cursor_todos_to_plan_entries(todos: &[CursorTodo]) -> Vec<PlanEntryInfo> {
    todos
        .iter()
        .map(|t| PlanEntryInfo {
            content: t.content.clone(),
            priority: "medium".to_string(),
            status: match t.status {
                CursorTodoStatus::Pending => "pending",
                CursorTodoStatus::InProgress => "in_progress",
                CursorTodoStatus::Completed => "completed",
                CursorTodoStatus::Cancelled => "cancelled",
            }
            .to_string(),
        })
        .collect()
}

pub fn parse_cursor_ask(params: &Value) -> Result<CursorAskQuestionRequest, String> {
    serde_json::from_value(params.clone())
        .map_err(|e| format!("invalid cursor/ask_question params: {e}"))
}

pub fn parse_cursor_create_plan(params: &Value) -> Result<CursorCreatePlanRequest, String> {
    serde_json::from_value(params.clone())
        .map_err(|e| format!("invalid cursor/create_plan params: {e}"))
}

pub fn parse_cursor_update_todos(params: &Value) -> Result<CursorUpdateTodosRequest, String> {
    serde_json::from_value(params.clone())
        .map_err(|e| format!("invalid cursor/update_todos params: {e}"))
}

pub fn parse_cursor_task(params: &Value) -> Result<CursorTaskRequest, String> {
    serde_json::from_value(params.clone()).map_err(|e| format!("invalid cursor/task params: {e}"))
}

pub fn parse_cursor_generate_image(
    params: &Value,
) -> Result<CursorGenerateImageRequest, String> {
    serde_json::from_value(params.clone())
        .map_err(|e| format!("invalid cursor/generate_image params: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_cursor_questions_preserves_ids() {
        let req = CursorAskQuestionRequest {
            tool_call_id: "call_1".into(),
            title: Some("Pick one".into()),
            questions: vec![CursorQuestion {
                id: "q1".into(),
                prompt: "Which mode?".into(),
                options: vec![
                    CursorQuestionOption {
                        id: "agent".into(),
                        label: "Agent".into(),
                    },
                    CursorQuestionOption {
                        id: "plan".into(),
                        label: "Plan".into(),
                    },
                ],
                allow_multiple: false,
            }],
        };
        let specs = map_cursor_questions_to_specs(&req).unwrap();
        assert_eq!(specs[0].id, "q1");
        assert_eq!(specs[0].options.len(), 2);
    }

    #[test]
    fn build_cursor_ask_response_maps_option_ids() {
        let req = CursorAskQuestionRequest {
            tool_call_id: "call_1".into(),
            title: None,
            questions: vec![CursorQuestion {
                id: "q1".into(),
                prompt: "Which?".into(),
                options: vec![
                    CursorQuestionOption {
                        id: "a".into(),
                        label: "Alpha".into(),
                    },
                    CursorQuestionOption {
                        id: "b".into(),
                        label: "Beta".into(),
                    },
                ],
                allow_multiple: false,
            }],
        };
        let answer = QuestionAnswer {
            answers: vec![crate::acp::question::QuestionAnswerItem {
                question_id: "q1".into(),
                labels: vec!["Beta".into()],
            }],
            declined: false,
        };
        let resp = build_cursor_ask_response_from_answer(&req, &answer);
        assert_eq!(
            resp["outcome"]["answers"][0]["selectedOptionIds"],
            json!(["b"])
        );
    }

    #[test]
    fn parse_cursor_task_accepts_custom_subagent_type() {
        let params = json!({
            "toolCallId": "call_126",
            "description": "Explore codebase",
            "prompt": "Find auth handlers",
            "subagentType": { "custom": "reviewer" },
            "durationMs": 1200
        });
        let req = parse_cursor_task(&params).unwrap();
        assert_eq!(req.description, "Explore codebase");
        assert_eq!(cursor_subagent_type_label(&req.subagent_type), "reviewer");
        assert_eq!(req.duration_ms, Some(1200));
    }

    #[test]
    fn parse_cursor_generate_image_accepts_optional_path() {
        let params = json!({
            "toolCallId": "call_127",
            "description": "App icon mockup",
            "filePath": "/tmp/icon.png",
            "referenceImagePaths": ["/tmp/ref.png"]
        });
        let req = parse_cursor_generate_image(&params).unwrap();
        assert_eq!(req.description, "App icon mockup");
        assert_eq!(req.file_path.as_deref(), Some("/tmp/icon.png"));
        assert_eq!(req.reference_image_paths, vec!["/tmp/ref.png"]);
    }
}
