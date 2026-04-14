//! Mock Yardi API client for integration tests.
//!
//! This module provides helpers to configure the mock-yardi-api service
//! for testing different scenarios.

use serde_json::json;

/// Client for interacting with the mock Yardi API admin endpoints.
pub struct MockYardiClient {
    http: reqwest::Client,
    base_url: String,
}

impl MockYardiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.to_string(),
        }
    }

    /// Resets all mock state (organizations, tokens, failures).
    pub async fn reset(&self) -> anyhow::Result<()> {
        self.http
            .post(format!("{}/admin/reset", self.base_url))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Creates an organization with the given credentials and data.
    pub async fn create_organization(&self, config: &OrganizationConfig) -> anyhow::Result<()> {
        self.http
            .post(format!("{}/admin/organizations", self.base_url))
            .json(&json!({
                "apiKey": config.api_key,
                "apiSecret": config.api_secret,
                "tokenTtlSeconds": config.token_ttl_seconds,
                "organizations": [{
                    "organizationId": config.org_id,
                    "locations": config.locations_bundle(),
                    "patients": config.patients_bundle(),
                    "encounters": config.encounters_bundle(),
                }]
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Updates an organization's data.
    pub async fn update_organization(
        &self,
        api_key: &str,
        org_id: &str,
        locations: Option<serde_json::Value>,
        patients: Option<serde_json::Value>,
        encounters: Option<serde_json::Value>,
    ) -> anyhow::Result<()> {
        let mut body = json!({ "apiKey": api_key });
        if let Some(l) = locations {
            body["locations"] = l;
        }
        if let Some(p) = patients {
            body["patients"] = p;
        }
        if let Some(e) = encounters {
            body["encounters"] = e;
        }

        self.http
            .put(format!("{}/admin/organizations/{}", self.base_url, org_id))
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Configures failures for subsequent requests.
    pub async fn set_failures(&self, config: &FailureConfig) -> anyhow::Result<()> {
        self.http
            .post(format!("{}/admin/failures", self.base_url))
            .json(&json!({
                "tokenStatus": config.token_status,
                "tokenBody": config.token_body,
                "fhirStatus": config.fhir_status,
                "fhirBody": config.fhir_body,
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Clears any configured failures.
    pub async fn clear_failures(&self) -> anyhow::Result<()> {
        self.set_failures(&FailureConfig::default()).await
    }

    /// Invalidates all tokens for a given API key.
    pub async fn invalidate_tokens(&self, api_key: &str) -> anyhow::Result<()> {
        self.http
            .post(format!("{}/admin/invalidate", self.base_url))
            .json(&json!({ "apiKey": api_key }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Gets the request log from the mock.
    pub async fn get_requests(
        &self,
        request_type: Option<&str>,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let mut url = format!("{}/admin/requests", self.base_url);
        if let Some(t) = request_type {
            url = format!("{}?type={}", url, t);
        }

        let response = self.http.get(&url).send().await?.error_for_status()?;
        Ok(response.json().await?)
    }

    /// Clears the request log.
    pub async fn clear_requests(&self) -> anyhow::Result<()> {
        self.http
            .delete(format!("{}/admin/requests", self.base_url))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Checks if the mock is healthy.
    pub async fn health_check(&self) -> anyhow::Result<bool> {
        match self
            .http
            .get(format!("{}/health", self.base_url))
            .send()
            .await
        {
            Ok(r) => Ok(r.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

/// Configuration for creating an organization in mock Yardi.
#[derive(Debug, Clone)]
pub struct OrganizationConfig {
    pub org_id: String,
    pub api_key: String,
    pub api_secret: String,
    pub token_ttl_seconds: u64,
    pub locations_page_size: Option<usize>,
    pub patients_page_size: Option<usize>,
    pub encounters_page_size: Option<usize>,
    pub locations: Vec<MockLocation>,
    pub patients: Vec<MockPatient>,
    pub encounters: Vec<MockEncounter>,
}

impl OrganizationConfig {
    pub fn new(org_id: &str, api_key: &str, api_secret: &str) -> Self {
        Self {
            org_id: org_id.to_string(),
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
            token_ttl_seconds: 300,
            locations_page_size: None,
            patients_page_size: None,
            encounters_page_size: None,
            locations: Vec::new(),
            patients: Vec::new(),
            encounters: Vec::new(),
        }
    }

    pub fn with_token_ttl(mut self, seconds: u64) -> Self {
        self.token_ttl_seconds = seconds;
        self
    }

    pub fn with_location(mut self, location: MockLocation) -> Self {
        self.locations.push(location);
        self
    }

    pub fn with_patient(mut self, patient: MockPatient) -> Self {
        self.patients.push(patient);
        self
    }

    pub fn with_encounter(mut self, encounter: MockEncounter) -> Self {
        self.encounters.push(encounter);
        self
    }

    pub fn with_locations_page_size(mut self, page_size: usize) -> Self {
        self.locations_page_size = Some(page_size);
        self
    }

    pub fn with_patients_page_size(mut self, page_size: usize) -> Self {
        self.patients_page_size = Some(page_size);
        self
    }

    pub fn with_encounters_page_size(mut self, page_size: usize) -> Self {
        self.encounters_page_size = Some(page_size);
        self
    }

    /// Adds a resident (patient + encounter pair) to the org.
    pub fn with_resident(
        mut self,
        id: &str,
        first_name: &str,
        last_name: &str,
        location_id: &str,
    ) -> Self {
        self.patients.push(MockPatient {
            id: id.to_string(),
            first_name: first_name.to_string(),
            last_name: last_name.to_string(),
            photo_content_type: None,
            photo_data_base64: None,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        });
        self.encounters.push(MockEncounter {
            id: format!("enc-{}", id),
            patient_id: id.to_string(),
            status: "in-progress".to_string(),
            location_id: Some(location_id.to_string()),
            period_start: Some(chrono::Utc::now().to_rfc3339()),
        });
        self
    }

    fn locations_bundle(&self) -> serde_json::Value {
        let entries: Vec<serde_json::Value> = self
            .locations
            .iter()
            .map(|l| {
                json!({
                    "resource": {
                        "id": l.id,
                        "name": l.name,
                        "physicalType": {
                            "coding": [{
                                "code": l.location_type.code()
                            }]
                        },
                        "partOf": l.parent_id.as_ref().map(|p| json!({
                            "reference": format!("Location/{}", p)
                        }))
                    }
                })
            })
            .collect();

        json!({
            "resourceType": "Bundle",
            "mockPageSize": self.locations_page_size,
            "entry": entries
        })
    }

    fn patients_bundle(&self) -> serde_json::Value {
        let entries: Vec<serde_json::Value> = self
            .patients
            .iter()
            .map(|p| {
                json!({
                    "resource": {
                        "id": p.id,
                        "meta": {
                            "lastUpdated": p.last_updated.clone().unwrap_or_else(|| chrono::Utc::now().to_rfc3339())
                        },
                        "name": [{
                            "use": "usual",
                            "family": p.last_name,
                            "given": [p.first_name]
                        }],
                        "photo": if let (Some(content_type), Some(data)) = (&p.photo_content_type, &p.photo_data_base64) {
                            json!([{
                                "contentType": content_type,
                                "data": data
                            }])
                        } else {
                            json!(null)
                        }
                    }
                })
            })
            .collect();

        json!({
            "resourceType": "Bundle",
            "mockPageSize": self.patients_page_size,
            "entry": entries
        })
    }

    fn encounters_bundle(&self) -> serde_json::Value {
        let entries: Vec<serde_json::Value> = self
            .encounters
            .iter()
            .map(|e| {
                let mut resource = json!({
                    "id": e.id,
                    "status": e.status,
                    "subject": {
                        "reference": format!("Patient/{}", e.patient_id)
                    }
                });

                if let Some(loc_id) = &e.location_id {
                    resource["location"] = json!([{
                        "location": {
                            "reference": format!("Location/{}", loc_id)
                        }
                    }]);
                }

                if let Some(start) = &e.period_start {
                    resource["period"] = json!({ "start": start });
                }

                json!({ "resource": resource })
            })
            .collect();

        json!({
            "resourceType": "Bundle",
            "mockPageSize": self.encounters_page_size,
            "entry": entries
        })
    }
}

#[derive(Debug, Clone)]
pub struct MockLocation {
    pub id: String,
    pub name: String,
    pub location_type: LocationType,
    pub parent_id: Option<String>,
}

impl MockLocation {
    pub fn room(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            location_type: LocationType::Room,
            parent_id: None,
        }
    }

    pub fn bed(id: &str, name: &str, parent_id: Option<&str>) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            location_type: LocationType::Bed,
            parent_id: parent_id.map(String::from),
        }
    }

    pub fn site(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            location_type: LocationType::Site,
            parent_id: None,
        }
    }

    pub fn corridor(id: &str, name: &str, parent_id: Option<&str>) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            location_type: LocationType::Corridor,
            parent_id: parent_id.map(String::from),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LocationType {
    Site,
    Corridor,
    Room,
    Bed,
}

impl LocationType {
    pub fn code(&self) -> &'static str {
        match self {
            LocationType::Site => "si",
            LocationType::Corridor => "co",
            LocationType::Room => "ro",
            LocationType::Bed => "bd",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MockPatient {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub photo_content_type: Option<String>,
    pub photo_data_base64: Option<String>,
    pub last_updated: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MockEncounter {
    pub id: String,
    pub patient_id: String,
    pub status: String,
    pub location_id: Option<String>,
    pub period_start: Option<String>,
}

impl MockEncounter {
    pub fn planned(id: &str, patient_id: &str) -> Self {
        Self {
            id: id.to_string(),
            patient_id: patient_id.to_string(),
            status: "planned".to_string(),
            location_id: None,
            period_start: None,
        }
    }

    pub fn finished(id: &str, patient_id: &str) -> Self {
        Self {
            id: id.to_string(),
            patient_id: patient_id.to_string(),
            status: "finished".to_string(),
            location_id: None,
            period_start: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FailureConfig {
    pub token_status: Option<u16>,
    pub token_body: Option<serde_json::Value>,
    pub fhir_status: Option<u16>,
    pub fhir_body: Option<serde_json::Value>,
}

impl FailureConfig {
    pub fn fhir_internal_error() -> Self {
        Self {
            fhir_status: Some(500),
            ..Default::default()
        }
    }
}
