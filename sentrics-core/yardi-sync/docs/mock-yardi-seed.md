# Seed the mock Yardi API with a community

Use the mock admin endpoint to load a community (organization) with two rooms and two residents.

## Base URL
- `http://localhost:3001` (from `../infra/docker-compose.yml`)

## Curl request
```bash
curl -i -X POST "http://localhost:3001/admin/organizations" \
  -H "Content-Type: application/json" \
  -d @- <<'JSON'
{
  "apiKey": "yardi-demo-key",
  "apiSecret": "yardi-demo-secret",
  "tokenTtlSeconds": 300,
  "organizations": [
    {
      "organizationId": "community-1001",
      "locations": {
        "resourceType": "Bundle",
        "entry": [
          {
            "resource": {
              "id": "room-101",
              "name": "Room 101",
              "physicalType": {
                "coding": [
                  { "code": "ro" }
                ]
              }
            }
          },
          {
            "resource": {
              "id": "room-102",
              "name": "Room 102",
              "physicalType": {
                "coding": [
                  { "code": "ro" }
                ]
              }
            }
          }
        ]
      },
      "patients": {
        "resourceType": "Bundle",
        "entry": [
          {
            "resource": {
              "id": "resident-1",
              "name": [
                {
                  "use": "usual",
                  "family": "Reed",
                  "given": [ "Ava" ]
                }
              ]
            }
          },
          {
            "resource": {
              "id": "resident-2",
              "name": [
                {
                  "use": "usual",
                  "family": "Nguyen",
                  "given": [ "Liam" ]
                }
              ]
            }
          }
        ]
      },
      "encounters": {
        "resourceType": "Bundle",
        "entry": [
          {
            "resource": {
              "id": "enc-resident-1",
              "status": "in-progress",
              "subject": { "reference": "Patient/resident-1" },
              "location": [
                {
                  "location": { "reference": "Location/room-101" }
                }
              ],
              "period": { "start": "2024-01-15T10:00:00Z" }
            }
          },
          {
            "resource": {
              "id": "enc-resident-2",
              "status": "in-progress",
              "subject": { "reference": "Patient/resident-2" },
              "location": [
                {
                  "location": { "reference": "Location/room-102" }
                }
              ],
              "period": { "start": "2024-01-15T11:00:00Z" }
            }
          }
        ]
      }
    }
  ]
}
JSON
```

## Expected response
- HTTP 204 No Content

## Verify seeded data
```bash
# Get an access token.
TOKEN=$(curl -s -X POST "http://localhost:3001/oauth/token" \
  -H "Authorization: Basic $(printf '%s' 'yardi-demo-key:yardi-demo-secret' | base64)" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=client_credentials" | jq -r '.access_token')

# Fetch locations, patients, and encounters for the org.
curl -s -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3001/community-1001/Location" | jq

curl -s -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3001/community-1001/Patient" | jq

curl -s -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3001/community-1001/Encounter" | jq
```

## Notes
- The `apiKey` and `apiSecret` are used by `/oauth/token` for auth when yardi-sync fetches data.
- Update `organizationId` to match the org id configured in your yardi-sync environment.
