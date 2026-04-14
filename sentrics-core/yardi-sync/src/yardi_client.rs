use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::models::{
    FailureNotification, FailureType, YardiCredentials, YardiLocation, YardiLocationType,
    YardiResident,
};

const TOKEN_EXPIRY_BUFFER: Duration = Duration::from_secs(5);

pub struct YardiClient {
    http: reqwest::Client,
    token_cache: Mutex<HashMap<String, CachedToken>>,
}

struct CachedToken {
    value: String,
    expires_at: Instant,
}

#[derive(Debug)]
pub enum YardiError {
    Unreachable(anyhow::Error),
    CredentialsInvalid,
    InvalidData(String),
    UnexpectedResponse(String),
}

impl std::fmt::Display for YardiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            YardiError::Unreachable(e) => write!(f, "Yardi unreachable: {e}"),
            YardiError::CredentialsInvalid => write!(f, "Yardi credentials invalid"),
            YardiError::InvalidData(msg) => write!(f, "Yardi invalid data: {msg}"),
            YardiError::UnexpectedResponse(msg) => write!(f, "Yardi unexpected response: {msg}"),
        }
    }
}

impl std::error::Error for YardiError {}

impl YardiError {
    pub fn to_failure_notification(
        &self,
        community_id: Option<uuid::Uuid>,
        community_name: Option<String>,
    ) -> FailureNotification {
        let (failure_type, message, details) = match self {
            YardiError::Unreachable(e) => (
                FailureType::Unreachable,
                "Yardi API is unreachable".to_string(),
                Some(format!("{e:#}")),
            ),
            YardiError::CredentialsInvalid => (
                FailureType::CredentialsInvalid,
                "Yardi credentials were rejected".to_string(),
                None,
            ),
            YardiError::InvalidData(msg) => {
                (FailureType::DataInvariantViolation, msg.clone(), None)
            }
            YardiError::UnexpectedResponse(msg) => {
                (FailureType::UnexpectedResponse, msg.clone(), None)
            }
        };

        FailureNotification {
            failure_type,
            community_id,
            community_name,
            message,
            details,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

impl YardiClient {
    pub fn new(http: reqwest::Client) -> Arc<Self> {
        Arc::new(Self {
            http,
            token_cache: Mutex::new(HashMap::new()),
        })
    }

    pub async fn fetch_locations(
        self: &Arc<Self>,
        credentials: &YardiCredentials,
    ) -> Result<Vec<YardiLocation>, YardiError> {
        let bundle: FhirBundle<LocationResource> = self
            .fetch_bundle(
                credentials,
                "Location",
                &[("organization", credentials.organization_id.as_str())],
            )
            .await?;

        parse_locations(bundle)
    }

    pub async fn fetch_residents(
        self: &Arc<Self>,
        credentials: &YardiCredentials,
    ) -> Result<Vec<YardiResident>, YardiError> {
        let patients_bundle: FhirBundle<PatientResource> = self
            .fetch_bundle(
                credentials,
                "Patient",
                &[
                    ("organization", credentials.organization_id.as_str()),
                    ("active", "true"),
                ],
            )
            .await?;

        let encounters_bundle: FhirBundle<EncounterResource> =
            self.fetch_bundle(credentials, "Encounter", &[]).await?;

        build_residents(patients_bundle, encounters_bundle)
    }

    pub async fn validate_organization_id(
        self: &Arc<Self>,
        credentials: &YardiCredentials,
    ) -> Result<(), YardiError> {
        let resource = format!("Organization/{}", credentials.organization_id);
        let url = self.resource_url(credentials, &resource, &[])?;
        let response = self.execute_get(credentials, url.as_str()).await?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        let body = response.text().await.unwrap_or_default();
        tracing::warn!(
            organization_id = %credentials.organization_id,
            status = %status,
            body = %body,
            "Yardi organization validation failed"
        );
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(YardiError::CredentialsInvalid);
        }
        Err(YardiError::UnexpectedResponse(format!(
            "Yardi organization {} is invalid or unreadable (status {})",
            credentials.organization_id, status
        )))
    }

    pub async fn fetch_patient_photo(
        self: &Arc<Self>,
        credentials: &YardiCredentials,
        patient_id: &str,
    ) -> Result<Option<PatientPhoto>, YardiError> {
        let resource = format!("Patient/{patient_id}");
        let url = self.resource_url(credentials, &resource, &[])?;
        let response = self.execute_get(credentials, url.as_str()).await?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            tracing::error!(
                patient_id = %patient_id,
                status = %status,
                body = %body,
                "Yardi Patient detail fetch failed"
            );
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(YardiError::CredentialsInvalid);
            }
            return Err(YardiError::UnexpectedResponse(format!(
                "Yardi Patient/{patient_id} returned status {}",
                status
            )));
        }

        let patient: PatientResource = response.json().await.map_err(|e| {
            YardiError::UnexpectedResponse(format!(
                "Failed to parse Yardi patient detail for {patient_id}: {e}"
            ))
        })?;

        let Some(attachment) = patient.photo.and_then(|mut photos| photos.drain(..).next()) else {
            return Ok(None);
        };

        let content_type = attachment.content_type.ok_or_else(|| {
            YardiError::InvalidData(format!(
                "Yardi patient {} photo missing contentType",
                patient_id
            ))
        })?;
        let data = attachment.data.ok_or_else(|| {
            YardiError::InvalidData(format!("Yardi patient {} photo missing data", patient_id))
        })?;

        Ok(Some(PatientPhoto {
            content_type,
            data_base64: data,
        }))
    }

