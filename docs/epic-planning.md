# Epic Planning: DynamoDB Persistence & Documentation

## Epic: Data Persistence (DynamoDB)

**Priority:** P2
**Status:** Open
**Description:** Replace in-memory storage with DynamoDB for production persistence

### Current State
- `InMemoryDb` in `src/db/mod.rs` with `RwLock<HashMap<Uuid, Score>>`
- Methods: `insert_score`, `get_score`, `get_all_scores`, `delete_score`, `get_scores_by_game`
- Data lost on restart

### Access Patterns Needed
- Get game by ID
- Get user's game history (sorted by completion time)
- Get puzzle leaderboard (sorted by fewest moves)
- Get team by ID
- Get user's teams
- Get team members
- Get team's scoreboards
- Enforce one game per user per puzzle
- Get puzzle by ID
- Get today's puzzle (by date)

### Tasks

| # | Task | Type | Priority | Description | Depends On |
|---|------|------|----------|-------------|------------|
| 1 | Define database trait abstraction | task | P2 | Create `ScoreDatabase` trait with async methods to abstract storage layer | 9 |
| 2 | DynamoDB table design | task | P2 | Design PK/SK, GSIs for access patterns | 9 |
| 3 | Add AWS SDK dependencies | task | P2 | Add `aws-sdk-dynamodb`, `aws-config` to Cargo.toml | 9 |
| 4 | Implement DynamoDB client | task | P2 | Create `DynamoDbClient` implementing the database trait | 1, 3, 9 |
| 5 | CDK DynamoDB table stack | task | P2 | Add DynamoDB table to infrastructure with GSIs | 2, 9 |
| 6 | Update route handlers for async | task | P2 | Refactor handlers to use async database trait | 1, 4, 9 |
| 7 | Error mapping | task | P2 | Map DynamoDB errors to `AppError` types | 4 |
| 8 | Add integration tests | task | P2 | Tests with DynamoDB Local or LocalStack | 4, 5, 9 |
| 9 | Update score to game in domain model | task | P2 |  | - |

### Proposed DynamoDB Schema

**Single-table design** holding 5 entities: Game, Team, TeamMembership, Scoreboard, Puzzle

#### Entity Definitions

```typescript
interface Move {
  letter: string & { length: 1 }
  letter_contained_in_answer: boolean
  correct_letter_and_position: boolean
}
type Guess = [Move, Move, Move, Move, Move]

interface Game {
  game_id: string          // uuid
  user_id: string          // uuid
  puzzle_id: string        // uuid (e.g., "wordle-2026-01-18")
  moves: [Guess, Guess?, Guess?, Guess?, Guess?, Guess?] // 1-6 guesses
  moves_qty: number        // count of guesses
  completed_at_millis: number
  won: boolean
}

interface Team {
  team_id: string          // uuid
  team_name: string
  created_at_millis: number
}

interface TeamMembership {
  team_id: string
  user_id: string
  role: 'admin' | 'member'
  joined_at_millis: number
}

interface Scoreboard {
  scoreboard_id: string    // uuid
  team_id: string          // uuid
  puzzle_id: string        // which puzzle this scoreboard is for
}

interface Puzzle {
  puzzle_id: string        // uuid even though cant we just use the date? Do we really need this?
  word: string
  date_iso_day: string     // date formatted like YYYY-MM-DD
}
```

#### Access Patterns

| # | Access Pattern | Operation | Key Condition |
|---|----------------|-----------|---------------|
| 1 | Get user's game for puzzle | GetItem | PK=`USER#<user_id>#PUZZLE#<puzzle_id>` SK=`GAME` |
| 2 | Get user's games (history) | Query GSI1 | GSI1 PK=`<user_id>` |
| 3 | Get puzzle leaderboard | Query GSI2 | GSI2 PK=`<puzzle_id>` |
| 4 | Get team by ID | GetItem | PK=`TEAM#<team_id>` SK=`TEAM#<team_id>` |
| 5 | Get user's teams | Query | PK=`USER#<user_id>` SK begins_with `TEAM#` |
| 6 | Get team members | Query GSI3 | GSI3 PK=`<team_id>` |
| 7 | Get scoreboard by ID | GetItem | PK=`SCOREBOARD#<id>` SK=`SCOREBOARD#<id>` |
| 8 | Get team's scoreboards | Query | PK=`TEAM#<team_id>` SK begins_with `SCOREBOARD#` |
| 9 | Get games for scoreboard | Query GSI2 + filter | GSI2 PK=`<puzzle_id>`, filter by team members |
| 10 | Get puzzle by ID | GetItem | PK=`PUZZLE#<puzzle_id>` SK=`PUZZLE#<puzzle_id>` |
| 11 | Get puzzle by date | Query GSI4 | GSI4 PK=`PUZZLE` SK=`<date_iso_day>` |
| 12 | List recent puzzles | Query GSI4 | GSI4 PK=`PUZZLE` SK descending, limit N |

#### Table Design

