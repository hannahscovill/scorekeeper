# Scorekeeper API Architecture

## 1. System Overview

The Scorekeeper API is a RESTful web service for tracking sports scores. It provides endpoints for creating, retrieving, and managing game score entries with support for user authentication and team-based filtering.

### Core Capabilities

- **Score Management**: Create and retrieve game scores with batch support
- **User Authentication**: JWT-based authentication for secure access
- **Team Support**: Optional team-based organization and filtering of scores
- **Health Monitoring**: Health check endpoint for deployment orchestration

### API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Root endpoint (hello world) |
| GET | `/health` | Health check |
| GET | `/games` | List all games |
| GET | `/games/{game_id}` | Get games for a specific game session |
| POST | `/games` | Create game scores (batch) |

---

## 2. Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Scorekeeper API                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                           HTTP Layer                                  │  │
│  │                         (Actix-web)                                   │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                   │  │
│  │  │    main.rs  │  │   Routes    │  │  Middleware │                   │  │
│  │  │  (Server)   │──│  /routes/   │──│ /middleware/│                   │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘                   │  │
│  └───────────────────────────┬──────────────────────────────────────────┘  │
│                              │                                              │
│  ┌───────────────────────────┼──────────────────────────────────────────┐  │
│  │                    Business Logic Layer                               │  │
│  │                              │                                        │  │
│  │  ┌───────────────────────────┴───────────────────────────────────┐   │  │
│  │  │                       Services                                 │   │  │
│  │  │                     /services/mod.rs                           │   │  │
│  │  │  ┌─────────────────────────────────────────────────────────┐  │   │  │
│  │  │  │                    GameService                          │  │   │  │
│  │  │  │  - create_game(user_id, game_id, GameCreate) -> Game    │  │   │  │
│  │  │  └─────────────────────────────────────────────────────────┘  │   │  │
│  │  └───────────────────────────────────────────────────────────────┘   │  │
│  └───────────────────────────┬──────────────────────────────────────────┘  │
│                              │                                              │
│  ┌───────────────────────────┼──────────────────────────────────────────┐  │
│  │                      Data Layer                                       │  │
│  │                              │                                        │  │
│  │  ┌───────────────────────────┴───────────────────────────────────┐   │  │
│  │  │                     Database                                   │   │  │
│  │  │                    /db/mod.rs                                  │   │  │
│  │  │  ┌─────────────────────┐   ┌──────────────────────────────┐   │   │  │
│  │  │  │    InMemoryDb       │   │    DynamoDB (planned)        │   │   │  │
│  │  │  │  - insert_game()    │   │    - AWS SDK integration     │   │   │  │
│  │  │  │  - get_game()       │   │                              │   │   │  │
│  │  │  │  - get_all_games()  │   │                              │   │   │  │
│  │  │  │  - delete_game()    │   │                              │   │   │  │
│  │  │  │  - get_games_by_    │   │                              │   │   │  │
│  │  │  │    game_id()        │   │                              │   │   │  │
│  │  │  └─────────────────────┘   └──────────────────────────────┘   │   │  │
│  │  └───────────────────────────────────────────────────────────────┘   │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                      Cross-Cutting Concerns                           │  │
│  │                                                                       │  │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐   │  │
│  │  │     Models      │  │     Config      │  │      Errors         │   │  │
│  │  │   /models/      │  │   config.rs     │  │  models/error.rs    │   │  │
│  │  │  - Game         │  │  - host         │  │  - AppError         │   │  │
│  │  │  - GameCreate   │  │  - port         │  │  - ErrorResponse    │   │  │
│  │  │  - GameList     │  │  - database_url │  │  - ValidationDetail │   │  │
│  │  │                 │  │  - jwt_secret   │  │                     │   │  │
│  │  └─────────────────┘  └─────────────────┘  └─────────────────────┘   │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Module Structure

```
src/
├── main.rs              # Application entry point, server initialization
├── lib.rs               # Library exports for all modules
├── config.rs            # Configuration management (env vars)
├── models/
│   ├── mod.rs           # Model exports
│   ├── game.rs          # Game, GameCreate, GameList types
│   └── error.rs         # AppError, ErrorResponse, ValidationDetail
├── db/
│   └── mod.rs           # InMemoryDb implementation
├── routes/
│   ├── mod.rs           # Route exports
│   ├── games.rs         # Game CRUD endpoints
│   └── health.rs        # Health check endpoint
├── services/
│   └── mod.rs           # GameService business logic
└── middleware/
    ├── mod.rs           # Middleware exports
    ├── auth.rs          # JWT authentication (JwtAuth, Claims)
    └── validation.rs    # Request validation utilities
```

