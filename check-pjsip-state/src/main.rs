use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::process::Command;
use tokio::time::{sleep, Duration};
use slack_morphism::prelude::*;

// Struct for deserializing TOML config
#[derive(Deserialize, Debug, PartialEq)]
struct Config {
    slack: SlackConfig,
    sleep_time_seconds: u64,
}

#[derive(Deserialize, Debug, PartialEq)]
struct SlackConfig {
    api_token: String,
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
            endpoints.push(Endpoint {
                endpoint,
                state,
                channels,
            });
        } else {
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

// Function to read the config file
fn read_config(file_content: &str) -> Config {
    toml::from_str(file_content).expect("Failed to parse config.toml")
}

async fn slack_send_message(app_token: &str, the_message: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let client = SlackClient::new(SlackClientHyperConnector::new()?);

    // Create our Slack API token
    let token_value: SlackApiTokenValue = app_token.into();
    let token: SlackApiToken = SlackApiToken::new(token_value);
    
    // Create a Slack session with this token
    // A session is just a lightweight wrapper around your token
    // not to specify it all the time for series of calls.
    let session = client.open_session(&token);
    
    // Make your first API call (which is `api.test` here)
    let _: SlackApiTestResponse = session
            .api_test(&SlackApiTestRequest::new().with_foo("Test".into()))
            .await?;

    // Send a simple text message
    let post_chat_req =
        SlackApiChatPostMessageRequest::new("#general".into(),
               SlackMessageContent::new().with_text(the_message.into())
        );

    let _ = session.chat_post_message(&post_chat_req).await?;

    Ok(())
}

// The main function that checks the endpoints periodically and posts to Slack on changes
#[tokio::main]
async fn main() {

    // Collect the config filename from the command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: check-pjsip-state <config_file>");
        std::process::exit(1);
    }

    // Read the configuration file
    let config_content = fs::read_to_string(&args[1]).expect("Failed to read config.toml");
    let config = read_config(&config_content);
    
    match slack_send_message(&config.slack.api_token, "check-pjsip-started").await {
        Ok(_) => println!("Message sent to Slack"),
        Err(e) => eprintln!("Failed to send message to Slack: {}", e),
    };
    
    // Store the hash of the previous data for change detection
    let mut last_hash: Option<String> = None;

    loop {
        // Run the asterisk command and get the current pjsip endpoints output
        let output = match Command::new("asterisk")
            .arg("-rx")
            .arg("pjsip list endpoints")
            .output() {
            Ok(output) => output,
            Err(e) => {

                eprintln!("Failed to run the command: {}", e);
                
                // Send a slack message and abort
                match slack_send_message(&config.slack.api_token, "Failed to run the command").await {
                    Ok(_) => println!("Message sent to Slack"),
                    Err(e) => eprintln!("Failed to send message to Slack: {}", e),
                };

                std::process::exit(1);
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Get the current pjsip endpoints data
        let current_data = get_pjsip_endpoints(&stdout);
        let current_hash = calculate_hash(&current_data);

        // Compare the hash with the last one
        if last_hash.is_none() || last_hash.as_ref().unwrap() != &current_hash {
            // Data has changed, send a notification to Slack
            let message = format!("Endpoints have changed: {:?}", current_data);
            
            match slack_send_message(&config.slack.api_token, &message).await {
                Ok(_) => println!("Message sent to Slack"),
                Err(e) => eprintln!("Failed to send message to Slack: {}", e),
            };

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