    async fn fetch_bundle<T>(
        &self,
        credentials: &YardiCredentials,
        resource: &str,
        params: &[(&str, &str)],
    ) -> Result<FhirBundle<T>, YardiError>
    where
        T: serde::de::DeserializeOwned + Default,
    {
        let mut url = self.resource_url(credentials, resource, params)?;
        let mut seen_urls = HashSet::new();
        let mut entries = Vec::new();

        loop {
            seen_urls.insert(url.to_string());

            let response = self.execute_get(credentials, url.as_str()).await?;
            let status = response.status();

            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                tracing::error!("Yardi API returned {} for {}: {}", status, resource, body);

                if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::FORBIDDEN
                {
                    return Err(YardiError::CredentialsInvalid);
                }

                return Err(YardiError::UnexpectedResponse(format!(
                    "Yardi API returned status {}",
                    status
                )));
            }

            let bundle: FhirBundle<T> = response.json().await.map_err(|e| {
                YardiError::UnexpectedResponse(format!("Failed to parse Yardi response: {e}"))
            })?;

            entries.extend(bundle.entry);

            let Some(next_url) = resolve_next_page_url(&url, &bundle.link, &seen_urls)? else {
                break;
            };
            url = next_url;
        }

        Ok(FhirBundle {
            entry: entries,
            link: Vec::new(),
        })
    }

    async fn execute_get(
        &self,
        credentials: &YardiCredentials,
        url: &str,
    ) -> Result<reqwest::Response, YardiError> {
        let mut retried = false;

        loop {
            let token = self.get_token(credentials).await?;

            let response = self
                .http
                .get(url)
                .header("Accept", "application/json")
                .bearer_auth(&token)
                .send()
                .await
                .map_err(|e| YardiError::Unreachable(e.into()))?;

            if response.status() == reqwest::StatusCode::UNAUTHORIZED && !retried {
                self.invalidate_token(credentials).await;
                retried = true;
                continue;
            }

            return Ok(response);
        }
    }

    fn resource_url(
        &self,
        credentials: &YardiCredentials,
        resource: &str,
        params: &[(&str, &str)],
    ) -> Result<reqwest::Url, YardiError> {
        reqwest::Url::parse_with_params(
            &format!(
                "{}/{}",
                credentials.api_base_url.trim_end_matches('/'),
                resource
            ),
            params.iter().copied(),
        )
        .map_err(|e| {
            YardiError::UnexpectedResponse(format!(
                "Invalid Yardi URL for resource {resource}: {e}"
            ))
        })
    }

    async fn get_token(&self, credentials: &YardiCredentials) -> Result<String, YardiError> {
        let cache_key = format!(
            "{}::{}::{}",
            credentials.token_url, credentials.api_key, credentials.api_secret
        );

        {
            let guard = self.token_cache.lock().await;
            if let Some(entry) = guard.get(&cache_key)
                && Instant::now() < entry.expires_at
            {
                return Ok(entry.value.clone());
            }
        }

        let form = [("grant_type", "client_credentials"), ("scope", "APIvR4")];
        let response = self
            .http
            .post(&credentials.token_url)
            .header(
                "Content-Type",
                "application/x-www-form-urlencoded; charset=utf-8",
            )
            .basic_auth(&credentials.api_key, Some(&credentials.api_secret))
            .form(&form)
            .send()
            .await
            .map_err(|e| YardiError::Unreachable(e.into()))?;

        let status = response.status();
        if status == reqwest::StatusCode::BAD_REQUEST
            || status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            return Err(YardiError::CredentialsInvalid);
        }

        if !status.is_success() {
            return Err(YardiError::Unreachable(anyhow::anyhow!(
                "Token endpoint returned {}",
                status
            )));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| YardiError::UnexpectedResponse(format!("Invalid token response: {e}")))?;

        let expires_in = Duration::from_secs(token_response.expires_in);
        let expires_at = Instant::now() + expires_in.saturating_sub(TOKEN_EXPIRY_BUFFER);

        let mut guard = self.token_cache.lock().await;
        guard.insert(
            cache_key,
            CachedToken {
                value: token_response.access_token.clone(),
                expires_at,
            },
        );

        Ok(token_response.access_token)
    }

    async fn invalidate_token(&self, credentials: &YardiCredentials) {
        let cache_key = format!(
            "{}::{}::{}",
            credentials.token_url, credentials.api_key, credentials.api_secret
        );
        let mut guard = self.token_cache.lock().await;
        guard.remove(&cache_key);
    }
}

