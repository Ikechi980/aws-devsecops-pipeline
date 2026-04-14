# Errors

Unless noted, error responses include both `error` and `reason`. Framework-level rejections (invalid UUIDs, invalid JSON bodies) return `400` without a `reason` field. These framework-level 400s can occur on any endpoint that parses path parameters or JSON bodies.

## GET /v1/health
- 500 database_connection_failed

## GET /v1/communities
- 500 internal_server_error

## POST /v1/communities
- 400 name_required
- 400 name_empty
- 400 name_too_long
- 400 yardi_fields_incomplete
- 400 yardi_api_base_url_invalid
- 400 yardi_token_url_invalid
- 500 api_gateway_auth_missing
- 500 api_gateway_type_unsupported
- 500 api_gateway_context_missing
- 500 internal_server_error

## GET /v1/communities/{id}
- 404 community_not_found
- 500 internal_server_error

## PUT /v1/communities/{id}
- 400 name_required
- 400 name_empty
- 400 name_too_long
- 400 yardi_fields_incomplete
- 400 yardi_api_base_url_invalid
- 400 yardi_token_url_invalid
- 500 api_gateway_auth_missing
- 500 api_gateway_type_unsupported
- 500 api_gateway_context_missing
- 404 community_not_found
- 409 yardi_references_present
- 500 internal_server_error

## DELETE /v1/communities/{id}
- 500 api_gateway_auth_missing
- 500 api_gateway_type_unsupported
- 500 api_gateway_context_missing
- 404 community_not_found
- 409 community_has_locations
- 500 internal_server_error

## GET /v1/communities/{community_id}/locations
- 404 community_not_found
- 500 internal_server_error

## POST /v1/communities/{community_id}/locations
- 400 name_required
- 400 name_empty
- 400 name_too_long
- 500 api_gateway_auth_missing
- 500 api_gateway_type_unsupported
- 500 api_gateway_context_missing
- 404 community_not_found
- 409 yardi_integration_required
- 409 yardi_reference_id_conflict
- 500 internal_server_error

## GET /v1/communities/{community_id}/locations/{id}
- 404 location_not_found
- 500 internal_server_error

## PUT /v1/communities/{community_id}/locations/{id}
- 400 name_required
- 400 name_empty
- 400 name_too_long
- 500 api_gateway_auth_missing
- 500 api_gateway_type_unsupported
- 500 api_gateway_context_missing
- 404 location_not_found
- 409 yardi_integration_required
- 409 yardi_reference_id_conflict
- 500 internal_server_error

## DELETE /v1/communities/{community_id}/locations/{id}
- 500 api_gateway_auth_missing
- 500 api_gateway_type_unsupported
- 500 api_gateway_context_missing
- 404 location_not_found
- 409 location_has_residents
- 500 internal_server_error

## GET /v1/communities/{community_id}/residents
- 404 community_not_found
- 400 location_not_found
- 500 internal_server_error

## POST /v1/communities/{community_id}/residents
- 400 first_name_required
- 400 first_name_too_long
- 400 last_name_required
- 400 last_name_empty
- 400 last_name_too_long
- 400 location_id_required
- 400 location_not_found
- 500 api_gateway_auth_missing
- 500 api_gateway_type_unsupported
- 500 api_gateway_context_missing
- 409 yardi_integration_required
- 409 yardi_reference_id_conflict
- 500 internal_server_error

## GET /v1/communities/{community_id}/residents/{id}
- 404 resident_not_found
- 500 internal_server_error

## PUT /v1/communities/{community_id}/residents/{id}
- 400 first_name_required
- 400 first_name_too_long
- 400 last_name_required
- 400 last_name_empty
- 400 last_name_too_long
- 400 location_id_required
- 500 api_gateway_auth_missing
- 500 api_gateway_type_unsupported
- 500 api_gateway_context_missing
- 400 location_not_found
- 404 resident_not_found
- 409 yardi_integration_required
- 409 yardi_reference_id_conflict
- 500 internal_server_error

## DELETE /v1/communities/{community_id}/residents/{id}
- 500 api_gateway_auth_missing
- 500 api_gateway_type_unsupported
- 500 api_gateway_context_missing
- 404 resident_not_found
- 409 resident_has_dependencies
- 500 internal_server_error

## GET /v1/communities/{community_id}/residents/{id}/photo
- 404 resident_not_found
- 404 resident_photo_not_found
- 500 resident_photo_header_invalid
- 500 internal_server_error

## PUT /v1/communities/{community_id}/residents/{id}/photo
- 500 api_gateway_auth_missing
- 500 api_gateway_type_unsupported
- 500 api_gateway_context_missing
- 404 resident_not_found
- 400 resident_photo_content_type_required
- 400 resident_photo_content_type_invalid
- 400 resident_photo_empty
- 413 resident_photo_too_large
- 500 internal_server_error

## DELETE /v1/communities/{community_id}/residents/{id}/photo
- 500 api_gateway_auth_missing
- 500 api_gateway_type_unsupported
- 500 api_gateway_context_missing
- 404 resident_not_found
- 404 resident_photo_not_found
- 500 internal_server_error
