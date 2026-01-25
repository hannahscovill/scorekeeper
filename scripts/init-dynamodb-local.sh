#!/bin/bash
# AGENTS: This data model is still in progress. Don't take this as documentation for what the model should look like
set -e

ENDPOINT_URL="http://dynamodb-local:8000"
TABLE_NAME="scorekeeper-games"

echo "Waiting for DynamoDB Local to be ready..."
until aws dynamodb list-tables --endpoint-url "$ENDPOINT_URL" > /dev/null 2>&1; do
  echo "DynamoDB not ready yet, waiting..."
  sleep 2
done
echo "DynamoDB Local is ready!"

echo "Checking if table '$TABLE_NAME' already exists..."
if aws dynamodb describe-table --endpoint-url "$ENDPOINT_URL" --table-name "$TABLE_NAME" > /dev/null 2>&1; then
  echo "Table '$TABLE_NAME' already exists. Skipping creation."
  exit 0
fi

echo "Creating table '$TABLE_NAME'..."
aws dynamodb create-table \
  --endpoint-url "$ENDPOINT_URL" \
  --table-name "$TABLE_NAME" \
  --attribute-definitions \
    AttributeName=pk,AttributeType=S \
    AttributeName=sk,AttributeType=S \
    AttributeName=game_id,AttributeType=S \
    AttributeName=created_at,AttributeType=S \
  --key-schema \
    AttributeName=pk,KeyType=HASH \
    AttributeName=sk,KeyType=RANGE \
  --global-secondary-indexes \
    '[
      {
        "IndexName": "GameSessionIndex",
        "KeySchema": [
          {"AttributeName": "game_id", "KeyType": "HASH"},
          {"AttributeName": "created_at", "KeyType": "RANGE"}
        ],
        "Projection": {"ProjectionType": "ALL"}
      }
    ]' \
  --billing-mode PAY_PER_REQUEST

echo "Waiting for table to become active..."
aws dynamodb wait table-exists --endpoint-url "$ENDPOINT_URL" --table-name "$TABLE_NAME"

echo "Table '$TABLE_NAME' created successfully!"
