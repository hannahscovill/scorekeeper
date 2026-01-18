# Local Development Guide

This guide covers setting up and running the Scorekeeper API locally for development.

## Prerequisites

### Required

- **Rust** (1.83 or later) - Install via [rustup](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

- **Node.js** (for npm scripts and CDK infrastructure) - Required for running the full test suite and pre-commit hooks.

### Optional

- **Docker** - For building container images
- **AWS CLI** - For deploying infrastructure

## Clone and Setup

1. Clone the repository:
   ```bash
   git clone https://github.com/hannahscovill/scorekeeper.git
   cd scorekeeper
   ```

2. Install Node.js dependencies (for pre-commit hooks and npm scripts):
   ```bash
   npm install
   ```

3. Build the project:
   ```bash
   cargo build
   ```

## Environment Variables

The server reads configuration from environment variables. All have sensible defaults for local development:

| Variable | Description | Default |
|----------|-------------|---------|
| `HOST` | Host address to bind to | `0.0.0.0` |
| `PORT` | Port to listen on | `8080` |
| `DATABASE_URL` | Database connection URL | None (uses in-memory store) |
| `JWT_SECRET` | Secret key for JWT token signing/validation | `development-secret-change-in-production` |
| `RUST_LOG` | Logging level (uses tracing) | `info` |

### Example `.env` file

Create a `.env` file in the project root (optional, for custom configuration):

```bash
HOST=127.0.0.1
PORT=3000
JWT_SECRET=my-local-dev-secret
RUST_LOG=debug
```

Note: The application does not automatically load `.env` files. Export variables manually or use a tool like `direnv`.

## Running the Server Locally

### Basic Run

```bash
cargo run
```

The server will start at `http://0.0.0.0:8080` by default.

### With Custom Port

```bash
PORT=3000 cargo run
```

### With Debug Logging

```bash
RUST_LOG=debug cargo run
```

### Release Mode (Optimized)

```bash
cargo run --release
```

## Running Tests

### All Tests (Rust + CDK)

```bash
npm test
```

### Rust Tests Only

```bash
cargo test
```

Or via npm:

```bash
npm run test:rust
```

### CDK Infrastructure Tests Only

```bash
npm run test:cdk
```

### Run Tests with Output

```bash
cargo test -- --nocapture
```

### Run Specific Test

```bash
cargo test test_health_check
```

### Run Tests in a Specific Module

```bash
cargo test routes::games::tests
```

## Useful Development Commands

### Code Formatting

Check formatting:
```bash
cargo fmt --check
# or
npm run fmt:check
```

Apply formatting:
```bash
cargo fmt
# or
npm run fmt
```

### Linting

Run Clippy linter:
```bash
cargo clippy -- -D warnings
# or
npm run lint
```

### Full Check (CI-like)

Run all checks (format, lint, test):
```bash
npm run check
```

### Build for Release

```bash
cargo build --release
```

### Build Docker Image

```bash
docker build -t scorekeeper .
```

### Run Docker Container

```bash
docker run -p 8080:8080 scorekeeper
```

## API Endpoints

Once running, the server exposes:

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/` | Hello World (basic connectivity check) |
| GET | `/health` | Health check endpoint |
| GET | `/games` | List games (placeholder) |
| GET | `/games/{game_id}` | Get games by game session ID (requires JWT) |
| POST | `/games` | Create games (requires JWT) |

### Example: Health Check

```bash
curl http://localhost:8080/health
# Response: OK
```

## Pre-commit Hooks

The project uses Husky for pre-commit hooks. After running `npm install`, hooks are automatically set up. The hooks run:

1. `cargo fmt --check` - Code formatting
2. `cargo clippy -- -D warnings` - Linting
3. `cargo test` - Unit tests

## Project Structure

```
scorekeeper/
├── src/
│   ├── main.rs          # Application entry point
│   ├── lib.rs           # Library exports
│   ├── config.rs        # Configuration management
│   ├── db/              # Database layer (in-memory store)
│   ├── middleware/      # Auth and validation middleware
│   ├── models/          # Data models (Game, Error types)
│   ├── routes/          # HTTP route handlers
│   └── services/        # Business logic services
├── tests/
│   └── integration_tests.rs  # Integration tests
├── infra/               # AWS CDK infrastructure
├── docs/                # Documentation
├── Cargo.toml           # Rust dependencies
└── package.json         # npm scripts and Node dependencies
```

## Troubleshooting

### Port Already in Use

If port 8080 is already in use:
```bash
PORT=3001 cargo run
```

### Cargo Build Fails

Ensure you have the latest Rust toolchain:
```bash
rustup update
```

### Tests Fail with JWT Errors

Tests use hardcoded test secrets. Ensure you are not setting `JWT_SECRET` environment variable when running tests, as it may conflict with test expectations.
