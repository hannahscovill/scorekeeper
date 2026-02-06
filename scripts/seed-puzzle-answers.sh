#!/bin/bash
# Seed puzzle answers for Jan 1, 2026 - Feb 28, 2026
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

# 59 curated 5-letter words for puzzle answers (Jan 1 - Feb 28, 2026)
WORDS=(
  "crane" "slate" "audio" "adieu" "story"
  "arose" "canoe" "about" "above" "acute"
  "actor" "adapt" "admit" "adopt" "adult"
  "after" "again" "agent" "agree" "ahead"
  "alarm" "album" "alert" "alike" "align"
  "alive" "allow" "alone" "along" "alter"
  "angel" "anger" "angle" "angry" "apart"
  "apple" "apply" "arena" "argue" "arise"
  "armor" "array" "arrow" "asset" "avoid"
  "award" "aware" "awful" "basic" "basis"
  "beach" "begun" "being" "below" "bench"
  "berry" "birth" "black" "blade" "blame"
)

# Generate dates from Jan 1, 2026 to Feb 28, 2026
START_DATE="2026-02-01"
END_DATE="2026-02-28"

# Use date command (works on macOS and Linux)
if [[ "$OSTYPE" == "darwin"* ]]; then
  # macOS
  current=$(date -j -f "%Y-%m-%d" "$START_DATE" "+%s")
  end=$(date -j -f "%Y-%m-%d" "$END_DATE" "+%s")
else
  # Linux
  current=$(date -d "$START_DATE" "+%s")
  end=$(date -d "$END_DATE" "+%s")
fi

index=0
while [ $current -le $end ]; do
  if [[ "$OSTYPE" == "darwin"* ]]; then
    date_str=$(date -j -f "%s" "$current" "+%Y-%m-%d")
  else
    date_str=$(date -d "@$current" "+%Y-%m-%d")
  fi

  word="${WORDS[$index]}"
  pk="PUZZLE#${date_str}"
  sk="ANSWER"

  echo "  $date_str -> $word"

  # Put item into DynamoDB
  aws dynamodb put-item \
    $ENDPOINT_FLAG \
    --table-name "$TABLE_NAME" \
    --item "{
      \"pk\": {\"S\": \"$pk\"},
      \"sk\": {\"S\": \"$sk\"},
      \"word\": {\"S\": \"$word\"},
      \"puzzle_date\": {\"S\": \"$date_str\"}
    }" \
    --condition-expression "attribute_not_exists(pk)" 2>/dev/null || echo "    (already exists, skipping)"

  # Increment date by 1 day (86400 seconds)
  current=$((current + 86400))
  index=$((index + 1))
done

echo ""
echo "Seeding complete! Added ${#WORDS[@]} puzzle answers."
