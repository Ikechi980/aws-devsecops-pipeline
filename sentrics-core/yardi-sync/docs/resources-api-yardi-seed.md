# Seed Resources API with a Yardi-linked community

Create a community in the local `resources-api` and attach Yardi integration fields, then add two room locations and two residents that reference the mock Yardi IDs.

## Base URL
- `http://127.0.0.1:9000/lambda-url/resources-api`

## Curl requests
```bash
BASE_URL="http://127.0.0.1:9000/lambda-url/resources-api"

# Create community with Yardi integration fields.
COMMUNITY_JSON=$(curl -s -X POST "$BASE_URL/v1/communities" \
  -H "Content-Type: application/json" \
  -d @- <<'JSON'
{
  "name": "Demo Community",
  "yardi_org_id": "community-1001",
  "yardi_api_key": "yardi-demo-key",
  "yardi_api_secret": "yardi-demo-secret",
  "yardi_api_base_url": "http://localhost:3001",
  "yardi_token_url": "http://localhost:3001/oauth/token"
}
JSON
)

COMMUNITY_ID=$(printf "%s" "$COMMUNITY_JSON" | jq -r '.id')
echo "Community ID: $COMMUNITY_ID"

# Create two room locations (yardi_reference_id ties to mock Yardi Location ids).
ROOM1_JSON=$(curl -s -X POST "$BASE_URL/v1/communities/$COMMUNITY_ID/locations" \
  -H "Content-Type: application/json" \
  -d '{"name":"Room 101","yardi_reference_id":"room-101"}')
ROOM1_ID=$(printf "%s" "$ROOM1_JSON" | jq -r '.id')

ROOM2_JSON=$(curl -s -X POST "$BASE_URL/v1/communities/$COMMUNITY_ID/locations" \
  -H "Content-Type: application/json" \
  -d '{"name":"Room 102","yardi_reference_id":"room-102"}')
ROOM2_ID=$(printf "%s" "$ROOM2_JSON" | jq -r '.id')

# Create two residents (yardi_reference_id ties to mock Yardi Patient ids).
curl -s -X POST "$BASE_URL/v1/communities/$COMMUNITY_ID/residents" \
  -H "Content-Type: application/json" \
  -d @- <<JSON
{"first_name":"Ava","last_name":"Reed","location_id":"$ROOM1_ID","yardi_reference_id":"resident-1"}
JSON

curl -s -X POST "$BASE_URL/v1/communities/$COMMUNITY_ID/residents" \
  -H "Content-Type: application/json" \
  -d @- <<JSON
{"first_name":"Liam","last_name":"Nguyen","location_id":"$ROOM2_ID","yardi_reference_id":"resident-2"}
JSON
```

## Expected responses
- Community create: HTTP 201 with JSON body containing `id`
- Location/resident create: HTTP 201 with JSON body containing `id`

## Notes
- The Yardi fields must be provided together; partial payloads are rejected.
- If you do not have `jq`, replace the `COMMUNITY_ID`, `ROOM1_ID`, and `ROOM2_ID` assignments with the UUIDs from the JSON responses.
