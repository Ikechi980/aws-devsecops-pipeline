use std::time::Duration;

// AWS SDK operation timeouts keep requests bounded without relying on defaults.
pub const AWS_OPERATION_TIMEOUT_SECS: u64 = 25;
pub const AWS_TIMEOUT_SLACK_SECS: u64 = 5;
// Hard cap is operation timeout plus slack to account for internal retries.
pub const AWS_OPERATION_TIMEOUT: Duration = Duration::from_secs(AWS_OPERATION_TIMEOUT_SECS);
pub const AWS_HARD_TIMEOUT: Duration =
    Duration::from_secs(AWS_OPERATION_TIMEOUT_SECS + AWS_TIMEOUT_SLACK_SECS);
pub const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
// Hold new failure notifications before first publish to reduce alert fatigue.
pub const FAILURE_NOTIFICATION_HOLD: Duration = Duration::from_secs(5 * 60);
// Retry interval for queued failure notifications.
pub const FAILURE_RETRY_INTERVAL: Duration = Duration::from_secs(30);
