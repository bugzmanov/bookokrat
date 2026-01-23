use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct ChatGPTRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub max_tokens: usize,
    pub temperature: f32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize, Debug)]
pub struct ChatGPTResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize, Debug)]
pub struct Choice {
    pub message: Message,
}

pub struct ChatGPTClient {
    api_key: String,
    client: reqwest::Client,
}

impl ChatGPTClient {
    pub fn new(api_key: String) -> Result<Self> {
        if api_key.is_empty() {
            return Err(anyhow!(
                "ChatGPT API key not found. Please set OPENAI_API_KEY environment variable."
            ));
        }

        Ok(ChatGPTClient {
            api_key,
            client: reqwest::Client::new(),
        })
    }

    pub async fn summarize(&self, text: &str, language_instruction: &str) -> Result<String> {
        let prompt = format!("{}{}", language_instruction, text);

        let request = ChatGPTRequest {
            model: "gpt-3.5-turbo".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
            max_tokens: 500,
            temperature: 0.7,
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to send request to ChatGPT: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!("ChatGPT API error ({}): {}", status, body));
        }

        let gpt_response: ChatGPTResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse ChatGPT response: {}", e))?;

        if let Some(choice) = gpt_response.choices.first() {
            Ok(choice.message.content.clone())
        } else {
            Err(anyhow!("No response from ChatGPT"))
        }
    }
}
