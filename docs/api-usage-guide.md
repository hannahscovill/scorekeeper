# Scorekeeper API Usage Guide

This guide provides comprehensive documentation for using the Scorekeeper API, including authentication, endpoints, request/response formats, and error handling.

## Table of Contents

1. [Authentication](#authentication)
2. [Headers](#headers)
3. [Endpoints Overview](#endpoints-overview)
4. [Endpoint Details](#endpoint-details)
5. [Error Responses](#error-responses)

---

## Authentication

The Scorekeeper API uses JWT (JSON Web Token) authentication with the HS256 algorithm.

### JWT Token Structure

Tokens must include the following claims:

| Claim | Type | Required | Description |
|-------|------|----------|-------------|
| `sub` | UUID | Yes | Subject - the user ID |
| `exp` | Integer | Yes | Expiration timestamp (Unix time) |
| `iat` | Integer | Yes | Issued at timestamp (Unix time) |
| `team_id` | UUID | No | Optional team ID for team-scoped access |

### Example JWT Payload

```json
{
  "sub": "550e8400-e29b-41d4-a716-446655440000",
  "exp": 1737302400,
  "iat": 1737298800,
  "team_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
}
```

### Using the Token

Include the JWT token in the `Authorization` header using the Bearer scheme:

```
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

### Token Validation

The API validates:
- Token signature using HS256 algorithm
- Token expiration (`exp` claim must be in the future)
- Token structure and required claims

---

## Headers

### Required Headers

| Header | Value | Description |
|--------|-------|-------------|
| `Authorization` | `Bearer <token>` | JWT authentication token (required for protected endpoints) |
| `Content-Type` | `application/json` | Required for POST requests with JSON body |

### Optional Headers

| Header | Value | Description |
|--------|-------|-------------|
| `team-id` | UUID string | Filter results by team ID (for GET requests) |

### Example Headers

```http
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
Content-Type: application/json
team-id: a1b2c3d4-e5f6-7890-abcd-ef1234567890
```

---

## Endpoints Overview

| Method | Endpoint | Description | Auth Required |
|--------|----------|-------------|---------------|
| GET | `/games` | List all games | No |
| POST | `/games` | Create games (batch) | Yes |
| GET | `/games/{game_id}` | Get games for a specific game session | Yes |

---

## Endpoint Details

### GET /games

List all games. This is a placeholder endpoint that returns an empty games array.

**Authentication:** Not required

#### Request

```http
GET /games HTTP/1.1
Host: api.example.com
```

#### Response

**Status:** `200 OK`

```json
{
  "games": []
}
```

---

### POST /games

Create one or more game entries in batch. The user ID and team ID are extracted from the JWT token.

**Authentication:** Required

#### Request

```http
POST /games HTTP/1.1
Host: api.example.com
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
Content-Type: application/json
```

#### Request Body

An array of game creation objects:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `score` | Integer (i32) | Yes | The score value (can be negative) |
| `game_id` | UUID | No | Game session ID. If not provided, a new UUID is generated |

**Example - Single game:**

```json
[
  {
    "score": 100
  }
]
```

**Example - Multiple games with same session:**

```json
[
  {
    "score": 100,
    "game_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
  },
  {
    "score": 200,
    "game_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
  },
  {
    "score": 300,
    "game_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
  }
]
```

**Example - Games with auto-generated session IDs:**

```json
[
  {
    "score": 42
  },
  {
    "score": -50
  },
  {
    "score": 0
  }
]
```

#### Response

**Status:** `201 Created`

Returns an array of created game objects:

```json
[
  {
    "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "game_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
    "team_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "score": 100,
    "created_at": "2024-01-15T10:30:00Z"
  },
  {
    "id": "8d0f7780-8536-51ef-a55c-f18fd2f01bf8",
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "game_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
    "team_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "score": 200,
    "created_at": "2024-01-15T10:30:00Z"
  }
]
```

**Note:** The `team_id` field is omitted from the response if the JWT token does not contain a `team_id` claim.

#### Validation Rules

- The game array must not be empty (returns 422 if empty)
- Score can be any i32 value (positive, negative, or zero)

---

### GET /games/{game_id}

Retrieve all game entries for a specific game session. Optionally filter by team ID.

**Authentication:** Required

#### Path Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `game_id` | UUID | The game session ID to query |

#### Request

```http
GET /games/6ba7b810-9dad-11d1-80b4-00c04fd430c8 HTTP/1.1
Host: api.example.com
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

**With team-id filter:**

```http
GET /games/6ba7b810-9dad-11d1-80b4-00c04fd430c8 HTTP/1.1
Host: api.example.com
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
team-id: a1b2c3d4-e5f6-7890-abcd-ef1234567890
```

#### Response

**Status:** `200 OK`

Returns an array of game objects matching the criteria:

```json
[
  {
    "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "game_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
    "team_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "score": 100,
    "created_at": "2024-01-15T10:30:00Z"
  },
  {
    "id": "8d0f7780-8536-51ef-a55c-f18fd2f01bf8",
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "game_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
    "team_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "score": 200,
    "created_at": "2024-01-15T10:30:00Z"
  }
]
```

**Empty result (no games found):**

```json
[]
```

---

## Error Responses

All error responses follow a consistent format:

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable error message",
    "details": []
  }
}
```

The `details` field is only present for validation errors (422).

### Error Codes

| HTTP Status | Code | Description |
|-------------|------|-------------|
| 400 | `BAD_REQUEST` | Invalid request format or parameters |
| 401 | `UNAUTHORIZED` | Missing or invalid authentication |
| 403 | `FORBIDDEN` | Insufficient permissions |
| 404 | `NOT_FOUND` | Resource not found |
| 422 | `VALIDATION_ERROR` | Request validation failed |
| 500 | `INTERNAL_ERROR` | Server error |

### Error Examples

#### 400 Bad Request - Invalid game_id format

```http
GET /games/not-a-valid-uuid HTTP/1.1
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

```json
{
  "error": {
    "code": "BAD_REQUEST",
    "message": "Invalid game_id format: not a valid UUID"
  }
}
```

#### 400 Bad Request - Invalid team-id header

```http
GET /games/6ba7b810-9dad-11d1-80b4-00c04fd430c8 HTTP/1.1
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
team-id: not-a-valid-uuid
```

```json
{
  "error": {
    "code": "BAD_REQUEST",
    "message": "Invalid team-id header: not a valid UUID"
  }
}
```

#### 401 Unauthorized - Missing token

```http
POST /games HTTP/1.1
Content-Type: application/json
```

```json
{
  "error": {
    "code": "UNAUTHORIZED",
    "message": "Missing Authorization header"
  }
}
```

#### 401 Unauthorized - Invalid or expired token

```http
POST /games HTTP/1.1
Authorization: Bearer invalid-or-expired-token
Content-Type: application/json
```

```json
{
  "error": {
    "code": "UNAUTHORIZED",
    "message": "Invalid or expired authentication token"
  }
}
```

#### 422 Validation Error - Empty game list

```http
POST /games HTTP/1.1
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
Content-Type: application/json

[]
```

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Validation failed",
    "details": [
      {
        "field": "games",
        "message": "Game list cannot be empty"
      }
    ]
  }
}
```

---

## Game Object Schema

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Unique identifier for the game entry |
| `user_id` | UUID | User who created the game (from JWT `sub` claim) |
| `game_id` | UUID | Game session identifier |
| `team_id` | UUID (optional) | Team identifier (from JWT `team_id` claim, omitted if null) |
| `score` | Integer (i32) | The score value |
| `created_at` | ISO 8601 DateTime | When the game was created |

### Example Game Object

```json
{
  "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "game_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
  "team_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "score": 150,
  "created_at": "2024-01-15T10:30:00Z"
}
```

### Example Game Object (without team_id)

```json
{
  "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "game_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
  "score": 150,
  "created_at": "2024-01-15T10:30:00Z"
}
```
