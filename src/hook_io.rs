use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: ToolInput,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ToolInput {
    #[serde(default)]
    pub file_path: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct HookOutput {
    #[serde(rename = "systemMessage", skip_serializing_if = "Option::is_none")]
    pub system_message: Option<String>,
    #[serde(rename = "hookSpecificOutput", skip_serializing_if = "Option::is_none")]
    pub hook_specific_output: Option<HookSpecificOutput>,
}

#[derive(Debug, Serialize)]
pub struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: &'static str,
    #[serde(rename = "additionalContext", skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

impl HookOutput {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_write_input() {
        let raw = r#"{
            "session_id": "abc",
            "cwd": "/some/dir",
            "hook_event_name": "PostToolUse",
            "tool_name": "Write",
            "tool_input": { "file_path": "/abs/foo.ts", "content": "..." },
            "tool_response": { "filePath": "/abs/foo.ts", "success": true }
        }"#;
        let i: HookInput = serde_json::from_str(raw).unwrap();
        assert_eq!(i.tool_name, "Write");
        assert_eq!(i.tool_input.file_path.as_deref(), Some("/abs/foo.ts"));
        assert_eq!(i.cwd.as_deref(), Some("/some/dir"));
    }

    #[test]
    fn parses_edit_input() {
        let raw = r#"{
            "tool_name": "Edit",
            "tool_input": { "file_path": "/abs/bar.ts", "old_string": "a", "new_string": "b" }
        }"#;
        let i: HookInput = serde_json::from_str(raw).unwrap();
        assert_eq!(i.tool_name, "Edit");
        assert_eq!(i.tool_input.file_path.as_deref(), Some("/abs/bar.ts"));
    }

    #[test]
    fn parses_multiedit_input_with_file_path() {
        let raw = r#"{
            "tool_name": "MultiEdit",
            "tool_input": { "file_path": "/abs/baz.ts", "edits": [] }
        }"#;
        let i: HookInput = serde_json::from_str(raw).unwrap();
        assert_eq!(i.tool_name, "MultiEdit");
        assert_eq!(i.tool_input.file_path.as_deref(), Some("/abs/baz.ts"));
    }

    #[test]
    fn parses_tool_without_file_path() {
        let raw = r#"{ "tool_name": "Bash", "tool_input": { "command": "ls" } }"#;
        let i: HookInput = serde_json::from_str(raw).unwrap();
        assert_eq!(i.tool_name, "Bash");
        assert!(i.tool_input.file_path.is_none());
    }

    #[test]
    fn serializes_hook_output_with_both_fields() {
        let out = HookOutput {
            system_message: Some("cc-essentials: formatted foo.ts (0 warnings)".to_string()),
            hook_specific_output: Some(HookSpecificOutput {
                hook_event_name: "PostToolUse",
                additional_context: Some("lint warning: foo".to_string()),
            }),
        };
        let s = out.to_json().unwrap();
        assert!(s.contains(r#""systemMessage""#));
        assert!(s.contains(r#""hookSpecificOutput""#));
        assert!(s.contains(r#""hookEventName":"PostToolUse""#));
        assert!(s.contains(r#""additionalContext""#));
    }

    #[test]
    fn serializes_empty_hook_output_as_empty_object() {
        let out = HookOutput::empty();
        let s = out.to_json().unwrap();
        assert_eq!(s, "{}");
    }
}
