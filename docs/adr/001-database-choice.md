# ADR 001: Database Choice - DynamoDB

## Status

Accepted

## Context

The Scorekeeper API needs a persistent data store to replace the current in-memory storage (`InMemoryDb` using `RwLock<HashMap<Uuid, Score>>`). The application is a game scoring API for a Wordle-like puzzle game, with the following characteristics:

- **Serverless architecture**: The API runs on AWS Lambda, deployed via AWS CDK
- **Variable traffic patterns**: Usage spikes around daily puzzle releases, with quieter periods otherwise
- **Simple access patterns**: Primarily key-value lookups with some secondary index queries (user game history, puzzle leaderboards, team memberships)
- **Single-table design feasibility**: The data model consists of well-defined entities (Game, Team, TeamMembership, Scoreboard, Puzzle) with predictable relationships

Key access patterns identified:
1. Get a user's game for a specific puzzle (enforcing one-game-per-user-per-puzzle constraint)
2. Get a user's game history sorted by completion time
3. Get puzzle leaderboard sorted by fewest moves
4. Get team details and memberships
5. Get today's puzzle by date

## Decision

We will use **Amazon DynamoDB** as the primary data store for the Scorekeeper API.

### Alternatives Considered

#### Amazon RDS (PostgreSQL/MySQL)

**Pros:**
- Familiar relational model with full SQL support
- Strong consistency by default
- Flexible querying without pre-planned access patterns
- Rich ecosystem of ORMs and tooling

**Cons:**
- Requires VPC configuration and management
- Connection pooling complexity with Lambda (cold starts, connection limits)
- Always-on pricing model regardless of usage
- Scaling requires manual intervention or Aurora Serverless (with its own trade-offs)
- Operational overhead for backups, patching, and failover configuration

#### Amazon Aurora Serverless v2

**Pros:**
- Auto-scaling relational database
- Better Lambda compatibility than traditional RDS
- Full SQL support

**Cons:**
- Still requires VPC and connection management
- Higher baseline cost than DynamoDB for low-traffic workloads
- ACU-based pricing can be unpredictable
- Cold start latency when scaling from zero
- More complex infrastructure setup

#### SQLite (embedded or via S3/EFS)

**Pros:**
- Zero operational overhead
- Simple, familiar SQL interface
- No network latency for embedded use

**Cons:**
- Not designed for concurrent writes in serverless environments
- No built-in replication or high availability
- S3-backed solutions add complexity and latency
- EFS adds cost and VPC requirements
- Not a production-ready solution for a distributed API

### Why DynamoDB

1. **Serverless-native**: DynamoDB integrates seamlessly with AWS Lambda. No VPC configuration, no connection pooling, no cold start penalties from database connections.

2. **Pay-per-request pricing**: On-demand capacity mode means we pay only for actual read/write operations. Ideal for variable traffic with quiet periods between puzzle releases.

3. **Automatic scaling**: No capacity planning or manual scaling interventions. DynamoDB handles traffic spikes during peak puzzle times automatically.

4. **Predictable performance**: Single-digit millisecond latency at any scale. Critical for responsive API endpoints.

5. **Built-in features**: TTL for automatic game expiration (1-year retention), GSIs for secondary access patterns, conditional writes for enforcing one-game-per-user-per-puzzle constraint.

6. **Operational simplicity**: Fully managed with automatic backups, encryption at rest, and multi-AZ durability by default.

7. **CDK integration**: First-class support in AWS CDK makes infrastructure-as-code straightforward.

## Consequences

### Positive

- **Reduced operational burden**: No database servers to manage, patch, or scale
- **Cost efficiency**: Pay-per-request aligns costs with actual usage
- **Simplified deployment**: No VPC configuration or NAT gateways required
- **High availability**: 99.999% SLA with automatic multi-AZ replication
- **Performance consistency**: Predictable latency regardless of scale
- **Natural constraint enforcement**: Composite primary keys enforce business rules at the database level

### Negative

- **Query pattern rigidity**: Access patterns must be designed upfront. Adding new query patterns may require new GSIs or table restructuring.
- **NoSQL learning curve**: Team members unfamiliar with DynamoDB's single-table design and key schema may need onboarding.
- **Eventual consistency**: Default reads are eventually consistent. Strongly consistent reads cost 2x and must be explicitly requested when needed.
- **Limited ad-hoc querying**: No SQL means analytics and debugging queries require alternative approaches (DynamoDB Streams to S3/Athena, or exporting to a data warehouse).
- **Item size limits**: 400KB maximum item size requires careful data modeling (though not a concern for our use case).
- **Transaction limitations**: While DynamoDB supports transactions, they're more constrained than relational databases (25 items per transaction, same region).

### Mitigations

- **Access pattern documentation**: Maintain comprehensive documentation of all access patterns and GSI purposes (see `docs/epic-planning.md`)
- **Single-table design**: Follow DynamoDB best practices with a well-documented schema to support current and anticipated access patterns
- **Strongly consistent reads where needed**: Use consistent reads for operations requiring up-to-date data
- **Future analytics path**: Plan for DynamoDB Streams export to S3 if analytics needs arise
