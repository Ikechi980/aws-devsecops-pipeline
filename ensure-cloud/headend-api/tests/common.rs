use std::env;

pub fn base_url() -> String {
    env::var("HEADEND_API_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9202/lambda-url/headend-api".to_string())
}
