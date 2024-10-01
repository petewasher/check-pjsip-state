use std::process::Command;
use std::fs;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use reqwest::Client;
use tokio;
use tokio::time::{sleep, Duration};

// Struct for deserializing TOML config
#[derive(Deserialize)]
struct Config {
    slack: SlackConfig,
    sleep_time_seconds: u64,
}

#[derive(Deserialize)]
struct SlackConfig {
    webhook_url: String,
}

// Define an endpoint structure for deserialization
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Endpoint {
    endpoint: String,
    state: String,
    channels: String,
}

// Struct to hold the parsed data
#[derive(Serialize, Deserialize, Debug, Clone)]
struct EndpointsData {
    endpoints: Vec<Endpoint>,
}

// Function to run the asterisk command and parse the output
fn get_pjsip_endpoints() -> EndpointsData {
    let output = Command::new("asterisk")
        .arg("-rx")
        .arg("pjsip list endpoints")
        .output()
        .expect("failed to execute process");

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut endpoints = Vec::new();

    // Parse the lines and extract the fields
    for line in stdout.lines() {
        if line.starts_with(" Endpoint:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 6 {
                endpoints.push(Endpoint {
                    endpoint: parts[1].to_string(),
                    state: format!("{} {}", parts[2], parts[3]),
                    channels: format!("{} {} {}", parts[4], parts[5], parts[6]),
                });
            }
        }
    }

    EndpointsData { endpoints }
}

// Function to calculate a hash for the endpoints data
fn calculate_hash(data: &EndpointsData) -> String {
    let serialized = serde_json::to_string(data).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(serialized);
    format!("{:x}", hasher.finalize())
}

// Function to post a message to Slack webhook
async fn post_to_slack(webhook_url: &str, message: &str) {
    let client = Client::new();
    let payload = json!({
        "text": message,
    });

    let response = client.post(webhook_url)
        .json(&payload)
        .send()
        .await;

    match response {
        Ok(_) => println!("Message sent to Slack"),
        Err(e) => eprintln!("Failed to send message to Slack: {}", e),
    }
}

// Function to read the config file
fn read_config() -> Config {
    let config_content = fs::read_to_string("config.toml")
        .expect("Failed to read config.toml");
    toml::from_str(&config_content).expect("Failed to parse config.toml")
}

#[tokio::main]
async fn main() {
    // Read the configuration file
    let config = read_config();

    // Store the hash of the previous data for change detection
    let mut last_hash: Option<String> = None;

    loop {
        // Get the current pjsip endpoints data
        let current_data = get_pjsip_endpoints();
        let current_hash = calculate_hash(&current_data);

        // Compare the hash with the last one
        if last_hash.is_none() || last_hash.as_ref().unwrap() != &current_hash {
            // Data has changed, send a notification to Slack
            let message = format!("Endpoints have changed: {:?}", current_data);
            post_to_slack(&config.slack.webhook_url, &message).await;

            // Update the last_hash with the current one
            last_hash = Some(current_hash);
        } else {
            println!("No change detected.");
        }

        // Sleep for a certain interval before the next check
        sleep(Duration::from_secs(config.sleep_time_seconds)).await;
    }
}
