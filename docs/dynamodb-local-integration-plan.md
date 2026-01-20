# DynamoDB Local Integration Plan

## Problem Statement

When running `docker compose up` and posting a game, the game does not appear in the local DynamoDB admin tool (http://localhost:8001).

## Root Cause Analysis

The application currently uses `InMemoryDb` (in-memory HashMap) for storage instead of connecting to DynamoDB Local. Despite the Docker Compose setup correctly provisioning DynamoDB Local with the init script, the Rust application in `src/main.rs:87` initializes:

```rust
let db = web::Data::new(InMemoryDb::new());
```

This means all game data is stored in-memory within the Rust process, never touching DynamoDB Local.

**Additionally:**
- The AWS SDK dependencies are commented out in `Cargo.toml:22-23` with a note about requiring Rust 1.88+
- No `DynamoDbRepository` implementation exists
- The config doesn't read DynamoDB-specific environment variables (`AWS_ENDPOINT_URL_DYNAMODB`, `DYNAMODB_TABLE_NAME`)

---

## Phase 1: Verify DynamoDB Local Setup Script

### Task 1.1: Verify init-dynamodb-local.sh script works correctly

**Goal:** Confirm the table creation script executes successfully and creates the correct schema.

**Steps:**
1. Start just the DynamoDB services: `docker compose up dynamodb-local dynamodb-init dynamodb-admin`
2. Check dynamodb-init container logs for success/failure
3. Verify table exists via dynamodb-admin UI at http://localhost:8001
4. Verify table schema matches expected structure:
   - Table name: `scorekeeper-games`
   - Primary key: `pk` (HASH), `sk` (RANGE)
   - GSI: `GameSessionIndex` with `game_id` (HASH), `created_at` (RANGE)

**Verification commands:**
```bash
# Check table exists
aws dynamodb describe-table \
  --endpoint-url http://localhost:8000 \
  --table-name scorekeeper-games \
  --region us-east-1

# List all tables
aws dynamodb list-tables \
  --endpoint-url http://localhost:8000 \
  --region us-east-1
```

---

## Phase 2: Enable AWS SDK Dependencies

### Task 2.1: Uncomment and verify AWS SDK dependencies

**Goal:** Enable the AWS SDK for DynamoDB in Cargo.toml.

**File:** `Cargo.toml`

**Changes:**
```toml
# Uncomment these lines:
aws-sdk-dynamodb = "1"
aws-config = { version = "1", features = ["behavior-version-latest"] }
```

**Note:** The comment mentions Rust 1.88+ requirement. Verify current Rust version with `rustc --version` and upgrade if needed.

---

## Phase 3: Implement DynamoDB Repository

### Task 3.1: Add DynamoDB configuration to Config struct

**File:** `src/config.rs`

**Add fields:**
- `dynamodb_endpoint_url: Option<String>` - For local development (`AWS_ENDPOINT_URL_DYNAMODB`)
- `dynamodb_table_name: String` - Table name (`DYNAMODB_TABLE_NAME`)

### Task 3.2: Create DynamoDB repository implementation

**File:** `src/db/dynamodb.rs` (new file)

**Implement:**
- `DynamoDbRepository` struct holding SDK client and table name
- `impl GameDatabase for DynamoDbRepository` with all trait methods:
  - `insert_game()` - PutItem with pk/sk structure
  - `get_game()` - GetItem by id
  - `get_all_games()` - Scan (or Query if user-scoped)
  - `delete_game()` - DeleteItem
  - `get_games_by_game_id()` - Query using GameSessionIndex GSI

**Key schema design:**
- `pk`: `USER#{user_id}` or `GAME#{id}` depending on access pattern
- `sk`: `GAME#{id}` or `CREATED#{timestamp}`
- GSI `GameSessionIndex`: `game_id` + `created_at` for querying by game session

### Task 3.3: Update db module exports

**File:** `src/db/mod.rs`

**Add:**
```rust
pub mod dynamodb;
pub use dynamodb::DynamoDbRepository;
```

---

## Phase 4: Wire Up DynamoDB in Main

### Task 4.1: Update main.rs to use DynamoDB

**File:** `src/main.rs`

**Changes:**
1. Add initialization logic to create DynamoDB client with endpoint URL override for local
2. Replace `InMemoryDb::new()` with `DynamoDbRepository::new(client, table_name)`
3. Keep InMemoryDb as fallback for testing (feature flag or env check)

**Logic:**
```rust
let db: Box<dyn GameDatabase> = if let Some(endpoint) = config.dynamodb_endpoint_url() {
    // Local development - use DynamoDB Local
    Box::new(DynamoDbRepository::new_with_endpoint(endpoint, config.dynamodb_table_name()))
} else if config.dynamodb_table_name().is_some() {
    // Production - use real DynamoDB with default credentials
    Box::new(DynamoDbRepository::new(config.dynamodb_table_name()))
} else {
    // Fallback to in-memory for tests
    Box::new(InMemoryDb::new())
};
```

### Task 4.2: Update route handlers for trait object

**Files:** `src/routes/games.rs`

**Change:** Update handlers to accept `web::Data<Box<dyn GameDatabase>>` instead of `web::Data<InMemoryDb>`

---

## Phase 5: End-to-End Testing

### Task 5.1: Manual integration test

1. `docker compose up`
2. Wait for all services healthy
3. POST a game: `curl -k -X POST https://localhost:8080/games -H "Content-Type: application/json" -d '[{"score": 100}]'`
4. Check dynamodb-admin at http://localhost:8001 - game should appear
5. GET games: `curl -k https://localhost:8080/games/{game_id}` - should return the game

### Task 5.2: Add integration tests

**File:** `tests/integration_tests.rs`

- Add DynamoDB-backed integration tests using testcontainers or docker-compose for CI

---

## Dependency Order

```
Phase 1 (Verify Setup)
    ↓
Phase 2 (Enable SDK)
    ↓
Phase 3.1 (Config) → Phase 3.2 (Repository) → Phase 3.3 (Exports)
    ↓
Phase 4.1 (Wire main) → Phase 4.2 (Update routes)
    ↓
Phase 5 (Testing)
```

---

## Environment Variables Reference

| Variable | Docker Compose Value | Description |
|----------|---------------------|-------------|
| `AWS_ENDPOINT_URL_DYNAMODB` | `http://dynamodb-local:8000` | DynamoDB endpoint (local override) |
| `AWS_ACCESS_KEY_ID` | `local` | AWS credentials (dummy for local) |
| `AWS_SECRET_ACCESS_KEY` | `local` | AWS credentials (dummy for local) |
| `AWS_REGION` | `us-east-1` | AWS region |
| `DYNAMODB_TABLE_NAME` | `scorekeeper-games` | Table name |

---

## Files to Modify

| File | Action |
|------|--------|
| `Cargo.toml` | Uncomment AWS SDK dependencies |
| `src/config.rs` | Add DynamoDB config fields |
| `src/db/mod.rs` | Add dynamodb module export |
| `src/db/dynamodb.rs` | **NEW** - DynamoDB repository implementation |
| `src/main.rs` | Wire up DynamoDB client |
| `src/routes/games.rs` | Use trait object instead of concrete type |

---

## Risk Considerations

1. **Rust version requirement:** AWS SDK may require newer Rust. Check `rustc --version` first.
2. **Breaking change to routes:** Switching from concrete `InMemoryDb` to `dyn GameDatabase` may require refactoring.
3. **Schema mismatch:** Ensure Game model serialization matches DynamoDB attribute expectations.
