# Epic: Local Development with Docker Compose

## Overview

Enable local development using `docker compose up` with an optional JWT authentication bypass for easier local testing.

## Goals

- Run the entire application locally with a single command: `docker compose up`
- Provide an environment variable `BYPASS_AUTH` that when set to `true` skips JWT validation
- Maintain security by ensuring auth bypass only works in development contexts
- Support hot-reload or quick rebuild workflows for development

---

## Tasks

### Task 1: Create docker-compose.yml

**Description**: Create a Docker Compose configuration file for local development.

**Acceptance Criteria**:
- [ ] `docker-compose.yml` file exists at project root
- [ ] Port 8080 exposed and mapped to host
- [ ] Environment variables configured for local development

**Implementation Notes**:
```yaml
# docker-compose.yml structure
services:
  scorekeeper:
    build: .
    ports:
      - "8080:8080"
    environment:
      - HOST=0.0.0.0
      - PORT=8080
      - JWT_SECRET=local-development-secret
      - BYPASS_AUTH=true
      - RUST_LOG=debug
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3
```

---

### Task 2: Add BYPASS_AUTH environment variable to config

**Description**: Update the configuration module to read the `BYPASS_AUTH` environment variable.

**File**: `src/config.rs`

**Acceptance Criteria**:
- [ ] Add `bypass_auth: bool` field to Config struct
- [ ] Read `BYPASS_AUTH` from environment, defaulting to `false`
- [ ] Parse string value "true"/"false" to boolean

**Implementation Notes**:
- Default must be `false` for security
- Only string "true" (case-insensitive) should enable bypass
- Log a warning when bypass is enabled

---

### Task 3: Implement auth bypass logic in middleware

**Description**: Modify the JWT validation middleware to skip authentication when `BYPASS_AUTH=true`.

**File**: `src/middleware/auth.rs`

**Acceptance Criteria**:
- [ ] Check `BYPASS_AUTH` config before validating JWT
- [ ] When bypass enabled, create a default/mock Claims object
- [ ] Log clearly when auth is being bypassed (WARN level)
- [ ] Ensure bypass logic is clearly marked with security comments

**Implementation Notes**:
- Create a `dev_claims()` function that returns mock claims for local dev
- The mock claims should have:
  - A fixed UUID for `sub` (e.g., a well-known dev user ID)
  - A far-future `exp` timestamp
  - No `team_id` (or optionally configurable)
- Add `#[cfg(debug_assertions)]` guard as additional safety layer (optional)

---

### Task 4: Add security safeguards for auth bypass

**Description**: Ensure the auth bypass cannot accidentally be enabled in production.

**Acceptance Criteria**:
- [ ] Log WARN message at startup when `BYPASS_AUTH=true`
- [ ] Consider adding environment check (e.g., require `ENVIRONMENT=local` as well)
- [ ] Document the security implications clearly

**Implementation Notes**:
- Could optionally refuse to start if `BYPASS_AUTH=true` and `ENVIRONMENT=production`
- At minimum, emit loud warnings in logs

---

### Task 5: Update Dockerfile for development mode (optional)

Human input: I like your "Alternative: use cargo-watch with volume mounts for live reload"! Lets to that.

**Description**: Consider adding a development-optimized Dockerfile or build target.

**File**: `Dockerfile` or `Dockerfile.dev`

**Acceptance Criteria**:
- [ ] Evaluate if separate dev Dockerfile needed
- [ ] If yes, create `Dockerfile.dev` with:
  - Faster builds (skip release optimizations)
  - Debug symbols included
  - cargo-watch for hot reload (optional)

**Implementation Notes**:
- Current Dockerfile uses release build which is slow
- Dev builds could use `cargo build` instead of `cargo build --release`
- Alternative: use cargo-watch with volume mounts for live reload

---

### Task 6: Add .env.example file

Human input: let's keep it simple. for now lets keep these envs in just the local docker-compose.yml

**Description**: Create an example environment file for local development.

**File**: `.env.example`

**Acceptance Criteria**:
- [ ] Document all environment variables
- [ ] Include safe default values for local development
- [ ] Add comments explaining each variable

