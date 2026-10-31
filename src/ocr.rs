use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use rig_core::{
    completion::{CompletionRequest, message::UserContent},
    message::{DocumentSourceKind, Image, ImageDetail, ImageMediaType},
    providers::openai,
};

const PREAMBLE: &str = "Extract the text from the following image and do not translate it.";
const TEMPERATURE: f64 = 0.5;

/// Perform OCR on a PNG image by sending it to an OpenAI-compatible LLM API.
///
/// Encodes the image as base64, builds a completion client, and prompts the
/// model to extract text from the image.
pub async fn perform_ocr(
    url: &str,
    model: &str,
    png_bytes: &[u8],
    timeout: Duration,
) -> Result<String> {
    let base64_str = base64::engine::general_purpose::STANDARD.encode(png_bytes);

    let client = openai::OpenAIConfig::new("").with_base_url(url).client();

    let image = Image {
        data: DocumentSourceKind::base64(&base64_str),
        media_type: Some(ImageMediaType::PNG),
        detail: Some(ImageDetail::Auto),
        ..Default::default()
    };

    let response = tokio::time::timeout(
        timeout,
        client.chat(model).call(
            CompletionRequest::new(UserContent::Image(image))
                .preamble(PREAMBLE)
                .temperature(TEMPERATURE),
        ),
    )
    .await
    .with_context(|| {
        format!(
            "OCR request timed out after {} seconds; check your server or increase --timeout",
            timeout.as_secs_f64()
        )
    })?
    .context("LLM prompt request failed")?;

    Ok(response.text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    async fn mock_api(
        status: &'static str,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<Value>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let (header_end, content_length) = loop {
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0, "Client closed before sending request headers");
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1"));
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    break (end + 4, length);
                }
            };
            while request.len() < header_end + content_length {
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0, "Client closed before sending request body");
                request.extend_from_slice(&buffer[..count]);
            }
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            serde_json::from_slice(&request[header_end..header_end + content_length]).unwrap()
        });
        (url, server)
    }

    #[tokio::test]
    async fn sends_png_to_chat_completions_and_returns_text() {
        let (url, server) = mock_api("200 OK", r#"{"id":"test","object":"chat.completion","created":0,"model":"test-vision","choices":[{"index":0,"message":{"role":"assistant","content":"recognized text"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#).await;
        let text = perform_ocr(&url, "test-vision", b"png", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(text, "recognized text");
        let request = server.await.unwrap();
        assert_eq!(request["model"], "test-vision");
        assert_eq!(request["temperature"], TEMPERATURE);
        assert_eq!(request["messages"][0]["content"][0]["text"], PREAMBLE);
        assert_eq!(
            request["messages"][1]["content"][0]["image_url"]["url"],
            "data:image/png;base64,cG5n"
        );
    }

    #[tokio::test]
    async fn server_errors_remain_errors() {
        let (url, server) = mock_api(
            "503 Service Unavailable",
            r#"{"error":{"message":"model unavailable","type":"server_error"}}"#,
        )
        .await;
        let error = perform_ocr(&url, "test-vision", b"png", Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("LLM prompt request failed"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn stalled_server_hits_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let error = perform_ocr(&url, "test-vision", b"png", Duration::from_millis(50))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("OCR request timed out"));
        assert!(error.to_string().contains("--timeout"));
        server.abort();
    }
}
