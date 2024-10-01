use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::process::Command;
use tokio::time::{sleep, Duration};
use regex::Regex;

// Struct for deserializing TOML config
#[derive(Deserialize, Debug, PartialEq)]
struct Config {
    slack: SlackConfig,
    sleep_time_seconds: u64,
}

#[derive(Deserialize, Debug, PartialEq)]
struct SlackConfig {
    webhook_url: String,
}

// Define an endpoint structure for deserialization
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Endpoint {
    endpoint: String,
    state: String,
    channels: String,
}

// Struct to hold the parsed data
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct EndpointsData {
    endpoints: Vec<Endpoint>,
}

// Function to run the asterisk command and parse the output
fn get_pjsip_endpoints(output: &str) -> EndpointsData {
    let mut endpoints = Vec::new();

    // Define the regex pattern for extracting the data
    let re = Regex::new(r"^Endpoint:\s+(\S+)\s+(.+?)\s+(\d+\s+of\s+inf)$").unwrap();

    // Iterate over each line and apply the regex
    for mut line in output.lines() {
        line = line.trim();
        if let Some(captures) = re.captures(line) {
            let endpoint = captures.get(1).unwrap().as_str().to_string();
            let state = captures.get(2).unwrap().as_str().to_string();
            let channels = captures.get(3).unwrap().as_str().to_string();

            // Add the parsed data to the endpoints vector
            endpoints.push(Endpoint { endpoint, state, channels });
        }
        else {
            println!("Failed to parse line: {}", line);
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

    let response = client.post(webhook_url).json(&payload).send().await;

    match response {
        Ok(_) => println!("Message sent to Slack"),
        Err(e) => eprintln!("Failed to send message to Slack: {}", e),
    }
}

// Function to read the config file
fn read_config(file_content: &str) -> Config {
    toml::from_str(file_content).expect("Failed to parse config.toml")
}

// The main function that checks the endpoints periodically and posts to Slack on changes
#[tokio::main]
async fn main() {
    // Read the configuration file
    let config_content = fs::read_to_string("config.toml").expect("Failed to read config.toml");
    let config = read_config(&config_content);

    // Store the hash of the previous data for change detection
    let mut last_hash: Option<String> = None;

    loop {
        // Run the asterisk command and get the current pjsip endpoints output
        let output = Command::new("asterisk")
            .arg("-rx")
            .arg("pjsip list endpoints")
            .output()
            .expect("failed to execute process");

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Get the current pjsip endpoints data
        let current_data = get_pjsip_endpoints(&stdout);
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

// Unit tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pjsip_output() {
        let output = r#"
            Endpoint:  500/500                                              Unavailable   0 of inf
            Endpoint:  502/502                                              Not in use    0 of inf
            Endpoint:  Voipfone                                             Not in use    0 of inf
        "#;

        let expected_data = EndpointsData {
            endpoints: vec![
                Endpoint {
                    endpoint: "500/500".to_string(),
                    state: "Unavailable".to_string(),
                    channels: "0 of inf".to_string(),
                },
                Endpoint {
                    endpoint: "502/502".to_string(),
                    state: "Not in use".to_string(),
                    channels: "0 of inf".to_string(),
                },
                Endpoint {
                    endpoint: "Voipfone".to_string(),
                    state: "Not in use".to_string(),
                    channels: "0 of inf".to_string(),
                },
            ],
        };

        let parsed_data = get_pjsip_endpoints(output);
        assert_eq!(parsed_data, expected_data);
    }

    #[test]
    fn test_calculate_hash() {
        let data = EndpointsData {
            endpoints: vec![
                Endpoint {
                    endpoint: "500/500".to_string(),
                    state: "Unavailable".to_string(),
                    channels: "0 of inf".to_string(),
                },
                Endpoint {
                    endpoint: "502/502".to_string(),
                    state: "Not in use".to_string(),
                    channels: "0 of inf".to_string(),
                },
            ],
        };

        // Calculate the hash for the initial data
        let initial_hash = calculate_hash(&data);

        // Modify the data and check that the hash changes
        let modified_data = EndpointsData {
            endpoints: vec![
                Endpoint {
                    endpoint: "500/500".to_string(),
                    state: "Unavailable".to_string(),
                    channels: "0 of inf".to_string(),
                },
                Endpoint {
                    endpoint: "502/502".to_string(),
                    state: "Unavailable".to_string(), // Changed from "Not in use"
                    channels: "0 of inf".to_string(),
                },
            ],
        };

        let modified_hash = calculate_hash(&modified_data);

        // Ensure that the hash is different after modification
        assert_ne!(initial_hash, modified_hash);
    }

    #[test]
    fn test_read_config() {
        let config_content = r#"
            sleep_time_seconds = 60
            [slack]
            webhook_url = "https://hooks.slack.com/services/TEST/WEBHOOK/URL"
        "#;

        let expected_config = Config {
            sleep_time_seconds: 60,
            slack: SlackConfig {
                webhook_url: "https://hooks.slack.com/services/TEST/WEBHOOK/URL".to_string(),
            },
        };

        let config = read_config(config_content);
        assert_eq!(config, expected_config);
    }
}
