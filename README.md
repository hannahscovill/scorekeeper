# Scorekeeper

API backend for the Scorekeeper word puzzle game.

## API Documentation

- [Current API](https://hannahscovill.github.io/scorekeeper) - Implemented endpoints
- [Planned Features](https://hannahscovill.github.io/scorekeeper/planned) - Upcoming endpoints

## Development

See [AGENTS.md](./AGENTS.md) for development guidelines.

## DynamoDB Schema

The application uses a single-table design in DynamoDB with composite primary keys (partition key `pk` and sort key `sk`).

### Table Structure

| Entity | Partition Key (pk) | Sort Key (sk) | Attributes |
|--------|-------------------|---------------|------------|
| Game State | `USER#{user_id}#PUZZLE#{date}` | `GAME_STATE` | `user_id`, `puzzle_date`, `guesses[]`, `won`, `created_at`, `updated_at` |
| Puzzle Answer | `PUZZLE#{date}` | `ANSWER` | `puzzle_date`, `word`, `team_id` (optional) |

### Entity Details

#### Game State
Stores a user's progress on a specific puzzle.

- **pk**: `USER#{user_id}#PUZZLE#{YYYY-MM-DD}` - Combines user identity with puzzle date
- **sk**: `GAME_STATE` - Constant sort key for this entity type
- **guesses**: Array of 5-letter words the user has guessed (lowercase)
- **won**: Boolean indicating if the user solved the puzzle
- **created_at**: ISO 8601 timestamp when the game was started
- **updated_at**: ISO 8601 timestamp of the last guess

Example:
```json
{
  "pk": "USER#auth0|123456#PUZZLE#2026-02-02",
  "sk": "GAME_STATE",
  "user_id": "auth0|123456",
  "puzzle_date": "2026-02-02",
  "guesses": ["crane", "slate", "moist"],
  "won": false,
  "created_at": "2026-02-02T10:30:00Z",
  "updated_at": "2026-02-02T10:35:00Z"
}
```

#### Puzzle Answer
Stores the answer word for a specific puzzle date.

- **pk**: `PUZZLE#{YYYY-MM-DD}` - The puzzle date
- **sk**: `ANSWER` - Constant sort key for this entity type
- **word**: The 5-letter answer word (lowercase)
- **team_id**: Optional team ID for team-specific puzzles

Example:
```json
{
  "pk": "PUZZLE#2026-02-02",
  "sk": "ANSWER",
  "puzzle_date": "2026-02-02",
  "word": "manna",
  "team_id": null
}
```

### Access Patterns

| Access Pattern | Operation | Key Condition |
|----------------|-----------|---------------|
| Get user's game for a puzzle | GetItem | `pk = USER#{user_id}#PUZZLE#{date}`, `sk = GAME_STATE` |
| Get all games for a user | Scan + Filter | `begins_with(pk, USER#{user_id}#PUZZLE#)`, `sk = GAME_STATE` |
| Get puzzle answer | GetItem | `pk = PUZZLE#{date}`, `sk = ANSWER` |
| Get all puzzle answers | Scan + Filter | `begins_with(pk, PUZZLE#)`, `sk = ANSWER` |

### Caching

Puzzle answers are cached in-memory on each server instance to reduce DynamoDB reads. The cache:
- Has no automatic TTL (entries persist until server restart)
- Is updated when answers are set via the API (`PUT /puzzles`)
- Can be manually cleared via `POST /puzzles/cache/clear` (admin only)

**Important**: In a multi-instance deployment, each server has its own cache. If puzzle answers are modified directly in DynamoDB (not via the API), call the cache clear endpoint or restart all server instances to ensure consistent grading.