---

## 3. Data Flow Diagram

### Request Flow: POST /games (Create Games)

```
┌──────────┐     ┌───────────────┐     ┌─────────────────┐     ┌────────────────┐
│  Client  │────▶│  Actix-web    │────▶│   JWT Auth      │────▶│   Validation   │
│          │     │   Router      │     │  (auth.rs)      │     │ (validation.rs)│
└──────────┘     └───────────────┘     └─────────────────┘     └────────────────┘
                                              │                        │
                                              │ Extract Claims         │ Validate
                                              │ (sub, team_id)         │ GameCreateList
                                              ▼                        ▼
                                       ┌─────────────────────────────────────────┐
                                       │           Route Handler                 │
                                       │         (routes/games.rs)               │
                                       │                                         │
                                       │  1. Extract user_id from JWT claims     │
                                       │  2. Create Game objects                 │
                                       │  3. Store in database                   │
                                       │  4. Return created games                │
                                       └────────────────────┬────────────────────┘
                                                           │
                                                           ▼
┌──────────┐     ┌───────────────┐     ┌─────────────────────────────────────────┐
│  Client  │◀────│  JSON         │◀────│           InMemoryDb                    │
│          │     │  Response     │     │         (db/mod.rs)                     │
└──────────┘     └───────────────┘     │                                         │
                                       │  HashMap<Uuid, Game> with RwLock        │
                                       └─────────────────────────────────────────┘
```

### Request Flow: GET /games/{game_id}

```
┌──────────┐     ┌───────────────┐     ┌─────────────────┐     ┌────────────────┐
│  Client  │────▶│  Actix-web    │────▶│   JWT Auth      │────▶│  Path & Header │
│          │     │   Router      │     │  (auth.rs)      │     │   Extraction   │
└──────────┘     └───────────────┘     └─────────────────┘     └────────────────┘
     │                                                                  │
     │ Headers:                                                        │
     │ - Authorization: Bearer <token>                                 │
     │ - team-id: <uuid> (optional)                                    │
     │                                                                  ▼
     │                                        ┌─────────────────────────────────┐
     │                                        │       Route Handler             │
     │                                        │     (routes/games.rs)           │
     │                                        │                                 │
     │                                        │  1. Validate JWT token          │
     │                                        │  2. Parse game_id from path     │
     │                                        │  3. Extract optional team_id    │
     │                                        │  4. Query database              │
     │                                        └───────────────┬─────────────────┘
     │                                                        │
     │                                                        ▼
     │                                        ┌─────────────────────────────────┐
     │                                        │        InMemoryDb               │
     │                                        │    get_games_by_game_id()       │
     │                                        │                                 │
     │                                        │  Filter by:                     │
     │                                        │  - game_id (required)           │
     │                                        │  - team_id (optional)           │
     │                                        └───────────────┬─────────────────┘
     │                                                        │
     ▼                                                        ▼
┌──────────┐                              ┌─────────────────────────────────────┐
│  Client  │◀─────────────────────────────│    JSON Response: GameList         │
│          │                              │    (Array of Game objects)         │
└──────────┘                              └─────────────────────────────────────┘
```

### Authentication Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          JWT Authentication Flow                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌────────────┐         ┌────────────────┐         ┌───────────────────┐   │
│  │   Client   │         │  Authorization │         │     JwtAuth       │   │
│  │            │────────▶│     Header     │────────▶│   validate_token  │   │
│  │            │         │ "Bearer <jwt>" │         │                   │   │
│  └────────────┘         └────────────────┘         └─────────┬─────────┘   │
│                                                              │             │
│                                                              ▼             │
│                                                    ┌─────────────────────┐ │
│                                                    │      Claims         │ │
│                                                    │  ┌───────────────┐  │ │
│                                                    │  │ sub: Uuid     │  │ │
│                                                    │  │ exp: usize    │  │ │
│                                                    │  │ iat: usize    │  │ │
│                                                    │  │ team_id: Opt  │  │ │
│                                                    │  └───────────────┘  │ │
│                                                    └─────────────────────┘ │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Key Design Decisions