// FHIR response structures

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct FhirBundle<T> {
    #[serde(default)]
    entry: Vec<FhirEntry<T>>,
    #[serde(default)]
    link: Vec<FhirBundleLink>,
}

#[derive(Default, Deserialize)]
struct FhirBundleLink {
    #[serde(default)]
    relation: String,
    url: Option<String>,
}

#[derive(Deserialize)]
struct FhirEntry<T> {
    resource: T,
}

#[derive(Default, Deserialize)]
struct PatientResource {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: Vec<HumanName>,
    meta: Option<ResourceMeta>,
    photo: Option<Vec<Attachment>>,
}

#[derive(Default, Deserialize)]
struct HumanName {
    #[serde(default)]
    family: String,
    #[serde(default, deserialize_with = "deserialize_given_names")]
    given: Vec<String>,
    #[serde(rename = "use")]
    use_field: Option<String>,
}

fn deserialize_given_names<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Vec<String>>::deserialize(deserializer)?;
    Ok(value.unwrap_or_default())
}

#[derive(Default, Deserialize)]
struct EncounterResource {
    #[serde(default)]
    status: String,
    subject: Option<Reference>,
    #[serde(default)]
    location: Vec<EncounterLocation>,
    period: Option<EncounterPeriod>,
    meta: Option<ResourceMeta>,
}

#[derive(Default, Deserialize)]
struct EncounterPeriod {
    start: Option<String>,
}

#[derive(Default, Deserialize)]
struct ResourceMeta {
    #[serde(rename = "lastUpdated")]
    last_updated: Option<String>,
}

#[derive(Default, Deserialize)]
struct Attachment {
    #[serde(rename = "contentType")]
    content_type: Option<String>,
    data: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PatientPhoto {
    pub content_type: String,
    pub data_base64: String,
}

#[derive(Default, Deserialize)]
struct EncounterLocation {
    location: Option<Reference>,
}

#[derive(Default, Deserialize)]
struct Reference {
    reference: Option<String>,
}

#[derive(Default, Deserialize)]
struct LocationResource {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(rename = "physicalType")]
    physical_type: Option<PhysicalType>,
    #[serde(rename = "partOf")]
    part_of: Option<Reference>,
}

#[derive(Default, Deserialize)]
struct PhysicalType {
    #[serde(default)]
    coding: Vec<Coding>,
}

#[derive(Default, Deserialize)]
struct Coding {
    code: Option<String>,
}

// Parsing functions

