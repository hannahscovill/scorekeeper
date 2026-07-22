# Scorekeeper

API backend for the Scorekeeper word puzzle game.

## API Documentation

- [Current API](https://hannahscovill.github.io/scorekeeper) - Implemented endpoints
- [Planned Features](https://hannahscovill.github.io/scorekeeper/planned) - Upcoming endpoints

## Development

See [AGENTS.md](./AGENTS.md) for development guidelines.

### Recommended Installations

- [mkcert](https://github.com/FiloSottile/mkcert) - generates locally-trusted TLS certs (see [Local HTTPS Certs](#local-https-certs) below)
  ```bash
  brew install mkcert
  ```
- [Bruno](https://www.usebruno.com/) - API client for exercising endpoints during local dev
  ```bash
  brew install --cask bruno
  ```

### Running Locally

```bash
docker compose -f docker-compose.dev.yml up
```

This starts DynamoDB Local + admin UI (seeded automatically) and the API via `Dockerfile.dev` with hot reload, served at `https://localhost:8080`. The DynamoDB admin UI is at `http://localhost:8001`.

If the stack is already running in another terminal/session, bring it down first with `docker compose -f docker-compose.dev.yml down` before starting a fresh copy.

### Local HTTPS Certs

The dev server runs with TLS enabled (`certs/cert.pem` / `certs/key.pem`, mounted into the container per `docker-compose.dev.yml`). These files are gitignored and **not** checked in — each developer generates their own.

Without a proper cert, browsers and strict HTTP clients will reject `https://localhost:8080` (self-signed cert not trusted, or missing Subject Alternative Names). Fix this with `mkcert`, which installs a local CA into your system/browser trust stores so `localhost` certs are trusted automatically:

```bash
# One-time: install mkcert's local CA into your system trust store
mkcert -install

# Generate the dev cert (run from the certs/ directory)
cd certs
mkcert -cert-file cert.pem -key-file key.pem localhost 127.0.0.1 ::1
cd ..
```

Restart the stack (`docker compose -f docker-compose.dev.yml down && docker compose -f docker-compose.dev.yml up`) after generating new certs so the container picks them up. You can then curl without `-k`:

```bash
curl https://localhost:8080/health
```

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
