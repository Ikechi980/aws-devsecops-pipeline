# Architecture

This document explains how yardi-sync works at a high level and how it guarantees eventual consistency.

## Overview

The yardi-sync service maintains one-directional synchronization from Yardi EHR to resources-api. Yardi is the source of truth, and resources-api is kept in sync with it.

The service has two independent mechanisms working together:

1. **Polling Loop** (primary) - Fetches fresh data from Yardi every N seconds and applies changes
2. **Event Consumer** (optimization) - Listens to resources-api change events to minimize API load

## Components

### State Manager
Tracks all communities with Yardi integrations and their current state. Maintains:
- List of tracked communities
- Cached resources-api data (locations and residents)
- Dirty flags (indicates when a refresh is needed)
- Last refresh timestamps
- Active failure tracking

### Polling Loop
Every `YARDI_POLL_INTERVAL_MS` (default 10 seconds):
1. For each tracked community, fetch locations and residents from Yardi (rooms only)
2. If the community is marked dirty OR hasn't been refreshed in `RESOURCES_REFRESH_INTERVAL_SECS`, fetch from resources-api
3. Compare and compute differences
4. Apply changes to resources-api in order:
   - Delete residents (they reference locations)
   - Delete locations (rooms)
   - Create/update locations (rooms)
   - Create/update residents

Every `RESOURCES_REFRESH_INTERVAL_SECS` (default 300 seconds), mark the community list dirty so the next poll cycle refreshes it from resources-api.

### Event Consumer  
Listens to resources-api change events via SQS:
- **Community events**: Add/remove from tracking, mark dirty
- **Location/Resident events**: Mark the community dirty

When a community is marked dirty, the next poll cycle will refresh its resources-api data before syncing.

### Sync Engine
Computes diffs and applies changes:
- Locations (rooms): create, update names, delete
- Residents: create, update names/rooms, delete

Handles 404 responses by marking the community dirty (race condition recovery).

### Failure Publisher
Publishes notifications to SNS when Yardi API issues are detected. Failures are deduplicated - each unique failure is only published once until resolved.

## Eventual Consistency Guarantees

The system guarantees eventual consistency through multiple layers:

### Layer 1: Polling is the Primary Mechanism
The polling loop continuously fetches fresh data from Yardi and applies changes. Even if all events are lost, polling ensures we converge to the correct state.

### Layer 2: Periodic Community List Refresh
Every `RESOURCES_REFRESH_INTERVAL_SECS`, the service refreshes its list of tracked communities from resources-api. This ensures we pick up any new communities even if we missed their creation events.

### Layer 3: Periodic resources-api Data Refresh
Each community's resources-api data is refreshed at least every `RESOURCES_REFRESH_INTERVAL_SECS`, even if no changes are detected. This provides a safety net against any state drift.

### Layer 4: Dirty Flag Recovery
When the service encounters a 404 (resource not found) while trying to update or delete, it marks the community dirty. The next poll cycle will refresh the full state, correcting any inconsistencies.

### Layer 5: Task Failure Recovery
If any internal task (poll loop, event consumer, event handler) exits unexpectedly, the entire service shuts down. The orchestration system (ECS, Kubernetes, etc.) will restart it, triggering a fresh load of all communities and resuming normal operation.

## Performance Optimization

The dirty flag approach minimizes load on resources-api:

**Without dirty flags** (100 communities, 10-second poll):
- 400 API calls every 10 seconds (200 to resources-api, 200 to Yardi)

**With dirty flags** (steady state, infrequent changes):
- Most polls: ~200 API calls (only Yardi, resources-api is cached)
- When changes occur: additional calls only for affected communities
- Safety net: full refresh every 5 minutes

## SQS Queue Configuration

The service uses a **standard (non-FIFO) queue** because:
- Message ordering is not required for correctness
- The polling loop provides eventual consistency regardless of event order
- Standard queues are cheaper and simpler

Configuration:
- Message retention: 4 days
- Visibility timeout: 30 seconds
- Dead-letter queue: Recommended

## Failure Scenarios

### Yardi API Failures
- Service logs the error, publishes notification to SNS, and continues
- Failed communities are retried on the next poll cycle
- Once Yardi is accessible again, sync resumes automatically

### resources-api Failures
- If fetching data fails, the service logs and retries next cycle
- If mutations fail, same retry behavior
- 404 responses mark the community dirty for refresh

### Event Processing Failures
- If event channel fails (closed/full), the service shuts down for restart
- Missed events are handled by periodic community list refresh
- Out-of-order events don't matter (polling provides correctness)

### Service Restarts
- On startup, loads all communities from resources-api
- Retries forever with exponential backoff until successful
- Once loaded, begins normal poll/event processing

## Data Flow

```
Yardi API ----[poll every 10s]----> yardi-sync
                                        |
                                        +--> [compare with resources-api state]
                                        |
                                        +--> [apply changes] ----> resources-api
                                        
resources-api ---[SNS/SQS events]----> yardi-sync
                                          |
                                          +--> [mark community dirty]
                                          +--> [refresh on next poll]
```

## Key Design Decisions

1. **Polling is primary, events are optimization** - Ensures we don't depend on event reliability for correctness

2. **Dirty flags minimize API load** - Only fetch when needed, with periodic safety refresh

3. **One-directional sync** - Yardi is source of truth, simplifies conflict resolution

4. **Ordered mutations** - Delete residents before locations maintains referential integrity

5. **Task failure = service shutdown** - Fail fast and restart ensures clean state recovery

6. **Standard (not FIFO) queue** - Simpler and cheaper, polling provides ordering guarantees

See the README for configuration details and deployment instructions.
