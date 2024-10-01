# check-pjsip-state

Uses the `asterisk` api to check the status of `pjsip` endpoints.
If the state changes, the application calls the provided Slack 
endpoint. 

Setup the `config.toml` file to provide your slack web hook url 
and optionally tweak the timings. 

Then run the software with e.g. `cargo run`.