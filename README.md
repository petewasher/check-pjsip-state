# check-pjsip-state

Uses the `asterisk` api to check the status of `pjsip` endpoints.
If the state changes, the application calls the provided Slack 
endpoint. 

Create a Slack api token by following the steps [here](https://api.slack.com/tutorials/tracks/getting-a-token).
Then when presented with the configuration, install in your 
organisation, and use the Bot User OAuth Token. 

Setup the `config.toml` file to provide your Slack api token 
and optionally tweak the timings. 

Then run the software with e.g. `cargo run`.

## Targets
```
# Tools - also requires Docker
cargo install cross --git https://github.com/cross-rs/cross

# X86
cargo build

# Pi 3
cross build --target=aarch64-unknown-linux-gnu
```