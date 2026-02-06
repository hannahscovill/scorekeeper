#!/bin/bash
# Seed puzzle answers for a 2-week window (1 week ago to 1 week ahead)
set -e

# Default to local DynamoDB
ENDPOINT_URL="${AWS_ENDPOINT_URL_DYNAMODB:-http://localhost:8000}"
TABLE_NAME="${DYNAMODB_TABLE_NAME:-scorekeeper-games}"
REMOTE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --remote)
      REMOTE=true
      ENDPOINT_URL=""
      shift
      ;;
    --endpoint)
      ENDPOINT_URL="$2"
      shift 2
      ;;
    --table)
      TABLE_NAME="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      echo "Usage: $0 [--remote] [--endpoint URL] [--table NAME]"
      exit 1
      ;;
  esac
done

# Build endpoint flag
ENDPOINT_FLAG=""
if [ -n "$ENDPOINT_URL" ]; then
  ENDPOINT_FLAG="--endpoint-url $ENDPOINT_URL"
fi

echo "Seeding puzzle answers..."
echo "  Table: $TABLE_NAME"
if [ "$REMOTE" = true ]; then
  echo "  Target: AWS (remote)"
else
  echo "  Target: $ENDPOINT_URL"
fi

# Curated 5-letter words for puzzle answers
WORDS=(
  "crane" "slate" "audio" "adieu" "story"
  "arose" "canoe" "about" "above" "acute"
  "actor" "adapt" "admit" "adopt" "adult"
)

# Generate dates dynamically: 1 week ago to 1 week ahead
if [[ "$OSTYPE" == "darwin"* ]]; then
  current=$(date -v-7d "+%s")
  end=$(date -v+7d "+%s")
else
  current=$(date -d "-7 days" "+%s")
  end=$(date -d "+7 days" "+%s")
fi

# Build batch write request (up to 25 items per batch)
ITEMS=""
index=0
while [ $current -le $end ]; do
  if [[ "$OSTYPE" == "darwin"* ]]; then
    date_str=$(date -j -f "%s" "$current" "+%Y-%m-%d")
  else
    date_str=$(date -d "@$current" "+%Y-%m-%d")
  fi

  word="${WORDS[$index]}"
  pk="PUZZLE#${date_str}"

  echo "  $date_str -> $word"

  # Add comma separator if not first item
  if [ -n "$ITEMS" ]; then
    ITEMS="$ITEMS,"
  fi

  ITEMS="$ITEMS{\"PutRequest\":{\"Item\":{\"pk\":{\"S\":\"$pk\"},\"sk\":{\"S\":\"ANSWER\"},\"word\":{\"S\":\"$word\"},\"puzzle_date\":{\"S\":\"$date_str\"}}}}"

  current=$((current + 86400))
  index=$((index + 1))
done

# Single batch write call
echo ""
echo "Writing batch..."
aws dynamodb batch-write-item \
  $ENDPOINT_FLAG \
  --request-items "{\"$TABLE_NAME\":[$ITEMS]}"

echo ""
echo "Seeding complete! Added $index puzzle answers in one batch."
