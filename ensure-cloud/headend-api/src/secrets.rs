use anyhow::{Result, anyhow};
use aws_sdk_ssm::Client;
use aws_sdk_ssm::operation::get_parameter::GetParameterOutput;

pub async fn resolve_ssm_parameter(client: &Client, parameter_name: &str) -> Result<String> {
    let response = client
        .get_parameter()
        .name(parameter_name)
        .with_decryption(true)
        .send()
        .await
        .map_err(|err| ssm_fetch_error(parameter_name, err))?;

    extract_parameter_value(parameter_name, &response)
}

fn extract_parameter_value(parameter_name: &str, response: &GetParameterOutput) -> Result<String> {
    let value = response
        .parameter()
        .and_then(|parameter| parameter.value())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("SSM parameter {parameter_name} has no value"))?;

    Ok(value.to_string())
}

fn ssm_fetch_error<E>(parameter_name: &str, err: E) -> anyhow::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    anyhow!(err).context(format!("failed to fetch SSM parameter {parameter_name}"))
}

#[cfg(test)]
mod tests {
    use super::{extract_parameter_value, ssm_fetch_error};
    use aws_sdk_ssm::operation::get_parameter::GetParameterOutput;
    use aws_sdk_ssm::types::Parameter;

    #[test]
    fn rejects_empty_parameter_values() {
        let response = GetParameterOutput::builder()
            .parameter(Parameter::builder().value("   ").build())
            .build();

        let err = extract_parameter_value("/ensure-cloud/headend-api/events-mongo-url", &response)
            .expect_err("empty parameter value should fail");

        assert_eq!(
            err.to_string(),
            "SSM parameter /ensure-cloud/headend-api/events-mongo-url has no value"
        );
    }

    #[test]
    fn adds_context_to_fetch_errors() {
        let err = ssm_fetch_error(
            "/ensure-cloud/headend-api/events-mongo-url",
            std::io::Error::other("boom"),
        );

        assert_eq!(
            err.to_string(),
            "failed to fetch SSM parameter /ensure-cloud/headend-api/events-mongo-url"
        );
        let message = format!("{err:#}");
        assert!(message.contains("boom"));
    }
}
