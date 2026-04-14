# Errors

Unless noted, error responses include both `error` and `reason`. Errors caused by missing or misconfigured API Gateway request context return 500 because they are backend configuration issues.

## GET /v1/health

No application-level errors are returned. This endpoint always returns 200 when the Lambda is running.

## GET /v1/core/community
- 400 ensure_community_id_invalid
- 400 ensure_community_id_missing
- 401 client_certificate_invalid
- 404 core_community_mapping_missing
- 404 core_resource_not_found
- 500 api_gateway_context_missing
- 500 api_gateway_payload_version_unsupported
- 500 api_gateway_type_unsupported
- 500 client_certificate_missing
- 502 ensure360_ems_unavailable
- 502 ensure360_ems_invalid_response
- 502 ensure360_ems_error
- 502 core_resources_unavailable
- 502 core_resources_invalid_response
- 502 core_resources_error

## GET /v1/core/locations
- 400 ensure_community_id_invalid
- 400 ensure_community_id_missing
- 401 client_certificate_invalid
- 404 core_community_mapping_missing
- 404 core_resource_not_found
- 500 api_gateway_context_missing
- 500 api_gateway_payload_version_unsupported
- 500 api_gateway_type_unsupported
- 500 client_certificate_missing
- 502 ensure360_ems_unavailable
- 502 ensure360_ems_invalid_response
- 502 ensure360_ems_error
- 502 core_resources_unavailable
- 502 core_resources_invalid_response
- 502 core_resources_error

## GET /v1/core/residents
- 400 ensure_community_id_invalid
- 400 ensure_community_id_missing
- 401 client_certificate_invalid
- 404 core_community_mapping_missing
- 404 core_resource_not_found
- 500 api_gateway_context_missing
- 500 api_gateway_payload_version_unsupported
- 500 api_gateway_type_unsupported
- 500 client_certificate_missing
- 502 ensure360_ems_unavailable
- 502 ensure360_ems_invalid_response
- 502 ensure360_ems_error
- 502 core_resources_unavailable
- 502 core_resources_invalid_response
- 502 core_resources_error

## GET /v1/core/residents/{id}/photo
- 400 ensure_community_id_invalid
- 400 ensure_community_id_missing
- 401 client_certificate_invalid
- 404 core_community_mapping_missing
- 404 core_resource_not_found
- 500 api_gateway_context_missing
- 500 api_gateway_payload_version_unsupported
- 500 api_gateway_type_unsupported
- 500 client_certificate_missing
- 502 ensure360_ems_unavailable
- 502 ensure360_ems_invalid_response
- 502 ensure360_ems_error
- 502 core_resources_unavailable
- 502 core_resources_invalid_response
- 502 core_resources_error

## GET /v1/events
- 400 ensure_community_id_invalid
- 400 ensure_community_id_missing
- 400 payload_types_missing
- 401 client_certificate_invalid
- 500 api_gateway_context_missing
- 500 api_gateway_payload_version_unsupported
- 500 api_gateway_type_unsupported
- 500 client_certificate_missing
- 502 events_repo_error