fn parse_locations(bundle: FhirBundle<LocationResource>) -> Result<Vec<YardiLocation>, YardiError> {
    let mut locations = Vec::new();

    for entry in bundle.entry {
        let resource = entry.resource;
        if resource.id.is_empty() {
            return Err(YardiError::InvalidData(
                "Yardi location missing id".to_string(),
            ));
        }
        if resource.name.is_empty() {
            return Err(YardiError::InvalidData(format!(
                "Yardi location {} missing name",
                resource.id
            )));
        }

        let code = resource
            .physical_type
            .as_ref()
            .and_then(|pt| pt.coding.first())
            .and_then(|c| c.code.as_deref())
            .ok_or_else(|| {
                YardiError::InvalidData(format!("Yardi location {} missing type", resource.id))
            })?;

        let location_type = match code {
            "si" => YardiLocationType::Site,
            "co" => YardiLocationType::Corridor,
            "ro" => YardiLocationType::Room,
            "bd" => YardiLocationType::Bed,
            other => {
                return Err(YardiError::InvalidData(format!(
                    "Yardi location {} has unsupported type: {}",
                    resource.id, other
                )));
            }
        };

        let parent_id = resource
            .part_of
            .as_ref()
            .and_then(|p| p.reference.as_deref())
            .and_then(extract_reference_id)
            .map(|s| s.to_string());

        locations.push(YardiLocation {
            id: resource.id,
            name: resource.name,
            location_type,
            parent_id,
        });
    }

    Ok(locations)
}

fn build_residents(
    patients: FhirBundle<PatientResource>,
    encounters: FhirBundle<EncounterResource>,
) -> Result<Vec<YardiResident>, YardiError> {
    let mut patient_map = HashMap::new();
    for entry in patients.entry {
        let resource = entry.resource;
        if resource.id.is_empty() {
            return Err(YardiError::InvalidData(
                "Yardi patient missing id".to_string(),
            ));
        }
        patient_map.insert(resource.id.clone(), resource);
    }

    let mut encounter_map: HashMap<String, Vec<EncounterResource>> = HashMap::new();
    for entry in encounters.entry {
        let resource = entry.resource;
        let Some(patient_id) = resource
            .subject
            .as_ref()
            .and_then(|s| s.reference.as_deref())
            .and_then(extract_reference_id)
            .map(|s| s.to_string())
        else {
            continue;
        };

        encounter_map.entry(patient_id).or_default().push(resource);
    }

    let mut residents = Vec::new();

    for (patient_id, patient) in patient_map {
        let Some(encounters) = encounter_map.get(&patient_id) else {
            return Err(YardiError::InvalidData(format!(
                "Yardi patient {} has no encounter",
                patient_id
            )));
        };

        let mut sorted_encounters: Vec<&EncounterResource> = encounters.iter().collect();
        sorted_encounters.sort_by(encounter_cmp_desc);

        let encounter = sorted_encounters[0];
        let status = encounter.status.to_lowercase();
        if status == "planned" || status == "finished" {
            continue;
        }

        let (first_name, last_name) = extract_name(&patient)?;

        let mut location_ids = extract_encounter_location_ids(encounter);

        if location_ids.is_empty() && status == "onleave" {
            location_ids = sorted_encounters
                .iter()
                .skip(1)
                .map(|prior| extract_encounter_location_ids(prior))
                .find(|location_ids| !location_ids.is_empty())
                .unwrap_or_default();
        }

        if status == "onleave" && location_ids.is_empty() {
            continue;
        }

        residents.push(YardiResident {
            id: patient_id,
            first_name,
            last_name,
            room_id: None,
            location_ids,
            last_updated: patient.meta.and_then(|m| m.last_updated),
        });
    }

    residents.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(residents)
}