### 4.1 Layered Architecture

The application follows a clean layered architecture pattern:

| Layer | Responsibility | Location |
|-------|----------------|----------|
| **HTTP/Presentation** | Request routing, response formatting | `routes/`, `main.rs` |
| **Middleware** | Cross-cutting concerns (auth, validation) | `middleware/` |
| **Business Logic** | Domain rules, service orchestration | `services/` |
| **Data Access** | Database operations, persistence | `db/` |
| **Domain Models** | Data structures, type definitions | `models/` |

**Rationale**: Separation of concerns enables independent testing, easier maintenance, and flexibility to swap implementations (e.g., database backends).

### 4.2 In-Memory Database with Pluggable Design

The current implementation uses an in-memory database (`InMemoryDb`) with `RwLock<HashMap>` for thread-safe concurrent access.

```rust
pub struct InMemoryDb {
    games: RwLock<HashMap<Uuid, Game>>,
}
```

**Rationale**:
- Enables rapid development and testing without external dependencies
- Thread-safe with `RwLock` for concurrent read/write access
- Designed for easy replacement with DynamoDB (AWS SDK already included in dependencies)

### 4.3 JWT-Based Authentication

Authentication uses JSON Web Tokens (JWT) with HS256 algorithm:

- **Claims structure**: `sub` (user ID), `exp`, `iat`, and optional `team_id`
- **Token extraction**: From `Authorization: Bearer <token>` header
- **Validation**: Signature verification, expiration checking

**Rationale**: Stateless authentication enables horizontal scaling without session storage synchronization.

### 4.4 Structured Error Responses

All errors follow a consistent JSON structure:

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable message",
    "details": [{"field": "...", "message": "..."}]  // for validation errors
  }
}
```

**Rationale**: Consistent error structure simplifies client-side error handling and debugging.

### 4.5 Batch Operations

The `POST /games` endpoint accepts an array of `GameCreate` objects for batch score submission:

```json
[
  {"score": 100, "game_id": "..."},
  {"score": 200, "game_id": "..."}
]
```

**Rationale**: Reduces network round-trips for bulk score submissions, common in gaming scenarios.

### 4.6 UUID-Based Identifiers

All entities use UUID v4 for identification:

- `Game.id`: Unique game entry identifier
- `Game.user_id`: User who created the entry (from JWT)
- `Game.game_id`: Game session identifier
- `Game.team_id`: Optional team grouping

**Rationale**: UUIDs enable distributed ID generation without coordination, essential for scalability.

---

## 5. Technology Stack

### Core Framework

| Technology | Version | Purpose |
|------------|---------|---------|
| **Rust** | 2021 Edition | Systems programming language |
| **Actix-web** | 4.x | Async web framework |
| **Tokio** | 1.x | Async runtime |

### Data & Serialization

| Technology | Version | Purpose |
|------------|---------|---------|
| **Serde** | 1.x | Serialization/deserialization |
| **serde_json** | 1.x | JSON processing |
| **chrono** | 0.4.x | Date/time handling |
| **uuid** | 1.x | UUID generation and parsing |

### Authentication & Security

| Technology | Version | Purpose |
|------------|---------|---------|
| **jsonwebtoken** | 9.x | JWT encoding/decoding |

### Observability

| Technology | Version | Purpose |
|------------|---------|---------|
| **tracing** | 0.1.x | Structured logging |
| **tracing-subscriber** | 0.3.x | Log collection and filtering |

### Database (Planned)

| Technology | Version | Purpose |
|------------|---------|---------|
| **aws-sdk-dynamodb** | 1.x | DynamoDB client |
| **aws-config** | 1.x | AWS configuration |

### Validation

| Technology | Version | Purpose |
|------------|---------|---------|
| **validator** | 0.18.x | Input validation |
| **thiserror** | 1.x | Error derivation |

---

## Future Considerations

1. **DynamoDB Integration**: Replace `InMemoryDb` with DynamoDB for persistent storage
2. **API Key Authentication**: Expand `validate_api_key` placeholder for service-to-service auth
3. **Rate Limiting**: Add middleware for request throttling
4. **Caching**: Consider Redis integration for high-traffic scenarios
5. **OpenAPI/Swagger**: Generate API documentation from code