```
Table: scorekeeper

Primary Key:
  PK (String) - Partition key with entity prefix
  SK (String) - Sort key with entity prefix

GSI1 - UserGamesIndex (for access pattern #2):
  PK: user_id (String) - extracted from composite PK, stored as separate attribute
  SK: completed_at_millis (Number)
  Projection: ALL
  → Enables: Get all games for a user, sorted by completion time

GSI2 - PuzzleLeaderboardIndex (for access pattern #3):
  PK: puzzle_id (String) - extracted from composite PK, stored as separate attribute
  SK: moves_qty (Number)
  Projection: ALL
  → Enables: Leaderboard sorted by fewest moves (best scores first)

GSI3 - TeamMembersIndex (for access pattern #6):
  PK: team_id (String)
  SK: user_id (String)
  Projection: KEYS_ONLY
  → Enables: List all members of a team

GSI4 - PuzzleDateIndex (for access patterns #11, #12):
  PK: entity_type (String) - always "PUZZLE" for puzzle items
  SK: date_iso_day (String) - YYYY-MM-DD format
  Projection: ALL
  → Enables: Get today's puzzle, list recent puzzles
```

#### Item Structures

| Entity | PK | SK | Attributes |
|--------|----|----|------------|
| Game | `USER#<user_id>#PUZZLE#<puzzle_id>` | `GAME` | user_id, puzzle_id, moves, moves_qty, completed_at_millis, won, ttl |
| Team | `TEAM#<team_id>` | `TEAM#<team_id>` | team_name, created_at_millis |
| TeamMembership | `USER#<user_id>` | `TEAM#<team_id>` | role, joined_at_millis, team_id (for GSI3) |
| Scoreboard | `TEAM#<team_id>` | `SCOREBOARD#<scoreboard_id>` | puzzle_id, created_at_millis |
| Puzzle | `PUZZLE#<puzzle_id>` | `PUZZLE#<puzzle_id>` | word, date_iso_day, entity_type="PUZZLE" (for GSI4) |

#### Design Decisions

**Why not embed games in scoreboard?**
- Games are small but could grow (6 guesses × 5 moves = 30 items)
- Keeping them separate allows reuse across multiple scoreboards
- Easier to query individual game stats
- DynamoDB item size limit is 400KB - embedding risks hitting this

**Why TeamMembership as separate items instead of arrays?**
- Enables "get user's teams" query without scanning
- Enables "get team members" query via GSI
- Avoids hot partition on popular teams
- Arrays in DynamoDB can't be efficiently queried

**Constraint: One game per user per puzzle**
- ✅ Option A (selected): Use composite PK `USER#<user_id>#PUZZLE#<puzzle_id>` for games
- This makes the constraint enforced by DynamoDB automatically via conditional write
- Trade-off: "Get game by ID" now requires knowing user_id + puzzle_id (or use a GSI)

**Puzzle ID: UUID vs Date?**
- Could use `date_iso_day` directly as puzzle_id (simpler, no lookup needed)
- UUID allows multiple puzzles per day in the future (e.g., hard mode, themed puzzles)
- UUID allows puzzle metadata without coupling to date
- Recommendation: Use UUID, but GSI4 enables efficient date-based lookup

**TTL for old games**
- ✅ Set to 1 year (365 days from completed_at_millis)
- Add `ttl` attribute to Game items
- DynamoDB will auto-delete expired items

---

## Epic: Documentation

**Priority:** P2
**Status:** New
**Description:** Comprehensive project documentation for development, operations, and onboarding

### Current State
- OpenAPI spec exists in `/docs/`
- `AGENTS.md` with code standards
- No architecture docs, ADRs, or runbooks

### Tasks

| # | Task | Type | Priority | Description | Depends On |
|---|------|------|----------|-------------|------------|
| 1 | Architecture overview | task | P2 | High-level system design doc with diagrams (components, data flow) | - |
| 2 | DynamoDB schema documentation | task | P2 | Table design rationale, access patterns, GSI decisions | DynamoDB task 2 |
| 3 | Local development guide | task | P2 | Setup instructions, env vars, running locally, running tests | - |
| 4 | ADR: Database choice | task | P3 | Why DynamoDB over RDS/Aurora/etc | - |
| 5 | Deployment runbook | task | P2 | CDK commands, environment setup, rollback procedures | - |
| 6 | API usage guide | task | P2 | Auth flow examples, request/response samples, pagination usage | DynamoDB task 9 |
| 7 | Update openapi docs with the updated | task | P3 | no new endpoints, we just wildly, inaccurately guessed at the entities the first time through | - |

---

## Notes

- Tasks marked P3 are lower priority / nice-to-have
- DynamoDB and Documentation epics have cross-dependencies (schema docs depend on schema design)
- Consider doing tasks 1-3 of DynamoDB epic in parallel (no dependencies)
- Documentation task 1 (architecture) and task 3 (local dev) can start immediately

---

## Decisions Made

- [x] One game per user per puzzle constraint → **Option A: composite PK** `USER#<user_id>#PUZZLE#<puzzle_id>`
- [x] TTL for old games → **Yes, 1 year**
- [x] Pagination → **Cursor-based** (DynamoDB native)
- [x] Architecture diagrams → **In docs/**
- [x] Scoreboard creation → **Manual for now**, auto-create via SNS/SQS in future epic

## Questions Still Open

- [ ] Puzzle ID: Use UUID or date string directly? (UUID recommended for flexibility)
    - let's keep the puzzle_id uuid
- [ ] GSI3 (TeamMembersIndex) - needed in v1 or defer?
    - let's keep it around
- [ ] GSI4 (PuzzleDateIndex) - needed in v1 or defer?
    - let's keep it around