fn extract_name(patient: &PatientResource) -> Result<(String, String), YardiError> {
    let preferred = patient
        .name
        .iter()
        .find(|n| matches!(n.use_field.as_deref(), Some("usual")))
        .or_else(|| patient.name.first())
        .ok_or_else(|| {
            YardiError::InvalidData(format!("Yardi patient {} missing name", patient.id))
        })?;

    let family = preferred.family.trim();
    if family.is_empty() {
        return Err(YardiError::InvalidData(format!(
            "Yardi patient {} missing last name",
            patient.id
        )));
    }

    let first_name = preferred
        .given
        .iter()
        .map(|given| given.trim())
        .filter(|given| !given.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    Ok((first_name, family.to_string()))
}

fn extract_reference_id(reference: &str) -> Option<&str> {
    reference.rsplit('/').next()
}

fn resolve_next_page_url(
    current_url: &reqwest::Url,
    links: &[FhirBundleLink],
    seen_urls: &HashSet<String>,
) -> Result<Option<reqwest::Url>, YardiError> {
    let Some(next_link) = links.iter().find(|link| link.relation == "next") else {
        return Ok(None);
    };

    let raw_url = next_link.url.as_deref().ok_or_else(|| {
        YardiError::UnexpectedResponse("Yardi pagination next link missing url".to_string())
    })?;

    let next_url = current_url.join(raw_url).map_err(|e| {
        YardiError::UnexpectedResponse(format!("Invalid Yardi pagination next URL: {e}"))
    })?;

    if seen_urls.contains(next_url.as_str()) {
        return Err(YardiError::UnexpectedResponse(format!(
            "Yardi pagination repeated next URL: {}",
            next_url
        )));
    }

    Ok(Some(next_url))
}

fn encounter_cmp_desc(a: &&EncounterResource, b: &&EncounterResource) -> std::cmp::Ordering {
    match (encounter_timestamp(a), encounter_timestamp(b)) {
        (Some(a_ts), Some(b_ts)) => b_ts.cmp(&a_ts),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn encounter_timestamp(encounter: &EncounterResource) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Some(period) = encounter.period.as_ref()
        && let Some(start) = period.start.as_deref()
        && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(start)
    {
        return Some(dt.with_timezone(&chrono::Utc));
    }

    if let Some(meta) = encounter.meta.as_ref()
        && let Some(updated) = meta.last_updated.as_deref()
        && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(updated)
    {
        return Some(dt.with_timezone(&chrono::Utc));
    }

    None
}

fn extract_encounter_location_ids(encounter: &EncounterResource) -> Vec<String> {
    encounter
        .location
        .iter()
        .filter_map(|encounter_location| encounter_location.location.as_ref())
        .filter_map(|loc| loc.reference.as_deref())
        .filter_map(extract_reference_id)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle<T>(resources: Vec<T>) -> FhirBundle<T> {
        FhirBundle {
            entry: resources
                .into_iter()
                .map(|resource| FhirEntry { resource })
                .collect(),
            link: Vec::new(),
        }
    }

    #[test]
    fn resolve_next_page_url_supports_relative_links() {
        let current_url =
            reqwest::Url::parse("https://example.com/Patient?active=true").expect("valid URL");
        let links = vec![FhirBundleLink {
            relation: "next".to_string(),
            url: Some("/Patient?_getpagesoffset=100&_count=100".to_string()),
        }];
        let seen_urls = HashSet::from([current_url.to_string()]);

        let next_url = resolve_next_page_url(&current_url, &links, &seen_urls)
            .expect("relative next URL should resolve")
            .expect("expected next URL");

        assert_eq!(
            next_url.as_str(),
            "https://example.com/Patient?_getpagesoffset=100&_count=100"
        );
    }

    #[test]
    fn resolve_next_page_url_rejects_repeated_urls() {
        let current_url = reqwest::Url::parse("https://example.com/Encounter").expect("valid URL");
        let repeated_url = "https://example.com/Encounter?_getpagesoffset=100&_count=100";
        let links = vec![FhirBundleLink {
            relation: "next".to_string(),
            url: Some(repeated_url.to_string()),
        }];
        let seen_urls = HashSet::from([current_url.to_string(), repeated_url.to_string()]);

        let err = resolve_next_page_url(&current_url, &links, &seen_urls)
            .expect_err("repeated next URL should fail");

        assert!(matches!(err, YardiError::UnexpectedResponse(_)));
    }

    #[test]
    fn parse_locations_missing_id() {
        let bundle = bundle(vec![LocationResource {
            id: "".to_string(),
            name: "Room".to_string(),
            physical_type: Some(PhysicalType {
                coding: vec![Coding {
                    code: Some("ro".to_string()),
                }],
            }),
            part_of: None,
        }]);

        let err = parse_locations(bundle).unwrap_err();
        assert!(matches!(err, YardiError::InvalidData(_)));
    }

    #[test]
    fn parse_locations_missing_name() {
        let bundle = bundle(vec![LocationResource {
            id: "loc-1".to_string(),
            name: "".to_string(),
            physical_type: Some(PhysicalType {
                coding: vec![Coding {
                    code: Some("ro".to_string()),
                }],
            }),
            part_of: None,
        }]);

        let err = parse_locations(bundle).unwrap_err();
        assert!(matches!(err, YardiError::InvalidData(_)));
    }

    #[test]
    fn parse_locations_missing_type() {
        let bundle = bundle(vec![LocationResource {
            id: "loc-1".to_string(),
            name: "Room".to_string(),
            physical_type: None,
            part_of: None,
        }]);

        let err = parse_locations(bundle).unwrap_err();
        assert!(matches!(err, YardiError::InvalidData(_)));
    }

    #[test]
    fn parse_locations_unsupported_type() {
        let bundle = bundle(vec![LocationResource {
            id: "loc-1".to_string(),
            name: "Room".to_string(),
            physical_type: Some(PhysicalType {
                coding: vec![Coding {
                    code: Some("xx".to_string()),
                }],
            }),
            part_of: None,
        }]);

        let err = parse_locations(bundle).unwrap_err();
        assert!(matches!(err, YardiError::InvalidData(_)));
    }

    #[test]
    fn build_residents_missing_encounter() {
        let patients = bundle(vec![PatientResource {
            id: "pat-1".to_string(),
            name: vec![HumanName {
                family: "Doe".to_string(),
                given: vec!["John".to_string()],
                use_field: Some("usual".to_string()),
            }],
            meta: None,
            photo: None,
        }]);
        let encounters = bundle(Vec::<EncounterResource>::new());

        let err = build_residents(patients, encounters).unwrap_err();
        assert!(matches!(err, YardiError::InvalidData(_)));
    }

    #[test]
    fn build_residents_missing_name() {
        let patients = bundle(vec![PatientResource {
            id: "pat-1".to_string(),
            name: vec![HumanName {
                family: "".to_string(),
                given: vec![],
                use_field: None,
            }],
            meta: None,
            photo: None,
        }]);
        let encounters = bundle(vec![EncounterResource {
            status: "in-progress".to_string(),
            subject: Some(Reference {
                reference: Some("Patient/pat-1".to_string()),
            }),
            location: vec![],
            period: None,
            meta: None,
        }]);

        let err = build_residents(patients, encounters).unwrap_err();
        assert!(matches!(err, YardiError::InvalidData(_)));
    }

    #[test]
    fn build_residents_null_given_normalized_to_empty_first_name() {
        let patient_json = serde_json::json!({
            "id": "pat-1",
            "name": [{
                "family": "Doe",
                "given": null,
                "use": "usual"
            }]
        });
        let patient: PatientResource =
            serde_json::from_value(patient_json).expect("Failed to deserialize patient");

        let patients = bundle(vec![patient]);
        let encounters = bundle(vec![EncounterResource {
            status: "in-progress".to_string(),
            subject: Some(Reference {
                reference: Some("Patient/pat-1".to_string()),
            }),
            location: vec![],
            period: None,
            meta: None,
        }]);

        let residents = build_residents(patients, encounters).expect("Expected residents");
        assert_eq!(residents.len(), 1);
        assert_eq!(residents[0].first_name, "");
        assert_eq!(residents[0].last_name, "Doe");
    }

    #[test]
    fn build_residents_without_location_keeps_resident() {
        let patients = bundle(vec![PatientResource {
            id: "pat-1".to_string(),
            name: vec![HumanName {
                family: "Doe".to_string(),
                given: vec!["John".to_string()],
                use_field: Some("usual".to_string()),
            }],
            meta: None,
            photo: None,
        }]);
        let encounters = bundle(vec![EncounterResource {
            status: "in-progress".to_string(),
            subject: Some(Reference {
                reference: Some("Patient/pat-1".to_string()),
            }),
            location: vec![],
            period: None,
            meta: None,
        }]);

        let residents = build_residents(patients, encounters).expect("Expected residents");
        assert_eq!(residents.len(), 1);
        assert!(residents[0].location_ids.is_empty());
        assert!(residents[0].room_id.is_none());
    }

    #[test]
    fn build_residents_onleave_uses_prior_location() {
        let patients = bundle(vec![PatientResource {
            id: "pat-1".to_string(),
            name: vec![HumanName {
                family: "Doe".to_string(),
                given: vec!["Jane".to_string()],
                use_field: Some("usual".to_string()),
            }],
            meta: None,
            photo: None,
        }]);
        let encounters = bundle(vec![
            EncounterResource {
                status: "finished".to_string(),
                subject: Some(Reference {
                    reference: Some("Patient/pat-1".to_string()),
                }),
                location: vec![EncounterLocation {
                    location: Some(Reference {
                        reference: Some("Location/room-101".to_string()),
                    }),
                }],
                period: Some(EncounterPeriod {
                    start: Some("2024-01-01T00:00:00Z".to_string()),
                }),
                meta: None,
            },
            EncounterResource {
                status: "onleave".to_string(),
                subject: Some(Reference {
                    reference: Some("Patient/pat-1".to_string()),
                }),
                location: vec![],
                period: Some(EncounterPeriod {
                    start: Some("2024-02-01T00:00:00Z".to_string()),
                }),
                meta: None,
            },
        ]);

        let residents = build_residents(patients, encounters).expect("Expected residents");
        assert_eq!(residents.len(), 1);
        assert_eq!(residents[0].location_ids, vec!["room-101"]);
        assert!(residents[0].room_id.is_none());
    }

    #[test]
    fn build_residents_onleave_without_prior_location_skips_resident() {
        let patients = bundle(vec![PatientResource {
            id: "pat-1".to_string(),
            name: vec![HumanName {
                family: "Doe".to_string(),
                given: vec!["Jane".to_string()],
                use_field: Some("usual".to_string()),
            }],
            meta: None,
            photo: None,
        }]);
        let encounters = bundle(vec![EncounterResource {
            status: "onleave".to_string(),
            subject: Some(Reference {
                reference: Some("Patient/pat-1".to_string()),
            }),
            location: vec![],
            period: Some(EncounterPeriod {
                start: Some("2024-02-01T00:00:00Z".to_string()),
            }),
            meta: None,
        }]);

        let residents = build_residents(patients, encounters).expect("Expected residents");
        assert!(residents.is_empty());
    }

    #[test]
    fn build_residents_keeps_all_encounter_locations() {
        let patients = bundle(vec![PatientResource {
            id: "pat-1".to_string(),
            name: vec![HumanName {
                family: "Doe".to_string(),
                given: vec!["Jane".to_string()],
                use_field: Some("usual".to_string()),
            }],
            meta: None,
            photo: None,
        }]);
        let encounters = bundle(vec![EncounterResource {
            status: "in-progress".to_string(),
            subject: Some(Reference {
                reference: Some("Patient/pat-1".to_string()),
            }),
            location: vec![
                EncounterLocation {
                    location: Some(Reference {
                        reference: Some("Location/site-1".to_string()),
                    }),
                },
                EncounterLocation {
                    location: Some(Reference {
                        reference: Some("Location/room-101".to_string()),
                    }),
                },
                EncounterLocation {
                    location: Some(Reference {
                        reference: Some("Location/bed-9".to_string()),
                    }),
                },
            ],
            period: None,
            meta: None,
        }]);

        let residents = build_residents(patients, encounters).expect("Expected residents");
        assert_eq!(residents.len(), 1);
        assert_eq!(
            residents[0].location_ids,
            vec!["site-1", "room-101", "bed-9"]
        );
        assert!(residents[0].room_id.is_none());
    }

    #[test]
    fn unreachable_failure_maps_to_notification() {
        let err = YardiError::Unreachable(anyhow::anyhow!("boom"));
        let notification = err.to_failure_notification(None, None);
        assert_eq!(notification.failure_type, FailureType::Unreachable);
    }
}
