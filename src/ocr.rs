use anyhow::{Context, Result};
use base64::Engine;
use rig::{
    client::CompletionClient,
    completion::Prompt,
    http_client::ReqwestClient,
    message::{DocumentSourceKind, Image, ImageDetail, ImageMediaType},
    providers::openai,
};

const PREAMBLE: &str = "Extract the text from the following image and do not translate it.";
const TEMPERATURE: f64 = 0.5;

/// Perform OCR on a PNG image by sending it to an OpenAI-compatible LLM API.
///
/// Encodes the image as base64, builds a completion client, and prompts the
/// model to extract text from the image.
pub async fn perform_ocr(url: &str, model: &str, png_bytes: &[u8]) -> Result<String> {
    let base64_str = base64::engine::general_purpose::STANDARD.encode(png_bytes);

    let client = openai::CompletionsClient::<ReqwestClient>::builder()
        .api_key("")
        .base_url(url)
        .build()
        .context("Failed to build OpenAI client")?;

    let agent = client
        .agent(model)
        .preamble(PREAMBLE)
        .temperature(TEMPERATURE)
        .build();

    let image = Image {
        data: DocumentSourceKind::base64(&base64_str),
        media_type: Some(ImageMediaType::PNG),
        detail: Some(ImageDetail::Auto),
        additional_params: None,
        ..Default::default()
    };

    let response = agent
        .prompt(image)
        .await
        .context("LLM prompt request failed")?;

    Ok(response)
}