**Content**:
```bash
# Scorekeeper Local Development Environment
HOST=0.0.0.0
PORT=8080
JWT_SECRET=local-development-secret-do-not-use-in-production
BYPASS_AUTH=true
RUST_LOG=debug
```

---

### Task 7: Update docker-compose.yml to use .env file

Human input: let's keep it simple. for now lets keep these envs in just the local docker-compose.yml

**Description**: Configure Docker Compose to automatically load `.env` file.

**Acceptance Criteria**:
- [ ] Add `env_file` directive to docker-compose.yml
- [ ] Ensure `.env` is in `.gitignore`
- [ ] Document the `.env` file usage in README

---

### Task 8: Add tests for auth bypass

Human input: let's skip this one, this bypass shouldnt be long lived

**Description**: Write tests to verify the auth bypass functionality works correctly.

**File**: `src/middleware/auth.rs` (tests module)

**Acceptance Criteria**:
- [ ] Test that requests succeed without JWT when bypass enabled
- [ ] Test that requests still require JWT when bypass disabled
- [ ] Test that mock claims are correctly populated

---

### Task 9: Update local-development.md documentation

Human input: let's skip this one. docs change too fast now while we're working extra fast

**Description**: Update the existing local development documentation to include Docker Compose instructions.

**File**: `docs/local-development.md`

**Acceptance Criteria**:
- [ ] Add "Running with Docker Compose" section
- [ ] Document `BYPASS_AUTH` environment variable
- [ ] Explain security implications
- [ ] Include troubleshooting tips

---

### Task 10: Add docker-compose healthcheck dependencies (future)

Human input: let's skip this one

**Description**: Placeholder for when additional services are added (e.g., DynamoDB Local, Redis).

**Acceptance Criteria**:
- [ ] Document how to add dependent services
- [ ] Include example configuration for DynamoDB Local
- [ ] Set up proper `depends_on` with health checks

**Implementation Notes**:
```yaml
# Future expansion example
services:
  dynamodb-local:
    image: amazon/dynamodb-local
    ports:
      - "8000:8000"
    healthcheck:
      test: ["CMD-SHELL", "curl -s http://localhost:8000"]

  scorekeeper:
    depends_on:
      dynamodb-local:
        condition: service_healthy
```

---

## Execution Order

```
┌─────────────────────────────────────────┐
│ Task 2: Add BYPASS_AUTH to config       │
└──────────────────┬──────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────┐
│ Task 3: Implement auth bypass logic     │
└──────────────────┬──────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────┐
│ Task 4: Add security safeguards         │
└──────────────────┬──────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────┐
│ Task 8: Add tests for auth bypass       │
└──────────────────┬──────────────────────┘
                   │
                   ├──────────────────────────────────┐
                   │                                  │
                   ▼                                  ▼
┌─────────────────────────────┐    ┌─────────────────────────────┐
│ Task 1: Create              │    │ Task 6: Add .env.example    │
│ docker-compose.yml          │    │                             │
└──────────────┬──────────────┘    └──────────────┬──────────────┘
               │                                  │
               ▼                                  │
┌─────────────────────────────┐                   │
│ done!                       │◄──────────────────┘
└─────────────────────────────┘


Optional/Future:
- Task 5: Development Dockerfile (can be done anytime)
- Task 10: Add dependent services (future work)
```

---

## Definition of Done

- [ ] `docker compose up` starts the application successfully
- [ ] API endpoints accessible at `http://localhost:8080`
- [ ] Health check passes at `http://localhost:8080/health`
- [ ] Requests to protected endpoints work without JWT when `BYPASS_AUTH=true`
- [ ] Requests to protected endpoints require JWT when `BYPASS_AUTH=false`
- [ ] Documentation updated
- [ ] All tests pass

---

## Security Considerations

1. **BYPASS_AUTH must default to false** - Never default to bypassing authentication
2. **Clear logging** - Always log when auth bypass is active
3. **Production safeguards** - Consider refusing to start if bypass enabled in production environment
4. **Documentation** - Clearly warn about security implications in all relevant docs
5. **Code comments** - Mark bypass code with security warnings for future maintainers
