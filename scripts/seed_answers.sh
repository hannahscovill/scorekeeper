#!/bin/bash
#
# Seed puzzle answers from a CSV file into DynamoDB.
#
# CSV format:
#     date,word
#     2026-01-30,crane
#     2026-01-31,slate
#
# Usage:
#     ./scripts/seed_answers.sh answers.csv
#     ./scripts/seed_answers.sh answers.csv --table scorekeeper-dev
#     ./scripts/seed_answers.sh answers.csv --dry-run
#

set -euo pipefail

# Defaults
TABLE="scorekeeper-prod"
REGION="us-west-2"
DRY_RUN=false

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

usage() {
    echo "Usage: $0 <csv_file> [--table TABLE] [--region REGION] [--dry-run]"
    echo ""
    echo "Options:"
    echo "  --table    DynamoDB table name (default: scorekeeper-prod)"
    echo "  --region   AWS region (default: us-west-2)"
    echo "  --dry-run  Validate CSV and show what would be written"
    exit 1
}

log_success() { echo -e "${GREEN}✓${NC} $1"; }
log_warning() { echo -e "${YELLOW}⚠${NC} $1"; }
log_error() { echo -e "${RED}✗${NC} $1" >&2; }

# Parse arguments
CSV_FILE=""
while [[ $# -gt 0 ]]; do
    case $1 in
        --table)
            TABLE="$2"
            shift 2
            ;;
        --region)
            REGION="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --help|-h)
            usage
            ;;
        -*)
            log_error "Unknown option: $1"
            usage
            ;;
        *)
            if [[ -z "$CSV_FILE" ]]; then
                CSV_FILE="$1"
            else
                log_error "Unexpected argument: $1"
                usage
            fi
            shift
            ;;
    esac
done

# Validate CSV file
if [[ -z "$CSV_FILE" ]]; then
    log_error "CSV file is required"
    usage
fi

if [[ ! -f "$CSV_FILE" ]]; then
    log_error "File not found: $CSV_FILE"
    exit 1
fi

echo "Loading answers from $CSV_FILE..."
echo "Target table: $TABLE (region: $REGION)"
echo ""

# Validate word (5 letters, alphabetic)
validate_word() {
    local word="$1"
    word=$(echo "$word" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')

    if [[ ${#word} -ne 5 ]]; then
        echo ""
        return
    fi

    if [[ ! "$word" =~ ^[a-z]+$ ]]; then
        echo ""
        return
    fi

    echo "$word"
}

# Validate date (YYYY-MM-DD format)
validate_date() {
    local date="$1"
    date=$(echo "$date" | tr -d '[:space:]')

    # Try to parse as YYYY-MM-DD
    if [[ "$date" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
        echo "$date"
        return
    fi

    # Try MM/DD/YYYY
    if [[ "$date" =~ ^([0-9]{1,2})/([0-9]{1,2})/([0-9]{4})$ ]]; then
        printf "%s-%02d-%02d" "${BASH_REMATCH[3]}" "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}"
        return
    fi

    echo ""
}

# Read and validate CSV
declare -a DATES
declare -a WORDS
SUCCESS_COUNT=0
SKIP_COUNT=0
LINE_NUM=0

while IFS=, read -r date word rest; do
    ((LINE_NUM++))

    # Skip header row
    if [[ $LINE_NUM -eq 1 ]]; then
        # Check if this looks like a header
        if [[ "${date,,}" == "date" ]] || [[ "${word,,}" == "word" ]]; then
            continue
        fi
    fi

    # Validate date
    validated_date=$(validate_date "$date")
    if [[ -z "$validated_date" ]]; then
        log_warning "Line $LINE_NUM: Invalid date '$date', skipping"
        ((SKIP_COUNT++))
        continue
    fi

    # Validate word
    validated_word=$(validate_word "$word")
    if [[ -z "$validated_word" ]]; then
        log_warning "Line $LINE_NUM: Invalid word '$word' (must be 5 letters), skipping"
        ((SKIP_COUNT++))
        continue
    fi

    DATES+=("$validated_date")
    WORDS+=("$validated_word")
    ((SUCCESS_COUNT++))

done < "$CSV_FILE"

echo "Found $SUCCESS_COUNT valid answers ($SKIP_COUNT skipped)"
echo ""

if [[ $SUCCESS_COUNT -eq 0 ]]; then
    log_error "No valid answers found"
    exit 1
fi

# Dry run - just show what would be written
if [[ "$DRY_RUN" == true ]]; then
    echo "[DRY RUN] Would write the following items:"
    echo ""
    for i in "${!DATES[@]}"; do
        echo "  ${DATES[$i]}: ${WORDS[$i]}"
        if [[ $i -ge 9 ]] && [[ ${#DATES[@]} -gt 10 ]]; then
            echo "  ... and $((${#DATES[@]} - 10)) more"
            break
        fi
    done
    echo ""
    log_success "Dry run complete"
    exit 0
fi

# Write to DynamoDB
echo "Writing to DynamoDB..."
WRITTEN=0
ERRORS=0

for i in "${!DATES[@]}"; do
    date="${DATES[$i]}"
    word="${WORDS[$i]}"

    item=$(cat <<EOF
{
    "pk": {"S": "PUZZLE#${date}"},
    "sk": {"S": "ANSWER"},
    "puzzle_date": {"S": "${date}"},
    "word": {"S": "${word}"}
}
EOF
)

    if aws dynamodb put-item \
        --table-name "$TABLE" \
        --item "$item" \
        --region "$REGION" 2>/dev/null; then
        ((WRITTEN++))
        # Progress indicator every 10 items
        if [[ $((WRITTEN % 10)) -eq 0 ]]; then
            echo "  Written $WRITTEN/${#DATES[@]} items..."
        fi
    else
        log_error "Failed to write: $date -> $word"
        ((ERRORS++))
    fi
done

echo ""
log_success "Done! $WRITTEN succeeded, $ERRORS failed"

if [[ $ERRORS -gt 0 ]]; then
    exit 1
fi
