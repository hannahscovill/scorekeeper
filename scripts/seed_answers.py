#!/usr/bin/env python3
"""
Seed puzzle answers from a CSV file into DynamoDB.

CSV format:
    date,word
    2026-01-30,crane
    2026-01-31,slate

Usage:
    python scripts/seed_answers.py answers.csv
    python scripts/seed_answers.py answers.csv --table scorekeeper-prod
    python scripts/seed_answers.py answers.csv --dry-run
"""

import argparse
import csv
import sys
from datetime import datetime

import boto3
from botocore.exceptions import ClientError


def parse_date(date_str: str) -> str:
    """Parse and validate date string, return in YYYY-MM-DD format."""
    try:
        parsed = datetime.strptime(date_str.strip(), "%Y-%m-%d")
        return parsed.strftime("%Y-%m-%d")
    except ValueError:
        # Try alternative formats
        for fmt in ["%m/%d/%Y", "%m-%d-%Y", "%d/%m/%Y"]:
            try:
                parsed = datetime.strptime(date_str.strip(), fmt)
                return parsed.strftime("%Y-%m-%d")
            except ValueError:
                continue
        raise ValueError(f"Invalid date format: {date_str}")


def validate_word(word: str) -> str:
    """Validate and normalize word."""
    word = word.strip().lower()
    if len(word) != 5:
        raise ValueError(f"Word must be 5 letters, got {len(word)}: {word}")
    if not word.isalpha():
        raise ValueError(f"Word must contain only letters: {word}")
    return word


def load_csv(filepath: str) -> list[tuple[str, str]]:
    """Load and validate CSV file, return list of (date, word) tuples."""
    answers = []

    with open(filepath, "r", newline="", encoding="utf-8") as f:
        reader = csv.DictReader(f)

        # Check for required columns
        if not reader.fieldnames:
            raise ValueError("CSV file is empty")

        fieldnames_lower = [fn.lower() for fn in reader.fieldnames]
        if "date" not in fieldnames_lower or "word" not in fieldnames_lower:
            raise ValueError("CSV must have 'date' and 'word' columns")

        # Find actual column names (case-insensitive)
        date_col = reader.fieldnames[fieldnames_lower.index("date")]
        word_col = reader.fieldnames[fieldnames_lower.index("word")]

        for i, row in enumerate(reader, start=2):  # Start at 2 (header is row 1)
            try:
                date = parse_date(row[date_col])
                word = validate_word(row[word_col])
                answers.append((date, word))
            except ValueError as e:
                print(f"Warning: Skipping row {i}: {e}", file=sys.stderr)

    return answers


def create_item(date: str, word: str) -> dict:
    """Create DynamoDB item for a puzzle answer."""
    return {
        "pk": {"S": f"PUZZLE#{date}"},
        "sk": {"S": "ANSWER"},
        "puzzle_date": {"S": date},
        "word": {"S": word},
    }


def seed_answers(
    answers: list[tuple[str, str]],
    table_name: str,
    region: str = "us-west-2",
    dry_run: bool = False,
) -> tuple[int, int]:
    """
    Seed answers to DynamoDB using batch writes.

    Returns (success_count, error_count).
    """
    if dry_run:
        print(f"\n[DRY RUN] Would write {len(answers)} items to {table_name}:")
        for date, word in answers[:10]:
            print(f"  {date}: {word}")
        if len(answers) > 10:
            print(f"  ... and {len(answers) - 10} more")
        return len(answers), 0

    dynamodb = boto3.client("dynamodb", region_name=region)

    success_count = 0
    error_count = 0

    # Batch write in groups of 25 (DynamoDB limit)
    batch_size = 25
    for i in range(0, len(answers), batch_size):
        batch = answers[i : i + batch_size]

        request_items = {
            table_name: [
                {"PutRequest": {"Item": create_item(date, word)}}
                for date, word in batch
            ]
        }

        try:
            response = dynamodb.batch_write_item(RequestItems=request_items)

            # Handle unprocessed items (retry once)
            unprocessed = response.get("UnprocessedItems", {})
            if unprocessed:
                print(f"Retrying {len(unprocessed.get(table_name, []))} unprocessed items...")
                dynamodb.batch_write_item(RequestItems=unprocessed)

            success_count += len(batch)
            print(f"  Written {success_count}/{len(answers)} items...")

        except ClientError as e:
            print(f"Error writing batch: {e}", file=sys.stderr)
            error_count += len(batch)

    return success_count, error_count


def main():
    parser = argparse.ArgumentParser(
        description="Seed puzzle answers from CSV to DynamoDB"
    )
    parser.add_argument("csv_file", help="Path to CSV file with date,word columns")
    parser.add_argument(
        "--table",
        default="scorekeeper-prod",
        help="DynamoDB table name (default: scorekeeper-prod)",
    )
    parser.add_argument(
        "--region",
        default="us-west-2",
        help="AWS region (default: us-west-2)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Validate CSV and show what would be written without actually writing",
    )

    args = parser.parse_args()

    print(f"Loading answers from {args.csv_file}...")
    try:
        answers = load_csv(args.csv_file)
    except FileNotFoundError:
        print(f"Error: File not found: {args.csv_file}", file=sys.stderr)
        sys.exit(1)
    except ValueError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

    if not answers:
        print("No valid answers found in CSV")
        sys.exit(1)

    print(f"Found {len(answers)} valid answers")

    # Check for duplicates
    dates = [date for date, _ in answers]
    if len(dates) != len(set(dates)):
        from collections import Counter
        dupes = [date for date, count in Counter(dates).items() if count > 1]
        print(f"Warning: Duplicate dates found: {dupes}", file=sys.stderr)

    success, errors = seed_answers(
        answers,
        table_name=args.table,
        region=args.region,
        dry_run=args.dry_run,
    )

    print(f"\nDone! {success} succeeded, {errors} failed")

    if errors > 0:
        sys.exit(1)


if __name__ == "__main__":
    main()
