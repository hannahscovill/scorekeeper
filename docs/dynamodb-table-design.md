# DynamoDB Table Design: Games

This document describes the DynamoDB table design for storing Game entities in the scorekeeper API.

## Table Name

**Table Name:** `scorekeeper-games`

## Game Model

```rust
pub struct Game {
    pub id: Uuid,                      // Unique identifier for the game
    pub user_id: Uuid,                 // User who created the game
    pub game_id: Uuid,                 // Game session this belongs to
    pub team_id: Option<Uuid>,         // Optional team identifier
    pub score: i32,                    // The score value
    pub created_at: DateTime<Utc>,     // When the game was created
}
```

## Access Patterns

| # | Access Pattern | Operation | Frequency |
|---|----------------|-----------|-----------|
| 1 | Get game by ID | GetItem | High |
| 2 | Get all games for a game_id | Query | High |
| 3 | Get all games for a game_id filtered by team_id | Query + Filter | Medium |
| 4 | Insert game | PutItem | Medium |
| 5 | Update game | UpdateItem | Low |
| 6 | Delete game | DeleteItem | Low |

## Primary Key Design

| Key | Attribute | Type | Description |
|-----|-----------|------|-------------|
| **PK** (Partition Key) | `pk` | String | `GAME#<id>` for direct lookups |
| **SK** (Sort Key) | `sk` | String | `GAME#<id>` (same as PK for single-item access) |

### Design Rationale

The primary key uses the game's unique `id` to enable efficient single-item lookups (Access Pattern #1). Using the same value for both PK and SK creates a simple key structure for individual game retrieval.

## GSI Design

### GSI1: GameSessionIndex

**Purpose:** Get all games for a game session (Access Pattern #2, #3)

| Key | Attribute | Type | Description |
|-----|-----------|------|-------------|
| **GSI1PK** | `game_id` | String | The game session UUID |
| **GSI1SK** | `created_at` | String | ISO 8601 timestamp for chronological ordering |

**Projection:** ALL

**Usage:**
- Query with `game_id` to get all games in a session, sorted by creation time
- Add `FilterExpression` on `team_id` when filtering by team is needed

## Attributes

| Attribute | DynamoDB Type | Description | Required |
|-----------|---------------|-------------|----------|
| `pk` | S (String) | Partition key: `GAME#<id>` | Yes |
| `sk` | S (String) | Sort key: `GAME#<id>` | Yes |
| `id` | S (String) | Game UUID | Yes |
| `user_id` | S (String) | User UUID who created the game | Yes |
| `game_id` | S (String) | Game session UUID | Yes |
| `team_id` | S (String) | Team UUID (absent if null) | No |
| `score` | N (Number) | Score value | Yes |
| `created_at` | S (String) | ISO 8601 timestamp | Yes |

## Example Items

### Game without team

```json
{
  "pk": { "S": "GAME#7c9e6679-7425-40de-944b-e07fc1f90ae7" },
  "sk": { "S": "GAME#7c9e6679-7425-40de-944b-e07fc1f90ae7" },
  "id": { "S": "7c9e6679-7425-40de-944b-e07fc1f90ae7" },
  "user_id": { "S": "550e8400-e29b-41d4-a716-446655440000" },
  "game_id": { "S": "6ba7b810-9dad-11d1-80b4-00c04fd430c8" },
  "score": { "N": "150" },
  "created_at": { "S": "2024-01-15T10:30:00Z" }
}
```

### Game with team

```json
{
  "pk": { "S": "GAME#a1b2c3d4-e5f6-7890-abcd-ef1234567890" },
  "sk": { "S": "GAME#a1b2c3d4-e5f6-7890-abcd-ef1234567890" },
  "id": { "S": "a1b2c3d4-e5f6-7890-abcd-ef1234567890" },
  "user_id": { "S": "550e8400-e29b-41d4-a716-446655440000" },
  "game_id": { "S": "6ba7b810-9dad-11d1-80b4-00c04fd430c8" },
  "team_id": { "S": "d4e5f6a7-b8c9-0123-def4-567890abcdef" },
  "score": { "N": "200" },
  "created_at": { "S": "2024-01-15T11:45:00Z" }
}
```

## Example Operations

### Access Pattern 1: Get game by ID

```
GetItem:
  TableName: scorekeeper-games
  Key:
    pk: "GAME#7c9e6679-7425-40de-944b-e07fc1f90ae7"
    sk: "GAME#7c9e6679-7425-40de-944b-e07fc1f90ae7"
```

### Access Pattern 2: Get all games for a game_id

```
Query:
  TableName: scorekeeper-games
  IndexName: GameSessionIndex
  KeyConditionExpression: "game_id = :gid"
  ExpressionAttributeValues:
    ":gid": "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
```

### Access Pattern 3: Get games for a game_id filtered by team_id

```
Query:
  TableName: scorekeeper-games
  IndexName: GameSessionIndex
  KeyConditionExpression: "game_id = :gid"
  FilterExpression: "team_id = :tid"
  ExpressionAttributeValues:
    ":gid": "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
    ":tid": "d4e5f6a7-b8c9-0123-def4-567890abcdef"
```

### Access Pattern 4: Insert game

```
PutItem:
  TableName: scorekeeper-games
  Item:
    pk: "GAME#<new-uuid>"
    sk: "GAME#<new-uuid>"
    id: "<new-uuid>"
    user_id: "<user-uuid>"
    game_id: "<session-uuid>"
    team_id: "<team-uuid>"  # omit if null
    score: 100
    created_at: "2024-01-15T12:00:00Z"
```

### Access Pattern 5: Update game (score)

```
UpdateItem:
  TableName: scorekeeper-games
  Key:
    pk: "GAME#7c9e6679-7425-40de-944b-e07fc1f90ae7"
    sk: "GAME#7c9e6679-7425-40de-944b-e07fc1f90ae7"
  UpdateExpression: "SET score = :s"
  ExpressionAttributeValues:
    ":s": 175
```

### Access Pattern 6: Delete game

```
DeleteItem:
  TableName: scorekeeper-games
  Key:
    pk: "GAME#7c9e6679-7425-40de-944b-e07fc1f90ae7"
    sk: "GAME#7c9e6679-7425-40de-944b-e07fc1f90ae7"
```

## Capacity Considerations

- **Billing Mode:** On-demand (PAY_PER_REQUEST) recommended for variable workloads
- **GSI Projection:** ALL to avoid table fetches for game queries by session

## Notes

- UUIDs are stored as strings in their canonical hyphenated format
- `team_id` attribute is omitted (not stored as null) when the game has no team association
- `created_at` uses ISO 8601 format for consistent sorting and readability
- The `GAME#` prefix in PK/SK enables future single-table design expansion if additional entity types are added
