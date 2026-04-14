//! Client identity extraction from mTLS headers.
//!
//! When deployed behind AWS ALB with mTLS enabled, the client's Distinguished Name
//! is passed via the X-Amzn-Mtls-Clientcert-Subject header.
//! This module extracts and normalizes the community ID from the certificate CN.

/// Extracts the client identity from ALB mTLS headers.
///
/// Parses `X-Amzn-Mtls-Clientcert-Subject`, extracts CN from RFC2253 or slash-delimited
/// subject formats, and normalizes `<community>.ensurelink.net` to lowercase `community`.
pub fn extract_client_id(headers: &axum::http::HeaderMap) -> Result<String, ClientIdError> {
    let subject_header = headers
        .get("X-Amzn-Mtls-Clientcert-Subject")
        .ok_or(ClientIdError::MissingHeader)?;

    let subject_str = subject_header
        .to_str()
        .map_err(|_| ClientIdError::InvalidHeader)?;

    let cn = extract_cn_from_subject(subject_str)?;
    normalize_community_id_from_cn(&cn)
}

/// Extracts CN from an RFC2253 or slash-delimited Distinguished Name string.
///
/// Example inputs:
/// - "CN=alpha.ensurelink.net,OU=IoT,O=Example,C=US"
/// - "/C=US/ST=MN/CN=alpha.ensurelink.net"
fn extract_cn_from_subject(subject: &str) -> Result<String, ClientIdError> {
    subject
        .split(',')
        .flat_map(|segment| segment.split('/'))
        .map(str::trim)
        .find_map(|segment| {
            if segment.len() >= 3 && segment[..3].eq_ignore_ascii_case("CN=") {
                Some(segment[3..].trim().to_string())
            } else {
                None
            }
        })
        .ok_or(ClientIdError::NoCnFound)
}

fn normalize_community_id_from_cn(cn: &str) -> Result<String, ClientIdError> {
    let suffix = ".ensurelink.net";
    let lower = cn.trim().to_ascii_lowercase();
    let Some(community_id) = lower.strip_suffix(suffix) else {
        return Err(ClientIdError::InvalidCnFormat);
    };

    if community_id.is_empty() || !is_valid_community_id(community_id) {
        return Err(ClientIdError::InvalidCnFormat);
    }

    Ok(community_id.to_string())
}

fn is_valid_community_id(value: &str) -> bool {
    value
        .bytes()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

#[derive(Debug)]
pub enum ClientIdError {
    MissingHeader,
    InvalidHeader,
    NoCnFound,
    InvalidCnFormat,
}

impl std::fmt::Display for ClientIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientIdError::MissingHeader => {
                write!(f, "Missing X-Amzn-Mtls-Clientcert-Subject header")
            }
            ClientIdError::InvalidHeader => write!(f, "Invalid header encoding"),
            ClientIdError::NoCnFound => write!(f, "No Common Name found in subject"),
            ClientIdError::InvalidCnFormat => {
                write!(f, "CN must be <community-id>.ensurelink.net")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_cn_from_subject, normalize_community_id_from_cn};

    #[test]
    fn parses_cn_from_rfc2253_subject() {
        let cn = extract_cn_from_subject("CN=dk-6.ensurelink.net,OU=IoT,O=Ensure,C=US").unwrap();
        assert_eq!(cn, "dk-6.ensurelink.net");
    }

    #[test]
    fn parses_cn_from_slash_subject() {
        let cn = extract_cn_from_subject("/C=US/ST=MN/CN=DK-6.ensurelink.net").unwrap();
        assert_eq!(cn, "DK-6.ensurelink.net");
    }

    #[test]
    fn normalizes_mixed_case_cn_to_lowercase_community_id() {
        let community_id = normalize_community_id_from_cn("DK-6.ensurelink.net").unwrap();
        assert_eq!(community_id, "dk-6");
    }

    #[test]
    fn rejects_cn_with_missing_suffix() {
        assert!(normalize_community_id_from_cn("dk-6").is_err());
    }

    #[test]
    fn rejects_cn_with_empty_community_segment() {
        assert!(normalize_community_id_from_cn(".ensurelink.net").is_err());
    }

    #[test]
    fn rejects_cn_with_invalid_community_characters() {
        assert!(normalize_community_id_from_cn("dk_6.ensurelink.net").is_err());
    }
}
