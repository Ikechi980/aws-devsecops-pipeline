use crate::error::AppError;
use axum::{extract::FromRequestParts, http::request::Parts};
use lambda_http::request::RequestContext;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Requester {
    EntraUser {
        username: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    IamAssumedRole {
        account_id: String,
        role_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        role_path: Option<String>,
        session_name: String,
    },
    IamUser {
        account_id: String,
        user_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_path: Option<String>,
    },
    IamFederatedUser {
        account_id: String,
        user_name: String,
    },
    IamRoot {
        account_id: String,
    },
    LocalDev,
}

impl Requester {
    fn from_apigw_v2(
        ctx: &lambda_http::aws_lambda_events::apigw::ApiGatewayV2httpRequestContext,
    ) -> Result<Self, AppError> {
        if let Some(authorizer) = &ctx.authorizer {
            let entra_user = authorizer.jwt.as_ref().and_then(|jwt| {
                jwt.claims
                    .get("preferred_username")
                    .map(|username| Requester::EntraUser {
                        username: username.clone(),
                        name: jwt.claims.get("name").cloned(),
                    })
            });
            let requester = entra_user.or_else(|| {
                authorizer
                    .iam
                    .as_ref()
                    .and_then(|iam| iam.user_arn.as_deref())
                    .and_then(Requester::from_iam_arn)
            });
            return match requester {
                Some(requester) => Ok(requester),
                None => Requester::local_dev_or_unauthorized("V2 context"),
            };
        }

        Requester::local_dev_or_unauthorized("V2 context")
    }
}

impl Requester {
    fn from_iam_arn(arn: &str) -> Option<Self> {
        let parsed = match ArnParts::parse(arn) {
            Some(parsed) => parsed,
            None => {
                tracing::warn!(arn, "Unrecognized ARN format for requester");
                return None;
            }
        };
        let requester = match parsed.service.as_str() {
            "iam" => parse_iam_resource(&parsed.account_id, &parsed.resource),
            "sts" => parse_sts_resource(&parsed.account_id, &parsed.resource),
            _ => None,
        };
        if requester.is_none() {
            tracing::warn!(
                arn,
                service = %parsed.service,
                resource = %parsed.resource,
                "Unsupported IAM/STS ARN for requester"
            );
        }
        requester
    }

    fn local_dev_or_unauthorized(context: &str) -> Result<Self, AppError> {
        if std::env::var("ALLOW_UNAUTHENTICATED").is_ok() {
            tracing::warn!(
                "ALLOW_UNAUTHENTICATED is set - using local dev identity in {}. This should never be enabled in production!",
                context
            );
            return Ok(Requester::LocalDev);
        }

        Err(AppError::internal_server_error(
            "api_gateway_auth_missing",
            "Missing authentication in API Gateway request context. Ensure JWT or IAM auth is configured.",
        ))
    }
}

#[derive(Debug)]
struct ArnParts {
    service: String,
    account_id: String,
    resource: String,
}

impl ArnParts {
    fn parse(arn: &str) -> Option<Self> {
        let mut parts = arn.splitn(6, ':');
        let prefix = parts.next()?;
        if prefix != "arn" {
            return None;
        }

        let _partition = parts.next()?;
        let service = parts.next()?.to_string();
        let _region = parts.next()?;
        let account_id = parts.next()?.to_string();
        let resource = parts.next()?.to_string();
        Some(Self {
            service,
            account_id,
            resource,
        })
    }
}

fn parse_iam_resource(account_id: &str, resource: &str) -> Option<Requester> {
    if resource == "root" {
        return Some(Requester::IamRoot {
            account_id: account_id.to_string(),
        });
    }

    if let Some(remainder) = resource.strip_prefix("user/") {
        let (user_name, user_path) = split_name_and_path(remainder);
        return Some(Requester::IamUser {
            account_id: account_id.to_string(),
            user_name,
            user_path,
        });
    }

    None
}

fn parse_sts_resource(account_id: &str, resource: &str) -> Option<Requester> {
    if let Some(remainder) = resource.strip_prefix("assumed-role/")
        && let Some((role_full, session_name)) = remainder.rsplit_once('/')
    {
        let (role_name, role_path) = split_name_and_path(role_full);
        return Some(Requester::IamAssumedRole {
            account_id: account_id.to_string(),
            role_name,
            role_path,
            session_name: session_name.to_string(),
        });
    }

    if let Some(remainder) = resource.strip_prefix("federated-user/") {
        let (user_name, _user_path) = split_name_and_path(remainder);
        return Some(Requester::IamFederatedUser {
            account_id: account_id.to_string(),
            user_name,
        });
    }

    None
}

fn split_name_and_path(value: &str) -> (String, Option<String>) {
    match value.rsplit_once('/') {
        Some((path, name)) if !path.is_empty() => (name.to_string(), Some(path.to_string())),
        Some((_path, name)) => (name.to_string(), None),
        None => (value.to_string(), None),
    }
}

impl<S> FromRequestParts<S> for Requester
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let request_context = parts.extensions.get::<RequestContext>();

        match request_context {
            Some(RequestContext::ApiGatewayV2(ctx)) => Requester::from_apigw_v2(ctx),
            Some(_) => Err(AppError::internal_server_error(
                "api_gateway_type_unsupported",
                "Unsupported API Gateway type. HTTP APIs (v2) are required.",
            )),
            None => {
                // `cargo lambda watch` doesn't provide API Gateway context, so we need
                // an escape hatch for local development. Never enable this in production.
                if std::env::var("ALLOW_UNAUTHENTICATED").is_ok() {
                    tracing::warn!(
                        "ALLOW_UNAUTHENTICATED is set - using local-dev identity. This should never be enabled in production!"
                    );
                    Ok(Requester::LocalDev)
                } else {
                    Err(AppError::internal_server_error(
                        "api_gateway_context_missing",
                        "Missing API Gateway request context. Requests must be authenticated with Entra ID or IAM.",
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lambda_http::aws_lambda_events::apigw::ApiGatewayV2httpRequestContext;

    #[test]
    fn test_parse_assumed_role_arn() {
        let arn = "arn:aws:sts::123456789012:assumed-role/MyServiceRole/session-id";
        let requester = Requester::from_iam_arn(arn).unwrap();
        match requester {
            Requester::IamAssumedRole {
                account_id,
                role_name,
                role_path,
                session_name,
                ..
            } => {
                assert_eq!(account_id, "123456789012");
                assert_eq!(role_name, "MyServiceRole");
                assert_eq!(role_path, None);
                assert_eq!(session_name, "session-id");
            }
            _ => panic!("Expected assumed role requester"),
        }
    }

    #[test]
    fn test_parse_user_arn_with_path() {
        let arn = "arn:aws:iam::123456789012:user/division_abc/subdivision_xyz/Jane";
        let requester = Requester::from_iam_arn(arn).unwrap();
        match requester {
            Requester::IamUser {
                account_id,
                user_name,
                user_path,
                ..
            } => {
                assert_eq!(account_id, "123456789012");
                assert_eq!(user_name, "Jane");
                assert_eq!(user_path, Some("division_abc/subdivision_xyz".to_string()));
            }
            _ => panic!("Expected IAM user requester"),
        }
    }

    #[test]
    fn test_parse_federated_user_arn() {
        let arn = "arn:aws:sts::123456789012:federated-user/my-federated-user-name";
        let requester = Requester::from_iam_arn(arn).unwrap();
        match requester {
            Requester::IamFederatedUser {
                account_id,
                user_name,
                ..
            } => {
                assert_eq!(account_id, "123456789012");
                assert_eq!(user_name, "my-federated-user-name");
            }
            _ => panic!("Expected federated user requester"),
        }
    }

    #[test]
    fn test_parse_root_arn() {
        let arn = "arn:aws:iam::123456789012:root";
        let requester = Requester::from_iam_arn(arn).unwrap();
        match requester {
            Requester::IamRoot { account_id, .. } => {
                assert_eq!(account_id, "123456789012");
            }
            _ => panic!("Expected root requester"),
        }
    }

    #[test]
    fn test_parse_invalid_arn() {
        let arn = "arn:aws:s3:::my-bucket";
        assert!(Requester::from_iam_arn(arn).is_none());
    }

    #[test]
    fn test_parse_iam_role_arn_not_supported() {
        let arn = "arn:aws:iam::123456789012:role/MyRoleName";
        assert!(Requester::from_iam_arn(arn).is_none());
    }

    fn create_v2_entra_context(
        username: &str,
        name: Option<&str>,
    ) -> ApiGatewayV2httpRequestContext {
        let name_field = name
            .map(|n| format!(r#", "name": "{n}""#))
            .unwrap_or_default();
        let json = format!(
            r#"{{
            "authorizer": {{
                "jwt": {{
                    "claims": {{
                        "preferred_username": "{username}"{name_field}
                    }}
                }}
            }},
            "http": {{
                "method": "GET",
                "path": "/test"
            }}
        }}"#
        );
        serde_json::from_str(&json).expect("Failed to parse V2 Entra context")
    }

    fn create_v2_iam_context(user_arn: &str) -> ApiGatewayV2httpRequestContext {
        let json = format!(
            r#"{{
            "authorizer": {{
                "iam": {{
                    "userArn": "{user_arn}"
                }}
            }},
            "http": {{
                "method": "GET",
                "path": "/test"
            }}
        }}"#
        );
        serde_json::from_str(&json).expect("Failed to parse V2 IAM context")
    }

    #[test]
    fn test_v2_entra_user_with_name() {
        let ctx = create_v2_entra_context("john.doe@example.com", Some("John Doe"));
        let result = Requester::from_apigw_v2(&ctx).unwrap();

        match result {
            Requester::EntraUser { username, name } => {
                assert_eq!(username, "john.doe@example.com");
                assert_eq!(name, Some("John Doe".to_string()));
            }
            _ => panic!("Expected Entra user requester"),
        }
    }

    #[test]
    fn test_v2_entra_user_without_name() {
        let ctx = create_v2_entra_context("jane.doe@example.com", None);
        let result = Requester::from_apigw_v2(&ctx).unwrap();

        match result {
            Requester::EntraUser { username, name } => {
                assert_eq!(username, "jane.doe@example.com");
                assert_eq!(name, None);
            }
            _ => panic!("Expected Entra user requester"),
        }
    }

    #[test]
    fn test_v2_iam_service() {
        let ctx = create_v2_iam_context(
            "arn:aws:sts::123456789012:assumed-role/NotificationService/session456",
        );
        let result = Requester::from_apigw_v2(&ctx).unwrap();

        match result {
            Requester::IamAssumedRole { role_name, .. } => {
                assert_eq!(role_name, "NotificationService");
            }
            _ => panic!("Expected assumed role requester"),
        }
    }

    #[test]
    fn test_v2_no_auth_fails() {
        let json = r#"{"http": {"method": "GET", "path": "/test"}}"#;
        let ctx: ApiGatewayV2httpRequestContext = serde_json::from_str(json).unwrap();
        let result = Requester::from_apigw_v2(&ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_requester_serialization_entra_user() {
        let requester = Requester::EntraUser {
            username: "john.doe".to_string(),
            name: Some("John Doe".to_string()),
        };
        let json = serde_json::to_value(&requester).unwrap();
        assert_eq!(json["type"], "entra_user");
        assert_eq!(json["username"], "john.doe");
        assert_eq!(json["name"], "John Doe");
    }

    #[test]
    fn test_requester_serialization_entra_user_without_name() {
        let requester = Requester::EntraUser {
            username: "jane.doe".to_string(),
            name: None,
        };
        let json = serde_json::to_value(&requester).unwrap();
        assert_eq!(json["type"], "entra_user");
        assert_eq!(json["username"], "jane.doe");
        assert!(json.get("name").is_none());
    }

    #[test]
    fn test_requester_serialization_assumed_role() {
        let requester = Requester::IamAssumedRole {
            account_id: "123456789012".to_string(),
            role_name: "MyService".to_string(),
            role_path: None,
            session_name: "session".to_string(),
        };
        let json = serde_json::to_value(&requester).unwrap();
        assert_eq!(json["type"], "iam_assumed_role");
        assert_eq!(json["role_name"], "MyService");
    }
}
