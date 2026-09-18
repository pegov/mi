use std::time::Duration;

use serde_json::json;

pub fn jev(gateway: &str) -> anyhow::Result<()> {
    let client = reqwest::blocking::ClientBuilder::default()
        .timeout(Duration::from_secs(600))
        .build()?;

    let payload = json!(
        {
            "model": "~typesafe/jev-latest",
            "state": "I am building simple and minimal agent harness to learn.",
            "questions": {
                "category": {
                    "type": "score",
                    "instructions": "To which category should I put this tool?",
                    "criteria": [
                        "Database",
                        "Accounting",
                        "AI",
                        "Manufacture",
                        "Programming",
                        "Marketing"
                    ]
                },
                "is_ai": {
                    "type": "noul",
                    "instructions": "Does this message mention AI in any way?",
                    "criteria": {
                        "true": "Yes",
                        "false": "No"
                    }
                }
            }
        }
    );

    let response = client.post(gateway).json(&payload).send()?;
    let status = response.status();
    let body_text = response.text()?;

    if !status.is_success() {
        anyhow::bail!("API {} error: {}", status, body_text);
    }

    let body: serde_json::Value = serde_json::from_str(&body_text)
        .map_err(|e| anyhow::anyhow!("invalid JSON: {} — body: {}", e, body_text))?;

    let pretty = serde_json::to_string_pretty(&body)?;
    println!("{pretty}");

    Ok(())
}
