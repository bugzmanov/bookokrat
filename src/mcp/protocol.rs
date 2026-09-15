use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SnapshotFormat {
    Epub,
    Pdf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReaderSnapshot {
    pub format: SnapshotFormat,
    pub book_path: PathBuf,
    pub book_title: String,
    pub chapter_title: String,
    pub chapter_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_number: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_index: Option<usize>,
    pub total_chapters: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<usize>,
    pub page_text: String,
    pub chapter_text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTool {
    GetCurrentPage,
    GetCurrentChapter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpRequest {
    pub tool: McpTool,
}

/// Per-tool response: metadata + exactly one `text` (the chosen page/chapter).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub format: SnapshotFormat,
    pub book_path: PathBuf,
    pub book_title: String,
    pub chapter_title: String,
    pub chapter_index: usize,
    pub total_chapters: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_number: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<usize>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum McpResponse {
    Ok { result: ToolResult },
    Error { error: String },
}

pub fn build_tool_result(snap: &ReaderSnapshot, tool: McpTool) -> ToolResult {
    let text = match tool {
        McpTool::GetCurrentPage => snap.page_text.clone(),
        McpTool::GetCurrentChapter => snap.chapter_text.clone(),
    };
    ToolResult {
        format: snap.format.clone(),
        book_path: snap.book_path.clone(),
        book_title: snap.book_title.clone(),
        chapter_title: snap.chapter_title.clone(),
        chapter_index: snap.chapter_index,
        total_chapters: snap.total_chapters,
        page_number: snap.page_number,
        screen_index: snap.screen_index,
        total_pages: snap.total_pages,
        text,
    }
}

pub fn encode_request_line(req: &McpRequest) -> String {
    serde_json::to_string(req).expect("request serializes")
}

pub fn decode_request_line(line: &str) -> Option<McpRequest> {
    serde_json::from_str(line.trim()).ok()
}

pub fn encode_response_line(resp: &McpResponse) -> String {
    serde_json::to_string(resp).expect("response serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_snapshot() -> ReaderSnapshot {
        ReaderSnapshot {
            format: SnapshotFormat::Epub,
            book_path: PathBuf::from("/abs/book.epub"),
            book_title: "Book".into(),
            chapter_title: "Ch 1".into(),
            chapter_index: 0,
            page_number: None,
            screen_index: Some(2),
            total_chapters: 5,
            total_pages: None,
            page_text: "screen text".into(),
            chapter_text: "whole chapter".into(),
        }
    }

    #[test]
    fn request_round_trip() {
        let req = McpRequest {
            tool: McpTool::GetCurrentPage,
        };
        let line = encode_request_line(&req);
        let back = decode_request_line(&line).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn response_ok_round_trip() {
        let snap = sample_snapshot();
        let result = build_tool_result(&snap, McpTool::GetCurrentChapter);
        let resp = McpResponse::Ok { result };
        let line = encode_response_line(&resp);
        let back: McpResponse = serde_json::from_str(&line).unwrap();
        match back {
            McpResponse::Ok { result } => assert_eq!(result.text, "whole chapter"),
            McpResponse::Error { .. } => panic!("expected Ok"),
        }
    }

    #[test]
    fn tool_result_carries_chosen_text_and_metadata() {
        let snap = sample_snapshot();
        let page = build_tool_result(&snap, McpTool::GetCurrentPage);
        assert_eq!(page.text, "screen text");
        assert_eq!(page.book_title, "Book");
        assert_eq!(page.screen_index, Some(2));
        // page_number is None for epub -> omitted from JSON
        let json = serde_json::to_string(&page).unwrap();
        assert!(!json.contains("page_number"));
    }
}
